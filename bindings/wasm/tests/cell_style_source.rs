use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::json;

#[test]
fn cell_style_creation_accepts_explicit_source_cell_addresses() {
    let mut doc = LayoutDocument::create("Cell.klayout", 612, 792).unwrap();
    let frame = doc.execute("frame.create", json!({"rect":[20,20,300,300],"content":"text"})).unwrap();
    let table = doc.execute("table.insert", json!({"rows":2,"cols":2})).unwrap();
    doc.execute("table.setCell", json!({"row":1,"col":1,"fill":"Black"})).unwrap();
    doc.execute("selection.set", json!({"ids":[]})).unwrap();
    let result = doc
        .execute("style.cell.create", json!({"name":"From Cell","fromSelection":true,"story":frame["story"],"table":table["table"],"row":1,"col":1}));
    assert!(result.is_ok(), "explicit address must reach table::target: {result:?}");
    let saved = doc.execute("style.list", json!({})).unwrap();
    assert!(saved["cell"].as_array().unwrap().contains(&json!("From Cell")), "created source style is retained: {saved}");
}

#[test]
fn selected_cell_style_creation_declares_its_copied_source_values() {
    let mut doc = LayoutDocument::create("Cell.klayout", 612, 792).unwrap();
    let frame = doc.execute("frame.create", json!({"rect":[20,20,300,300],"content":"text"})).unwrap();
    let table = doc.execute("table.insert", json!({"rows":2,"cols":2})).unwrap();
    doc.execute("table.select", json!({"story":frame["story"],"table":table["table"],"row":1,"col":1})).unwrap();
    let params = json!({"name":"Selected Cell","fromSelection":true});
    let under = doc.under("style.cell.create", &params).unwrap().expect("a selected source has premises");
    let sources: Vec<_> =
        under.premises.iter().filter(|premise| premise.fields.iter().flatten().any(|field| field.starts_with("~styleSource"))).collect();
    assert_eq!(sources.len(), 1, "only the selected cell's copied values are held: {under:?}");
    assert_ne!(sources[0].id, "layout", "the selected path declares its source on the cell: {under:?}");
    let result = doc.execute("style.cell.create", params).unwrap();
    assert_eq!(result["name"], "Selected Cell");
}
