//! C8-land-13-layout: per-call facts, independent of the person's editor state.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::{CallContext, KitDocument};
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

fn fixture() -> (LayoutDocument, Value, Value) {
    let mut doc = LayoutDocument::create("Premises.klayout", 612, 792).unwrap();
    let a = doc.execute("frame.add", json!({"rect":[20,20,60,60],"content":"unassigned"})).unwrap()["id"].clone();
    let b = doc.execute("frame.add", json!({"rect":[100,20,140,60],"content":"unassigned"})).unwrap()["id"].clone();
    (doc, a, b)
}

#[test]
fn context_is_reversible_without_an_edit() {
    let (mut doc, a, b) = fixture();
    doc.execute("select.set", json!({"ids":[a]})).unwrap();
    let saved = doc.editor_state();
    assert!(saved.is_object(), "the complete editor state must be available");
    kition_kit::testing::check_context(
        &mut doc,
        &CallContext { on: vec![b.clone()], ..Default::default() },
        &["/selection", "/activeLayer", "/tool", "/canUndo", "/canRedo", "/dirty", "/path"],
    )
    .unwrap();
    assert_eq!(doc.editor_state(), saved);
    let before = doc.inspect().unwrap();
    assert!(doc.set_context(&CallContext { on: vec![json!(u64::MAX)], ..Default::default() }).is_err());
    assert_eq!(doc.inspect().unwrap(), before, "bad context must be atomic");
}

#[test]
fn property_calls_hold_only_their_fields() {
    let (doc, a, _) = fixture();
    for (command, params, fields) in [
        ("object.rename", json!({"id":a,"name":"New"}), vec!["name"]),
        ("object.opacity", json!({"ids":[a],"opacity":0.5}), vec!["opacity"]),
        ("object.fill", json!({"ids":[a],"swatch":"Black"}), vec!["fill.swatch", "fill.tint"]),
        ("object.label", json!({"ids":[a],"label":"Tag"}), vec!["props.label"]),
    ] {
        let under = doc.under(command, &params).unwrap().unwrap();
        let premise = under.premises.iter().find(|premise| premise.id == a).unwrap();
        assert_eq!(premise.fields, Some(fields.into_iter().map(str::to_owned).collect()), "{command}");
        assert_eq!(premise.positions, Some(vec![]), "{command}");
    }
}

#[test]
fn moves_hold_transform_lock_and_parent_transform() {
    let (mut doc, a, b) = fixture();
    let group = doc.execute("arrange.group", json!({"ids":[a,b]})).unwrap()["id"].clone();
    let under = doc.under("transform.move", &json!({"ids":[a],"dx":10,"dy":0})).unwrap().unwrap();
    let moved = under.premises.iter().find(|premise| premise.id == a).unwrap();
    assert_eq!(moved.fields, Some(vec!["locked".to_owned()]));
    assert_eq!(moved.positions, Some(vec!["xf".to_owned(), "items".to_owned(), "spread".to_owned()]));
    let parent = under.premises.iter().find(|premise| premise.id == group).unwrap();
    assert_eq!(parent.fields, Some(vec![]));
    assert_eq!(parent.positions, Some(vec!["xf".to_owned(), "items".to_owned(), "spread".to_owned()]));
}

#[test]
fn moves_to_a_spread_hold_the_destination_index() {
    let (mut doc, a, _) = fixture();
    doc.execute("layout.pages.insert", json!({"count":2})).unwrap();
    let loaded = kition_kit::session::model::Exchange::new()
        .batch(&mut doc, &json!({"ops":[],"load":[{"target":"layout","fields":["spreads"]}]}), true)
        .unwrap();
    let destination = json!(loaded["loaded"][0]["fields"]["spreads"][1].as_str().unwrap().parse::<u64>().unwrap());
    let under = doc.under("transform.move", &json!({"ids":[a],"dx":2,"dy":0,"toSpread":{"kind":"doc","index":1}})).unwrap().unwrap();
    assert!(under.premises.iter().any(|premise| premise.id == destination), "destination existence and placement matter: {under:?}");
    assert!(
        under.within.iter().any(|within| within.parent == "layout" && within.field == "spreads" && within.from == 1 && within.to == 2),
        "the index must still name that destination: {under:?}"
    );
}

