//! Queries for agents and UIs: document structure, items, pages.

use kition_layout_doc::{Content, Item};
use serde_json::{Value, json};

use super::{CommandSpec, always, cmd, has_doc};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "file.inspect", "Inspect Document", [], None, "{} → pages, spreads, items, stories (with overset), styles, swatches, selection", has_doc, inspect),
        cmd!(query "file.list", "List Documents", [], None, "{}", always, |s, _| {
            Ok(json!(s.documents().iter().enumerate().map(|(i, d)| json!({"index": i, "title": d.title(), "dirty": d.is_dirty(), "active": Some(i) == s.active_index()})).collect::<Vec<_>>()))
        }),
        cmd!(noundo "tool.select", "Select Tool", [], None, "{tool: selection|directSelection|type|rectangleFrame|…}", always, |s, p| {
            let t = p.get("tool").and_then(Value::as_str).unwrap_or("selection");
            if kition_layout_tools::tool_info(t).is_none() && t != "placeGun" {
                return Err(super::bad("tool.select", format!("unknown tool `{t}`")));
            }
            s.set_tool(t);
            Ok(json!({"tool": s.tool_id()}))
        }),
        cmd!(query "tool.list", "List Tools", [], None, "{}", always, |_, _| Ok(serde_json::to_value(kition_layout_tools::TOOL_GROUPS).unwrap_or_default())),
        cmd!(query "file.history", "History", [], None, "{}", has_doc, |s, _| {
            let st = s.doc()?;
            let (undo, redo) = st.history_labels();
            Ok(json!({"undo": undo, "redo": redo}))
        }),
    ]
}

fn item_json(it: &Item) -> Value {
    let b = it.bounds();
    let mut v = json!({
        "id": it.id.0, "name": it.name, "kind": it.default_label(), "layer": it.layer.0,
        "bounds": [b.x0, b.y0, b.x1, b.y1], "fill": it.fill.swatch, "stroke": {"swatch": it.stroke.swatch, "weight": it.stroke.weight},
        "locked": it.locked, "hidden": it.hidden,
        // For the Properties panel: fill tint, opacity, and the top-left corner's shape and size.
        "fillTint": it.fill.tint, "opacity": it.opacity,
        "corner": {"shape": it.corners.corners[0].shape, "size": it.corners.corners[0].size},
    });
    // Text wrap (Window → Text Wrap), when the item has one.
    if it.wrap.mode != kition_layout_doc::WrapMode::None {
        v["wrap"] = json!({ "mode": it.wrap.mode, "offsets": it.wrap.offsets, "invert": it.wrap.invert, "side": it.wrap.side });
    }
    match &it.content {
        Content::Text(t) => {
            v["story"] = json!(t.story.0);
            v["columns"] = json!(t.options.columns);
            v["columnRule"] = json!(t.options.column_rule);
            if t.options.column_rule {
                v["columnRuleWeight"] = json!(t.options.column_rule_weight);
                v["columnRuleColor"] = json!(t.options.column_rule_color);
                v["columnRuleTint"] = json!(t.options.column_rule_tint);
                v["columnRuleOffset"] = json!(t.options.column_rule_offset);
                v["columnRuleTopInset"] = json!(t.options.column_rule_top_inset);
                v["columnRuleBottomInset"] = json!(t.options.column_rule_bottom_inset);
            }
        }
        Content::Graphic(g) => v["asset"] = json!(g.asset.0),
        Content::Group { items } => v["children"] = Value::Array(items.iter().map(|c| item_json(c)).collect()),
        Content::Unassigned => {}
    }
    v
}

