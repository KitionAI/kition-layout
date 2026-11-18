//! A page's background colour, done the InDesign way: a locked rectangle at the back of the
//! page's parent, covering the page and its bleed. Transparent (no background) is no rectangle;
//! PNG export with `background: transparent` then leaves the page clear.

use kition_layout_color::{Swatch, SwatchValue};
use kition_layout_doc::{Document, Fill, Item, ItemId, ObjectLiquid, Shape, SpreadId, SpreadRef};
use kition_layout_geom::{Rect, shapes};
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc};
use crate::{Result, Session};

/// The script label that marks a parent's background rectangle.
pub(crate) const LABEL: &str = "kition.background";

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "page.background",
        "Page Background",
        [],
        None,
        "{color: \"#rrggbb\" | swatch name | null (no background: transparent in PNG export), pages?: [1-based] (default: every page)} — a locked rectangle at the back of each page's parent (a parent is made for pages without one), covering the page and its bleed → {parents, swatch}",
        has_doc,
        page_background
    )]
}

/// The swatch a background colour names: a swatch by name, or one with the hex colour (made
/// when the document has none).
fn swatch_for(d: &mut Document, color: &str) -> Result<String> {
    const ID: &str = "page.background";
    if d.swatch(color).is_some() {
        return Ok(color.to_string());
    }
    let c = kition_layout_color::Color::from_hex(color).ok_or_else(|| bad(ID, format!("`{color}` is neither a swatch nor a #rrggbb colour")))?;
    let same = |w: &Swatch| matches!(&w.value, SwatchValue::Color { color, .. } if color.to_rgba8(1.0) == c.to_rgba8(1.0));
    if let Some(w) = d.swatches.iter().find(|w| !w.hidden && same(w)) {
        return Ok(w.name.clone());
    }
    let [r, g, b, _] = c.to_rgba8(1.0);
    let name = kition_layout_doc::Styles::unique_name(|n| d.swatch(n).is_some(), &format!("R={r} G={g} B={b}"));
    d.swatches.push(Swatch {
        name: name.clone(),
        value: SwatchValue::Color { color: c, color_type: kition_layout_color::ColorType::Process },
        locked: false,
        named: true,
        hidden: false,
    });
    Ok(name)
}

/// Whether `it` is a parent's background rectangle.
pub(crate) fn is_background(it: &Item) -> bool {
    it.label == LABEL
}

/// The swatch of document page `abs`'s background, if its parent has one.
pub(crate) fn background_of(d: &Document, abs: usize) -> Option<String> {
    let (pi, ppage) = d.parent_page_for(abs)?;
    let parent = d.parents.get(pi)?;
    let bounds = parent.pages.get(ppage)?.bounds();
    parent.items.iter().find(|it| is_background(it) && bounds.contains(it.bounds().center())).map(|it| it.fill.swatch.clone())
}

