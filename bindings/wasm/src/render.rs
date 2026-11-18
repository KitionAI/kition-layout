//! `file.render`, the kit's render command (`kit.json` `agent.render`; Kition platform rule K21,
//! KHP A24, A39): what `layout_inspect` with `preview: true` draws. It never changes the document.
//!
//! - `scale`: device pixels per point, above 0 and at most 2 (default 1);
//! - `maxSize`: at most this many pixels on the longer side, 1 to 4096;
//! - `target`: the shape of a diagnostic's target: `{ path: "item:<id>" }` draws that object (its
//!   bounds with its stroke, on its spread), `{ page }` (counted from 1) that page, `{ spread }`
//!   (counted from 0) that spread;
//! - `slide`: a page counted from 0, as the host names a part by number, when there is no `target`.
//!
//! Without a target it draws the whole canvas, the spreads on their pasteboard, exactly as
//! [`KitDocument::render`] does (the conformance goldens). It answers `{ png, width, height }`:
//! `png` is the base64 PNG, at the top of the result.

use kition_kit::{KitCommand, KitDocument, MAX_RENDER_SIDE};
use kition_layout_doc::{Document, ItemId};
use kition_layout_geom::{Affine, Rect};
use serde_json::{Value, json};

use crate::LayoutDocument;

/// The command's id, `agent.render` in `kit.json`.
pub const ID: &str = "file.render";

/// The largest `scale` and `maxSize` the platform asks for (K21).
const MAX_SCALE: f64 = 2.0;
const MAX_SIZE: u64 = 4096;

/// The command as the catalogue offers it; [`crate::describe`] adds its description, output and
/// typed parameters from `commands.json`.
pub fn command() -> KitCommand {
    KitCommand {
        id: ID.to_owned(),
        title: "Render Preview".to_owned(),
        params: Some("{scale?, maxSize?, target?, slide?}".to_owned()),
        mutates: false,
        enabled: true,
        ..Default::default()
    }
    .with_kit_schema()
}