fn swatch_json(all: &[kition_layout_doc::color::Swatch], w: &kition_layout_doc::color::Swatch) -> Value {
    use kition_layout_doc::color::SwatchValue;
    let kind = match &w.value {
        SwatchValue::None => "none",
        SwatchValue::Paper { .. } => "paper",
        SwatchValue::Registration => "registration",
        SwatchValue::Color { .. } => "color",
        SwatchValue::Tint { .. } => "tint",
        SwatchValue::Gradient { .. } => "gradient",
    };
    let hex = match &w.value {
        SwatchValue::None => None,
        SwatchValue::Paper { color } => Some(*color),
        SwatchValue::Color { color, .. } => Some(*color),
        SwatchValue::Registration => Some(kition_layout_doc::color::Color::cmyk(1.0, 1.0, 1.0, 1.0)),
        SwatchValue::Tint { base, tint } => kition_layout_doc::color::swatch::resolve(all, base, *tint),
        SwatchValue::Gradient { gradient } => gradient.stops.first().map(|stop| stop.color),
    }
    .map(|c| {
        let [r, g, b, _] = c.to_rgba8(1.0);
        format!("#{r:02x}{g:02x}{b:02x}")
    });
    json!({"name": w.name, "kind": kind, "color": hex, "locked": w.locked})
}

fn inspect(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let d = &st.doc;
    let spreads: Vec<Value> = d
        .spreads
        .iter()
        .enumerate()
        .map(|(si, sp)| {
            json!({
                "index": si,
                "pages": sp.pages.iter().enumerate().map(|(pi, p)| {
                    let abs = d.first_page_of_spread(si) + pi;
                    json!({"index": abs, "name": d.page_name(abs), "bounds": [p.x, 0.0, p.x + p.width, p.height], "side": p.side,
                        "parent": p.parent.and_then(|pid| d.parents.iter().find(|x| x.id == pid)).and_then(|x| x.parent.as_ref()).map(|i| i.label()),
                        "margins": p.margins, "columns": p.columns.count,
                        // The page's background colour (a locked rectangle on its parent; `page.background`).
                        "background": super::background::background_of(d, abs)})
                }).collect::<Vec<_>>(),
                "items": sp.items.iter().map(|i| item_json(i)).collect::<Vec<_>>(),
            })
        })
        .collect();
    let stories: Vec<Value> = d
        .stories
        .values()
        .map(|story| {
            let cs = s.cache.get(d, story.id, None);
            let preview: String = story.text.chars().take(80).collect();
            json!({"id": story.id.0, "frames": story.frames.iter().map(|f| f.0).collect::<Vec<_>>(), "length": story.len(), "paragraphs": story.paras.len(),
                "overset": cs.overset_at.is_some(), "lines": cs.line_count(), "preview": preview})
        })
        .collect();
    Ok(json!({
        "title": st.title(), "path": st.path, "dirty": st.is_dirty(), "pageCount": d.page_count(),
        "settings": {"pageWidth": d.settings.page_width, "pageHeight": d.settings.page_height, "facingPages": d.settings.facing_pages, "intent": d.settings.intent},
        "spreads": spreads,
        // The original layout and its alternate layouts (`layout.addAlternate`).
        "layouts": super::liquid::layouts_json(d),
        "parents": d.parents.iter().map(|p| json!({"id": p.id.0, "label": p.parent.as_ref().map(|i| i.label()), "pages": p.pages.len(), "items": p.items.len()})).collect::<Vec<_>>(),
        "layers": d.layers,
        "stories": stories,
        "paragraphStyles": d.styles.paragraph.iter().map(|p| &p.name).collect::<Vec<_>>(),
        "characterStyles": d.styles.character.iter().map(|p| &p.name).collect::<Vec<_>>(),
        "swatches": d.swatches.iter().map(|w| &w.name).collect::<Vec<_>>(),
        // The Swatches panel's chips: each swatch's kind and display colour (none for [None]).
        "swatchColors": d.swatches.iter().filter(|w| !w.hidden).map(|w| swatch_json(&d.swatches, w)).collect::<Vec<_>>(),
        "selection": st.selection,
        "activeLayer": st.active_layer.0,
        "tool": s.tool_id(),
        "canUndo": st.can_undo(),
        "canRedo": st.can_redo(),
    }))
}