fn page_background(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "page.background";
    let color = match p.get("color") {
        Some(Value::Null) => None,
        Some(Value::String(c)) => Some(c.trim().to_string()),
        _ => return Err(bad(ID, "`color` is \"#rrggbb\", a swatch name, or null")),
    };
    let n = s.doc()?.doc.page_count();
    let pages: Vec<usize> = match p.get("pages") {
        None | Some(Value::Null) => (0..n).collect(),
        Some(Value::Array(list)) => {
            let mut out = Vec::new();
            for v in list {
                let page = v
                    .as_u64()
                    .and_then(|x| usize::try_from(x).ok())
                    .filter(|x| (1..=n).contains(x))
                    .ok_or_else(|| bad(ID, format!("no page {v}")))?;
                out.push(page - 1);
            }
            out
        }
        Some(_) => return Err(bad(ID, "`pages` is a list of 1-based page numbers")),
    };
    s.edit(|d, _| {
        let swatch = color.as_deref().map(|c| swatch_for(d, c)).transpose()?;
        // Pages without a parent get one (only when there is a colour to show).
        let bare: Vec<usize> = pages.iter().copied().filter(|abs| d.page(*abs).is_some_and(|pg| pg.parent.is_none())).collect();
        if !bare.is_empty() && swatch.is_some() {
            let (columns, gutter, margins) =
                d.page(bare[0]).map(|pg| (pg.columns.count, pg.columns.gutter, pg.margins)).ok_or_else(|| bad(ID, "no such page"))?;
            let prefix = d.next_parent_prefix();
            let id = d.add_parent(&prefix, "Parent", columns, gutter, margins);
            d.apply_parent(&bare, Some(id))?;
        }
        // Each (parent, parent page) the pages show, with the largest page size that uses it.
        let mut targets: Vec<(usize, usize, f64, f64)> = Vec::new();
        for abs in &pages {
            let (Some((pi, ppage)), Some(pg)) = (d.parent_page_for(*abs), d.page(*abs)) else { continue };
            match targets.iter_mut().find(|t| t.0 == pi && t.1 == ppage) {
                Some(t) => {
                    t.2 = t.2.max(pg.width);
                    t.3 = t.3.max(pg.height);
                }
                None => targets.push((pi, ppage, pg.width, pg.height)),
            }
        }
        let bleed = d.settings.bleed.iter().copied().fold(0.0f64, f64::max);
        let layer = d.layers.last().map(|l| l.id).unwrap_or_else(|| d.default_layer());
        let mut ids: Vec<SpreadId> = Vec::new();
        for (pi, ppage, w, h) in targets {
            let Some(parent) = d.parents.get(pi) else { continue };
            let Some(pb) = parent.pages.get(ppage).map(|pg| pg.bounds()) else { continue };
            ids.push(parent.id);
            // The old rectangle on that parent page goes.
            let old: Vec<ItemId> = parent.items.iter().filter(|it| is_background(it) && pb.contains(it.bounds().center())).map(|it| it.id).collect();
            for id in old {
                d.remove_item(id)?;
            }
            let Some(swatch) = &swatch else { continue };
            let r = Rect::new(pb.x0 - bleed, pb.y0 - bleed, pb.x0 + w.max(pb.width()) + bleed, pb.y0 + h.max(pb.height()) + bleed);
            let mut it = Item::new(ItemId(d.alloc()), layer, Shape::Rectangle, shapes::rectangle(r));
            it.name = "Background".into();
            it.label = LABEL.into();
            it.locked = true;
            it.fill = Fill { swatch: swatch.clone(), tint: 1.0, ..Fill::none() };
            // It stretches with its page when a liquid rule resizes the parent (alternate layouts).
            it.liquid =
                Some(ObjectLiquid { resize_width: true, resize_height: true, pin_top: true, pin_bottom: true, pin_left: true, pin_right: true });
            d.insert_item(SpreadRef::Parent(pi), it, Some(0))?;
        }
        ids.dedup();
        let labels: Vec<String> =
            ids.iter().filter_map(|id| d.parents.iter().find(|p| p.id == *id)).filter_map(|p| p.parent.as_ref().map(|i| i.label())).collect();
        Ok(json!({ "parents": labels, "swatch": swatch }))
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    fn background(s: &mut Session, page: usize) -> Option<String> {
        super::background_of(&s.doc().unwrap().doc, page)
    }

    #[test]
    fn a_background_is_a_locked_rectangle_on_the_parent() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 1080, "height": 1440, "pages": 2, "facingPages": false})).unwrap();
        let r = s.execute("page.background", &json!({"color": "#f8f5e8"})).unwrap();
        assert_eq!(r["parents"], json!(["A-Parent"]), "{r}");
        let swatch = r["swatch"].as_str().unwrap().to_string();
        assert_eq!(background(&mut s, 0).as_deref(), Some(swatch.as_str()));
        assert_eq!(background(&mut s, 1).as_deref(), Some(swatch.as_str()));
        let d = s.doc().unwrap().doc.clone();
        let rect = d.parents[0].items.iter().find(|it| super::is_background(it)).unwrap();
        assert!(rect.locked);
        assert_eq!(d.parents[0].items[0].id, rect.id, "at the back");
        assert!((rect.bounds().width() - 1080.0).abs() < 1e-6 && (rect.bounds().height() - 1440.0).abs() < 1e-6);
        // Again with the same colour: the swatch is reused and the rectangle replaced.
        let again = s.execute("page.background", &json!({"color": "#f8f5e8"})).unwrap();
        assert_eq!(again["swatch"], json!(swatch));
        assert_eq!(s.doc().unwrap().doc.parents[0].items.iter().filter(|it| super::is_background(it)).count(), 1);
        // A swatch by name.
        s.execute("page.background", &json!({"color": "C=100 M=0 Y=0 K=0"})).unwrap();
        assert_eq!(background(&mut s, 0).as_deref(), Some("C=100 M=0 Y=0 K=0"));
        // Transparent: no rectangle.
        s.execute("page.background", &json!({"color": null})).unwrap();
        assert_eq!(background(&mut s, 0), None);
        // A page that had no parent gets one.
        s.execute("layout.pages.applyParent", &json!({"pages": [2], "parent": null})).unwrap();
        s.execute("page.background", &json!({"color": "#102030", "pages": [2]})).unwrap();
        assert!(background(&mut s, 1).is_some());
        assert_eq!(background(&mut s, 0), None, "page 1 keeps its own parent's (none)");
        // One undo step each.
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(background(&mut s, 1), None);
    }

    #[test]
    fn png_export_shows_the_background_unless_transparent_and_bad_input_errs() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 40, "height": 30, "pages": 1, "facingPages": false})).unwrap();
        s.execute("page.background", &json!({"color": "#ff0000"})).unwrap();
        let out = s.execute("file.exportImage", &json!({"format": "png"})).unwrap();
        assert_eq!(out["files"][0]["width"], 40);
        assert!(s.execute("page.background", &json!({"color": "nope"})).is_err());
        assert!(s.execute("page.background", &json!({"color": "#00ff00", "pages": [9]})).is_err());
        assert!(s.execute("page.background", &json!({})).is_err());
    }
}