/// The part to draw, in document pixels (the canvas the editor shows, its origin at the top left
/// of the pasteboard); `None`: the whole canvas.
fn area(doc: &LayoutDocument, params: &Value) -> Result<Option<Rect>, String> {
    let target = params.get("target").filter(|t| !t.is_null());
    let slide = params.get("slide").filter(|s| !s.is_null());
    let Some((kind, value)) = (match (target, slide) {
        (Some(target), _) => {
            if let Some(path) = target.get("path").filter(|p| !p.is_null()) {
                Some(("path", path))
            } else if let Some(page) = target.get("page").filter(|p| !p.is_null()) {
                Some(("page", page))
            } else if let Some(spread) = target.get("spread").filter(|s| !s.is_null()) {
                Some(("spread", spread))
            } else {
                return Err("`target` names an object (`path: \"item:<id>\"`), a `page` (counted from 1) or a `spread` (counted from 0)".into());
            }
        }
        (None, Some(slide)) => Some(("slide", slide)),
        (None, None) => None,
    }) else {
        return Ok(None);
    };
    let state = doc.active()?;
    let d: &Document = &state.doc;
    let layout = doc.session.layout();
    let origin = doc.origin();
    let on_canvas = |spread: kition_layout_doc::SpreadRef, rect: Rect| -> Result<Rect, String> {
        let slot = layout.slots.iter().find(|slot| slot.spread == spread).ok_or("that spread is not on the canvas (a parent spread)")?;
        let placed = slot.xf.transform_rect_bbox(rect);
        Ok(Rect::new(placed.x0 - origin.x, placed.y0 - origin.y, placed.x1 - origin.x, placed.y1 - origin.y))
    };
    let number = |value: &Value, what: &str| value.as_u64().ok_or_else(|| format!("`{what}` is a whole number"));
    let rect = match kind {
        "path" => {
            let path = value.as_str().ok_or("`target.path` is a string, `item:<id>`")?;
            let id = path
                .strip_prefix("item:")
                .and_then(|id| id.parse::<u64>().ok())
                .ok_or_else(|| format!("`target.path` {path:?} names no object: give `item:<id>`, an id layout_inspect lists"))?;
            let loc = d.find(ItemId(id)).ok_or_else(|| format!("no object has id {id} on a spread"))?;
            let spread = d.spread(loc.spread).ok_or("the object's spread is gone")?;
            let mut xf = Affine::IDENTITY;
            let mut items = &spread.items[..];
            let mut item = None;
            for (depth, index) in loc.path.iter().enumerate() {
                let found = items.get(*index).ok_or("the object's group changed")?;
                if depth + 1 < loc.path.len() {
                    xf *= found.xf;
                    items = found.children();
                }
                item = Some(found);
            }
            let item = item.ok_or("the object's group changed")?;
            on_canvas(loc.spread, xf.transform_rect_bbox(item.visible_bounds()))?
        }
        "page" | "slide" => {
            let n = number(value, if kind == "page" { "target.page" } else { "slide" })?;
            let abs = if kind == "page" { n.checked_sub(1).ok_or("`target.page` counts from 1")? } else { n };
            let abs = usize::try_from(abs).map_err(|_| "no such page")?;
            let (si, pi) = d.page_loc(abs).ok_or_else(|| format!("the layout has {} pages", d.page_count()))?;
            let page = d.spreads.get(si).and_then(|s| s.pages.get(pi)).ok_or("no such page")?;
            on_canvas(kition_layout_doc::SpreadRef::Doc(si), page.bounds())?
        }
        _ => {
            let si = usize::try_from(number(value, "target.spread")?).map_err(|_| "no such spread")?;
            let spread = d.spreads.get(si).ok_or_else(|| format!("the layout has {} spreads, counted from 0", d.spreads.len()))?;
            on_canvas(kition_layout_doc::SpreadRef::Doc(si), spread.bounds())?
        }
    };
    Ok(Some(rect))
}

/// Runs `file.render` (see the module docs).
pub fn run(doc: &LayoutDocument, params: &Value) -> Result<Value, String> {
    let scale = match params.get("scale").filter(|s| !s.is_null()) {
        None => 1.0,
        Some(v) => v.as_f64().filter(|s| s.is_finite() && *s > 0.0 && *s <= MAX_SCALE).ok_or("`scale` is a number above 0, at most 2")?,
    };
    let max_size = match params.get("maxSize").filter(|s| !s.is_null()) {
        None => u64::from(MAX_RENDER_SIDE),
        Some(v) => v.as_u64().filter(|n| (1..=MAX_SIZE).contains(n)).ok_or("`maxSize` is a whole number from 1 to 4096")?,
    };
    let max_size = max_size.min(u64::from(MAX_RENDER_SIDE)) as f64;
    let part = area(doc, params)?;
    let whole = part.is_none();
    let rect = part.unwrap_or_else(|| Rect::new(0.0, 0.0, f64::from(doc.width()), f64::from(doc.height())));
    let (w, h) = (rect.width().max(1.0), rect.height().max(1.0));
    let scale = scale.min(max_size / w.max(h));
    // The whole canvas is sized as the goldens are (truncated); a part covers all of its area.
    let side = |units: f64| -> u32 {
        let px = if whole { (units * scale).floor() } else { (units * scale).ceil() };
        px.clamp(1.0, max_size) as u32
    };
    let (width, height) = (side(w), side(h));
    let rgba = doc.render(rect.x0, rect.y0, scale, width, height)?;
    let picture = image::RgbaImage::from_raw(width, height, rgba).ok_or("the renderer answered a short image")?;
    let mut png = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut png, image::ImageFormat::Png).map_err(|error| format!("cannot encode the PNG: {error}"))?;
    Ok(json!({ "png": kition_layout_engine::cmd::base64_encode(&png.into_inner()), "width": width, "height": height }))
}
