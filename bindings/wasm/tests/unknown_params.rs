//! A command refuses a parameter its schema does not name, saying which, instead of running
//! without it (engine rule E7a, kit requirement AX15).
#![allow(clippy::unwrap_used, clippy::panic)]

use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::json;

fn layout() -> LayoutDocument {
    LayoutDocument::create("Layout.klayout", 64, 36).unwrap()
}

#[test]
fn every_command_refuses_a_parameter_it_does_not_take() {
    if let Err(problems) = kition_kit::testing::check_unknown_params(layout, &[]) {
        panic!("{} commands:\n{}", problems.len(), problems.join("\n"));
    }
}

#[test]
fn a_misspelled_parameter_names_the_one_it_resembles_and_changes_nothing() {
    let mut doc = layout();
    let before = doc.seal_and_save(None).unwrap();
    let error = doc.execute("transform.set", json!({ "widht": 10 })).unwrap_err();
    assert!(error.contains("`widht`") && error.contains("did you mean `width`"), "{error}");
    assert_eq!(doc.seal_and_save(None).unwrap(), before);
}
