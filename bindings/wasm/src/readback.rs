//! What the editor's Properties panel and status bar read back beyond `file.inspect` (plan
//! 2026-11-25, stage 4), added to `inspect()` without changing any field it already has: each
//! object's whole stroke (type, alignment, caps, joins, arrowheads, gap), its four corners and a
//! text frame's options (columns, gutter, inset, vertical justification); every swatch's colour,
//! the unnamed ones too (`colors`); and each story's words and the characters that do not fit.
//! Stage 7 adds each object's blend mode and effects (the Effects dialog), every paragraph and
//! character style's definition and resolved values (`styleDefs`, the style options), and how
//! each colour swatch is made (process or spot, a tint's base and tint).

use kition_layout_doc::color::{Color, SwatchValue, swatch};
use kition_layout_doc::{Content, Document, Item, ItemId};
use kition_layout_engine::Session;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

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
        object.insert("styleDefs".into(), style_defs(doc));
    }
}

/// Every paragraph and character style: what it sets itself (`para`, `chars`), its Based On and
/// Next Style, and (paragraph styles) the values it resolves to with its Based On chain.
fn style_defs(doc: &Document) -> Value {
    let styles = &doc.styles;
    // How much text uses each style: paragraphs, and runs of characters (Delete Style asks for
    // a replacement only when text uses the style).
    let mut paragraphs: HashMap<&str, usize> = HashMap::new();
    let mut runs: HashMap<&str, usize> = HashMap::new();
    for story in doc.stories.values() {
        for_each_text(story, 0, &mut |text| {
            for format in &text.paras {
                *paragraphs.entry(format.style.as_str()).or_default() += 1;
            }
            for run in text.chars.iter().filter(|run| run.len > 0) {
                *runs.entry(run.format.style.as_str()).or_default() += 1;
            }
        });
    }
    let paragraph: Vec<Value> = styles
        .paragraph
        .iter()
        .map(|style| {
            let (para, chars) = styles.resolve_para_style(&style.name);
            json!({
                "name": style.name,
                "basedOn": style.based_on,
                "nextStyle": style.next_style,
                "para": style.para,
                "chars": style.chars,
                "resolvedPara": para,
                "resolvedChars": chars,
                "uses": paragraphs.get(style.name.as_str()).copied().unwrap_or(0),
            })
        })
        .collect();
    let character: Vec<Value> = styles
        .character
        .iter()
        .map(|style| {
            json!({
                "name": style.name,
                "basedOn": style.based_on,
                "chars": style.chars,
                "resolvedChars": styles.char_style_attrs(&style.name),
                "uses": runs.get(style.name.as_str()).copied().unwrap_or(0),
            })
        })
        .collect();
    json!({ "paragraph": paragraph, "character": character })
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
    if let Ok(blend) = serde_json::to_value(item.blend) {
        object.insert("blend".into(), blend);
    }
    object.insert("effects".into(), effects(&item.effects));
}

/// Every effect with all its settings (the defaults too), for the Effects dialog.
fn effects(effects: &kition_layout_doc::Effects) -> Value {
    let value = |part: Result<Value, serde_json::Error>| part.unwrap_or(Value::Null);
    json!({
        "dropShadow": value(serde_json::to_value(&effects.drop_shadow)),
        "innerShadow": value(serde_json::to_value(&effects.inner_shadow)),
        "outerGlow": value(serde_json::to_value(&effects.outer_glow)),
        "innerGlow": value(serde_json::to_value(&effects.inner_glow)),
        "bevel": value(serde_json::to_value(&effects.bevel)),
        "satin": value(serde_json::to_value(&effects.satin)),
        "feather": effects.feather,
        "directionalFeather": value(serde_json::to_value(&effects.directional_feather)),
        "gradientFeather": value(serde_json::to_value(&effects.gradient_feather)),
    })
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
    let uses = swatch_uses(doc);
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
        // What uses it (Delete Swatch asks for a replacement only then).
        value["uses"] = json!(uses.get(swatch_entry.name.as_str()).copied().unwrap_or(0));
        // How the swatch is made (Swatch Options): process or spot, a tint's base and tint.
        match &swatch_entry.value {
            SwatchValue::Color { color_type, .. } => value["spot"] = json!(matches!(color_type, kition_layout_doc::color::ColorType::Spot)),
            SwatchValue::Tint { base, tint } => {
                value["base"] = json!(base);
                value["tint"] = json!(tint);
            }
            _ => {}
        }
        out.insert(swatch_entry.name.clone(), value);
    }
    Value::Object(out)
}

/// Visits a story and the texts inside it (table cells, notes), to a bounded depth.
fn for_each_text<'a>(story: &'a kition_layout_doc::Story, depth: usize, visit: &mut impl FnMut(&'a kition_layout_doc::Story)) {
    if depth > MAX_DEPTH {
        return;
    }
    visit(story);
    for table in story.tables.values() {
        for cell in &table.cells {
            for_each_text(&cell.text, depth + 1, visit);
        }
    }
    for note in story.notes.iter().chain(&story.endnotes) {
        for_each_text(&note.text, depth + 1, visit);
    }
}

/// How many objects (fills and strokes) and runs of text use each swatch.
fn swatch_uses(doc: &Document) -> HashMap<&str, usize> {
    fn items<'a>(list: &'a [std::sync::Arc<Item>], depth: usize, uses: &mut HashMap<&'a str, usize>) {
        if depth > MAX_DEPTH {
            return;
        }
        for item in list {
            *uses.entry(item.fill.swatch.as_str()).or_default() += 1;
            *uses.entry(item.stroke.swatch.as_str()).or_default() += 1;
            items(item.children(), depth + 1, uses);
        }
    }
    let mut uses = HashMap::new();
    for spread in doc.spreads.iter().chain(&doc.parents) {
        items(&spread.items, 0, &mut uses);
    }
    for story in doc.stories.values() {
        for_each_text(story, 0, &mut |text| {
            for run in &text.chars {
                for name in [&run.format.over.fill, &run.format.over.stroke].into_iter().flatten() {
                    *uses.entry(name.as_str()).or_default() += 1;
                }
            }
        });
    }
    for style in &doc.styles.paragraph {
        for name in [&style.chars.fill, &style.chars.stroke].into_iter().flatten() {
            *uses.entry(name.as_str()).or_default() += 1;
        }
    }
    for style in &doc.styles.character {
        for name in [&style.chars.fill, &style.chars.stroke].into_iter().flatten() {
            *uses.entry(name.as_str()).or_default() += 1;
        }
    }
    uses
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
