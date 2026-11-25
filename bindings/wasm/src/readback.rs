//! What the editor's Properties panel and status bar read back beyond `file.inspect` (plan
//! 2026-11-25, stage 4), added to `inspect()` without changing any field it already has: each
//! object's whole stroke (type, alignment, caps, joins, arrowheads, gap), its four corners and a
//! text frame's options (columns, gutter, inset, vertical justification); every swatch's colour,
//! the unnamed ones too (`colors`); and each story's words and the characters that do not fit.

use kition_layout_doc::color::{Color, SwatchValue, swatch};
use kition_layout_doc::{Content, Document, Item, ItemId};
use kition_layout_engine::Session;
use serde_json::{Map, Value, json};

/// Adds the read-backs to `inspected` (the answer of `file.inspect` on `scratch`, a session that
/// holds a copy of `doc`).
pub fn extend(inspected: &mut Value, doc: &Document, scratch: &Session) {
    if let Some(spreads) = inspected.get_mut("spreads").and_then(Value::as_array_mut) {
        for spread in spreads {
            if let Some(items) = spread.get_mut("items").and_then(Value::as_array_mut) {
                for item in items {
                    extend_item(item, doc, 0);
                }
            }
        }
    }
    if let Some(stories) = inspected.get_mut("stories").and_then(Value::as_array_mut) {
        for entry in stories {
            extend_story(entry, doc, scratch);
        }
    }
    if let Some(object) = inspected.as_object_mut() {
        object.insert("colors".into(), colors(doc));
    }
}

/// Groups nest at most this deep before the read-backs stop (a hostile file).
const MAX_DEPTH: usize = 64;

fn extend_item(entry: &mut Value, doc: &Document, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    if let Some(item) = entry.get("id").and_then(Value::as_u64).and_then(|id| doc.item(ItemId(id))) {
        add_item_fields(entry, item);
    }
    if let Some(children) = entry.get_mut("children").and_then(Value::as_array_mut) {
        for child in children {
            extend_item(child, doc, depth + 1);
        }
    }
}

fn add_item_fields(entry: &mut Value, item: &Item) {
    let Some(object) = entry.as_object_mut() else { return };
    if let Ok(stroke) = serde_json::to_value(&item.stroke) {
        object.insert("strokeDetail".into(), stroke);
    }
    if let Ok(corners) = serde_json::to_value(item.corners.corners) {
        object.insert("corners".into(), corners);
    }
    if let Content::Text(text) = &item.content
        && let Ok(options) = serde_json::to_value(&text.options)
    {
        object.insert("textFrame".into(), options);
    }
}

fn extend_story(entry: &mut Value, doc: &Document, scratch: &Session) {
    let Some(id) = entry.get("id").and_then(Value::as_u64) else { return };
    let Some(story) = doc.stories.get(&kition_layout_doc::StoryId(id)) else { return };
    let composed = scratch.active().map(|state| scratch.cache.get(&state.doc, story.id, None));
    let overset = composed.and_then(|composed| composed.overset_at).map_or(0, |at| story.text.get(at..).map_or(0, |rest| rest.chars().count()));
    if let Some(object) = entry.as_object_mut() {
        object.insert("words".into(), json!(word_count(&story.text)));
        object.insert("oversetChars".into(), json!(overset));
    }
}

/// Words as a person counts them: runs of letters and digits between spaces and punctuation, and
/// each Chinese, Japanese or Korean character on its own.
pub fn word_count(text: &str) -> usize {
    let mut words = 0usize;
    let mut inside = false;
    for c in text.chars() {
        if is_cjk(c) {
            words = words.saturating_add(1);
            inside = false;
        } else if c.is_alphanumeric() || c == '\'' || c == '\u{2019}' {
            if !inside {
                words = words.saturating_add(1);
                inside = true;
            }
        } else {
            inside = false;
        }
    }
    words
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F)
}

/// Every swatch's colour by name, hidden (unnamed) ones too: `hex` as the screen shows it, and
/// `cmyk` (percentages) for a colour defined in CMYK.
fn colors(doc: &Document) -> Value {
    let mut out = Map::new();
    for swatch_entry in &doc.swatches {
        let defined = match &swatch_entry.value {
            SwatchValue::Color { color, .. } | SwatchValue::Paper { color } => Some(*color),
            SwatchValue::Tint { base, tint } => swatch::resolve(&doc.swatches, base, *tint),
            _ => None,
        };
        let Some(color) = defined else { continue };
        let [r, g, b, _] = color.to_rgba8(1.0);
        let mut value = json!({ "hex": format!("#{r:02x}{g:02x}{b:02x}") });
        if let Color::Cmyk { c, m, y, k } = color {
            value["cmyk"] = json!([percent(c), percent(m), percent(y), percent(k)]);
        }
        out.insert(swatch_entry.name.clone(), value);
    }
    Value::Object(out)
}

fn percent(part: f32) -> f64 {
    (f64::from(part) * 1000.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_count_latin_runs_and_each_cjk_character() {
        assert_eq!(word_count(""), 0);
        assert_eq!(word_count("Hello, world!  It's 2026."), 4);
        assert_eq!(word_count("极光 Aurora"), 3);
        assert_eq!(word_count("one\ttwo\nthree"), 3);
    }
}