#[test]
fn layer_index_move_declares_the_gap_in_stored_order() {
    let (mut doc, _, _) = fixture();
    let a = doc.execute("layer.new", json!({"name":"A"})).unwrap()["id"].clone();
    doc.execute("layer.new", json!({"name":"B"})).unwrap();
    let under = doc.under("layer.move", &json!({"id":a,"to":0})).unwrap().unwrap();
    assert_eq!(under.within.len(), 1);
    let window = serde_json::to_value(&under.within[0]).unwrap();
    assert_eq!(window["parent"], "layout");
    assert_eq!(window["field"], "layers");
    assert_eq!(window["from"], 0);
    assert_eq!(window["to"], 0);
}

#[test]
fn moves_carry_children_but_renames_do_not() {
    let (mut doc, a, b) = fixture();
    let group = doc.execute("arrange.group", json!({"ids":[a,b]})).unwrap()["id"].clone();
    let carried =
        doc.carried(std::slice::from_ref(&group), &[json!({"id":group,"fields":["xf"],"edits":[{"field":"xf.4","before":0,"after":10}]})]).unwrap();
    assert_eq!(carried.len(), 1);
    assert_eq!(carried[0].carries.count, 2);
    assert!(
        doc.carried(std::slice::from_ref(&group), &[json!({"id":group,"fields":["name"],"edits":[{"field":"name","before":"","after":"G"}]})])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn point_places_replay_all_changed_geometry_together() {
    let (mut doc, a, b) = fixture();
    assert_eq!(doc.derived(&[], &[]).unwrap().len(), 8, "an omitted id list asks for the root, items, pages, spreads and layers with derived facts");
    let params = json!({"at":[80,40],"delta":2});
    let under = doc.under("gap.move", &params).unwrap().unwrap();
    assert!(under.covers.is_some());
    assert_eq!(under.objects, vec![a.clone(), b.clone()]);
    let before = doc.derived(&[a.clone(), b.clone()], &[]).unwrap();
    doc.execute("transform.move", json!({"ids":[a,b],"dx":200,"dy":0})).unwrap();
    let asks: Vec<_> = before
        .into_iter()
        .map(|values| kition_kit::CoveredAsk {
            id: values.id,
            before: values.values.into_iter().map(|(name, value)| (format!("~{name}"), value)).collect(),
            ..Default::default()
        })
        .collect();
    let historical = doc.covered("gap.move", &params, &asks).unwrap().unwrap();
    assert_eq!(historical.under, under.objects, "the historical pick uses both original bounds");
    assert!(historical.covered.iter().all(|covered| covered.covered == Some(true)));
}

#[test]
fn grouping_nested_children_returns_a_parameter_error_without_panicking() {
    let (mut doc, a, b) = fixture();
    doc.execute("arrange.group", json!({"ids":[a,b]})).unwrap();
    let error = doc.execute("arrange.group", json!({"ids":[a,b]})).unwrap_err();
    assert!(error.contains("top-level"), "must validate before indexing the top-level members: {error}");
}

#[test]
fn table_numbers_resolve_with_their_holder_and_hold_owned_cells() {
    let mut doc = LayoutDocument::create("Table.klayout", 612, 792).unwrap();
    let frame = doc.execute("frame.add", json!({"rect":[20,20,300,300],"content":"text"})).unwrap();
    let table = doc.execute("table.insert", json!({"rows":2,"cols":2})).unwrap();
    let under = doc.under("table.setRowHeight", &json!({"story":frame["story"],"table":table["table"],"row":0,"height":30})).unwrap().unwrap();
    assert!(under.objects.iter().any(Value::is_string), "a table's local number resolves to its stable model address: {under:?}");
    assert!(under.premises.len() >= 5, "the table and its owned cells/rows/columns are dependencies: {under:?}");
}
