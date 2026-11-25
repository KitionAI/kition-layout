//! Regression: CMYK swatch components are percentages (0 to 100,
//! as the catalogue says: `c: 1` is 1 %), a command inserts the page-number and other special
//! characters, and Step and Repeat offsets only along the axes it is given.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

fn run(doc: &mut LayoutDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn swatch_hex(doc: &LayoutDocument, name: &str) -> String {
    let inspected = doc.inspect().unwrap();
    let swatches = inspected["swatchColors"].as_array().unwrap_or_else(|| panic!("swatches in {inspected}"));
    swatches.iter().find(|swatch| swatch["name"] == name).unwrap_or_else(|| panic!("{name} in {swatches:?}"))["color"].as_str().unwrap().to_owned()
}

#[test]
fn cmyk_components_are_percentages() {
    let mut doc = LayoutDocument::create("swatches.klayout", 600, 800).unwrap();
    run(&mut doc, "swatch.add", json!({ "name": "Faint", "color": { "c": 1, "m": 0, "y": 0, "k": 0 } }));
    run(&mut doc, "swatch.add", json!({ "name": "Cyan", "color": { "c": 100, "m": 0, "y": 0, "k": 0 } }));
    run(&mut doc, "swatch.add", json!({ "name": "Half", "color": { "c": 0, "m": 0, "y": 0, "k": 50 } }));
    let red = |hex: &str| u8::from_str_radix(&hex[1..3], 16).unwrap();
    assert!(red(&swatch_hex(&doc, "Faint")) > 240, "1 % cyan is almost white: {}", swatch_hex(&doc, "Faint"));
    assert!(red(&swatch_hex(&doc, "Cyan")) < 60, "100 % cyan: {}", swatch_hex(&doc, "Cyan"));
    let half = red(&swatch_hex(&doc, "Half"));
    assert!((100..200).contains(&half), "50 % black: {half}");
    assert!(doc.execute("swatch.add", json!({ "name": "Over", "color": { "c": 120, "m": 0, "y": 0, "k": 0 } })).is_err(), "components are 0 to 100");
}

#[test]
fn special_characters_have_a_command() {
    let mut doc = LayoutDocument::create("folio.klayout", 600, 800).unwrap();
    let frame = run(&mut doc, "frame.add", json!({ "spread": 0, "rect": [40, 740, 300, 760], "text": "Page " }));
    run(&mut doc, "text.select", json!({ "story": frame["story"], "anchor": 5, "focus": 5 }));
    run(&mut doc, "text.insertSpecial", json!({ "character": "currentPageNumber" }));
    let story = run(&mut doc, "story.get", json!({ "story": frame["story"] }));
    assert!(story["text"].as_str().unwrap().contains('\u{E000}'), "{story}");
    assert!(doc.execute("text.insertSpecial", json!({ "character": "nope" })).is_err());
}

#[test]
fn step_and_repeat_offsets_only_the_axes_given() {
    let mut doc = LayoutDocument::create("buoys.klayout", 600, 800).unwrap();
    let buoy = run(&mut doc, "frame.add", json!({ "spread": 0, "rect": [40, 100, 60, 120], "content": "graphic" }));
    let copies = run(&mut doc, "edit.stepAndRepeat", json!({ "ids": [buoy["id"]], "count": 3, "dx": 50 }));
    assert_eq!(copies["created"], json!(3), "{copies}");
    let inspected = doc.inspect().unwrap();
    let items = inspected["spreads"][0]["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    let mut lefts: Vec<f64> = items.iter().map(|item| item["bounds"][0].as_f64().unwrap()).collect();
    lefts.sort_by(f64::total_cmp);
    assert_eq!(lefts, [40.0, 90.0, 140.0, 190.0], "{items:?}");
    for item in items {
        assert_eq!(item["bounds"][1], json!(100.0), "every copy stays level: {item}");
    }
}
