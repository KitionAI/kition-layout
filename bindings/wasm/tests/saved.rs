//! C8-land-13-layout: a command's live result must survive the native file boundary exactly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::session::model::Exchange;
use kition_kit::{KitDocument, document_content};
use kition_layout_kit::LayoutDocument;
use serde_json::json;

#[test]
fn representative_fixture_writes_equal_the_saved_document() {
    for file in ["basic.klayout", "check-empty-frame.klayout", "check-overset.klayout"] {
        let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance").join(file)).unwrap();
        for (command, mut params) in [
            ("object.rename", json!({"name":"Saved name"})),
            ("object.opacity", json!({"opacity":0.123456789})),
            ("object.fill", json!({"swatch":"Black","tint":0.123456789})),
            ("object.stroke", json!({"weight":1.23456789,"tint":0.123456789})),
            ("transform.move", json!({"dx":0.123456789,"dy":-2.3456789})),
            ("object.label", json!({"label":"Saved label"})),
        ] {
            let mut doc = LayoutDocument::open(file, &bytes).unwrap();
            let inspected = doc.inspect().unwrap();
            let id =
                inspected["spreads"].as_array().unwrap().iter().flat_map(|spread| spread["items"].as_array().unwrap()).next().unwrap()["id"].clone();
            params[if command == "object.rename" { "id" } else { "ids" }] = if command == "object.rename" { id.clone() } else { json!([id]) };
            let mut exchange = Exchange::new();
            let before = exchange.replica(&mut doc).unwrap().state_json().unwrap();
            let receipt = exchange.batch(&mut doc, &json!({"ops":[{"op":"command","id":command,"params":params}]}), false).unwrap();
            let live = document_content(doc.inspect().unwrap(), &["/selection", "/tool", "/activeLayer"]);
            let saved = doc.save(None).unwrap();
            let mut reopened = LayoutDocument::open(file, &saved).unwrap();
            assert_eq!(live, document_content(reopened.inspect().unwrap(), &["/selection", "/tool", "/activeLayer"]), "{file}: {command}");
            let after = exchange.replica(&mut doc).unwrap().state_json().unwrap();
            let disk = Exchange::new().replica(&mut reopened).unwrap().state_json().unwrap();
            let schema = kition_layout_kdm::schema();
            let memory = kition_kit::kdm::reread::diff_paired(&schema, &before, &after).to_json();
            let persisted = kition_kit::kdm::reread::diff_paired(&schema, &before, &disk).to_json();
            assert_eq!(memory, persisted, "{file}: {command}: receipt source must equal saved diff; receipt={receipt}");
            assert!(!persisted["changed"].as_array().unwrap().is_empty(), "the invariant must exercise a real write: {receipt}");
        }
    }
}
