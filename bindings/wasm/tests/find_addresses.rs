//! C8-layout review: typed layer addresses and names resolve to identical object queries.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::json;

#[test]
fn object_find_resolves_layer_addresses_and_names() {
    let mut doc = LayoutDocument::create("Find.klayout", 612, 792).unwrap();
    let a = doc.execute("frame.create", json!({"rect":[20,20,60,60]})).unwrap()["id"].clone();
    let layer = doc.execute("layer.new", json!({"name":"Find layer"})).unwrap()["id"].clone();
    doc.execute("frame.create", json!({"rect":[80,20,120,60]})).unwrap();
    for address in [json!("Layer 1"), json!(1), json!("1")] {
        assert_eq!(doc.execute("find.objects", json!({"layer":address,"select":false})).unwrap()["ids"], json!([a]), "{address}");
    }
    assert_eq!(doc.execute("find.changeObjects", json!({"layer":layer,"change":{"opacity":0.25}})).unwrap()["changed"], 1);
}
