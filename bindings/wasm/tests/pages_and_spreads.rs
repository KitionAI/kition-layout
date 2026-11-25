//! Regression: turning on facing pages makes the parents two-page
//! spreads (right-hand parent items fall on right-hand pages); pages are numbered from 1 in every
//! command that takes them (`layout.pages.applyParent` as `layout.section` and `frame.thread`);
//! a `spread` that names nothing is refused, a parent spread can be named by its prefix, and a
//! frame threaded into a new `rect` goes on the source frame's spread.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

fn run(doc: &mut LayoutDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn inspect(doc: &LayoutDocument) -> Value {
    doc.inspect().unwrap()
}

/// The parent label document page `index` (0-based, as inspect lists them) shows.
fn parent_of(doc: &LayoutDocument, index: u64) -> Value {
    let inspected = inspect(doc);
    let pages: Vec<Value> = inspected["spreads"].as_array().unwrap().iter().flat_map(|spread| spread["pages"].as_array().unwrap().clone()).collect();
    pages.iter().find(|page| page["index"] == json!(index)).unwrap_or_else(|| panic!("page {index}"))["parent"].clone()
}

#[test]
fn facing_pages_make_parents_spreads() {
    let mut doc = LayoutDocument::create("book.klayout", 400, 600).unwrap();
    run(&mut doc, "layout.documentSetup", json!({ "facingPages": false, "pages": 4 }));
    assert_eq!(inspect(&doc)["parents"][0]["pages"], json!(1), "{}", inspect(&doc)["parents"]);
    run(&mut doc, "layout.documentSetup", json!({ "facingPages": true }));
    assert_eq!(inspect(&doc)["parents"][0]["pages"], json!(2), "parent A becomes a spread: {}", inspect(&doc)["parents"]);
    // A folio on the parent's right-hand page lands on the parent, by its prefix.
    let folio = run(&mut doc, "frame.add", json!({ "spread": "A", "rect": [700, 560, 760, 580], "text": "folio" }));
    assert!(folio["id"].is_u64(), "{folio}");
    assert_eq!(inspect(&doc)["parents"][0]["items"], json!(1));
    // Back to single pages: one page again.
    run(&mut doc, "layout.documentSetup", json!({ "facingPages": false }));
    assert_eq!(inspect(&doc)["parents"][0]["pages"], json!(1));
}

#[test]
fn pages_count_from_one_and_spreads_must_exist() {
    let mut doc = LayoutDocument::create("leaflet.klayout", 400, 600).unwrap();
    run(&mut doc, "layout.documentSetup", json!({ "pages": 3 }));
    // Page 1 is the first page, as layout.section and frame.thread count.
    run(&mut doc, "layout.pages.applyParent", json!({ "pages": [1], "parent": null }));
    assert_eq!(parent_of(&doc, 0), Value::Null, "the first page has no parent");
    assert_eq!(parent_of(&doc, 1), json!("A-Parent"), "the second keeps A");
    assert!(doc.execute("layout.pages.applyParent", json!({ "pages": [0], "parent": null })).is_err(), "there is no page 0");
    assert!(doc.execute("layout.pages.applyParent", json!({ "pages": [4], "parent": null })).is_err(), "nor a page 4");
    // A spread that names nothing is refused, not read as spread 0.
    for spread in [json!("Z"), json!({ "kind": "nope", "index": 0 }), json!(99), json!("1")] {
        let made = doc.execute("frame.add", json!({ "spread": spread.clone(), "rect": [10, 10, 100, 100], "content": "graphic" }));
        assert!(made.is_err(), "spread {spread}: {made:?}");
    }
}

#[test]
fn a_threaded_frame_goes_on_the_source_frames_spread() {
    let mut doc = LayoutDocument::create("story.klayout", 400, 600).unwrap();
    run(&mut doc, "layout.documentSetup", json!({ "pages": 3, "facingPages": false }));
    let first = run(&mut doc, "frame.add", json!({ "spread": 2, "rect": [36, 36, 364, 300], "text": "Lorem ipsum ".repeat(200) }));
    let next = run(&mut doc, "frame.thread", json!({ "from": first["id"], "rect": [36, 320, 364, 560] }));
    let inspected = inspect(&doc);
    let on_spread = |spread: usize| inspected["spreads"][spread]["items"].as_array().unwrap().iter().any(|item| item["id"] == next["frame"]);
    assert!(on_spread(2) && !on_spread(0), "the new frame is on spread 2 with its source: {next}");
}
