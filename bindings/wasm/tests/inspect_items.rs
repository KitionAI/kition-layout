//! Regression: inspect tells items apart as the Layers panel does
//! and shows their text wrap: a two-anchor path with curves is a path, not a line; type on a path
//! is not a text frame; a frame's text wrap is reported.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

fn run(doc: &mut LayoutDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn item(inspected: &Value, id: &Value) -> Value {
    fn find(items: &Value, id: &Value) -> Option<Value> {
        items.as_array()?.iter().find_map(|item| if &item["id"] == id { Some(item.clone()) } else { find(&item["children"], id) })
    }
    let spreads = inspected["spreads"].as_array().unwrap();
    spreads.iter().find_map(|spread| find(&spread["items"], id)).unwrap_or_else(|| panic!("item {id} in {inspected}"))
}

#[test]
fn inspect_names_paths_type_on_a_path_and_wrap() {
    let mut doc = LayoutDocument::create("poster.klayout", 600, 800).unwrap();
    let straight = run(&mut doc, "path.add", json!({ "anchors": [{ "p": [70, 100] }, { "p": [530, 100] }] }))["id"].clone();
    let curved =
        run(&mut doc, "path.add", json!({ "anchors": [{ "p": [70, 330], "out": [200, 262] }, { "p": [530, 330], "in": [400, 262] }] }))["id"].clone();
    let on_path = run(&mut doc, "text.onPath", json!({ "id": curved, "text": "EVERY FRIDAY" }))["id"].clone();
    let photo = run(&mut doc, "frame.add", json!({ "rect": [100, 400, 300, 600], "content": "graphic" }))["id"].clone();
    run(&mut doc, "object.textWrap", json!({ "ids": [photo], "mode": "contour", "offset": 10 }));
    let inspected = doc.inspect().unwrap();
    assert_eq!(item(&inspected, &straight)["kind"], json!("<line>"));
    let on_path = if on_path.is_null() { curved.clone() } else { on_path };
    assert_eq!(item(&inspected, &on_path)["kind"], json!("<text path>"), "{}", item(&inspected, &on_path));
    let wrap = item(&inspected, &photo)["wrap"].clone();
    assert_eq!(wrap["mode"], json!("contour"), "{wrap}");
    assert_eq!(wrap["offsets"], json!([10.0, 10.0, 10.0, 10.0]), "{wrap}");
    assert!(item(&inspected, &straight).get("wrap").is_none(), "no wrap, nothing reported");
    // A curved two-anchor path that holds no text is a path.
    let bare =
        run(&mut doc, "path.add", json!({ "anchors": [{ "p": [70, 700], "out": [200, 650] }, { "p": [530, 700], "in": [400, 650] }] }))["id"].clone();
    assert_eq!(item(&doc.inspect().unwrap(), &bare)["kind"], json!("<path>"));
}
