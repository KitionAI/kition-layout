use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::json;

#[test]
fn table_graphic_source_parameters_reach_the_handler() {
    let mut doc = LayoutDocument::create("Graphic.klayout", 612, 792).unwrap();
    doc.execute("frame.add", json!({"rect":[20,20,300,300],"content":"text"})).unwrap();
    doc.execute("table.insert", json!({"rows":2,"cols":2})).unwrap();
    let png = kition_layout_render::Rendered { width: 2, height: 2, pixels: vec![120; 16] }.to_png();
    let result = doc.execute("table.placeGraphic", json!({"base64":kition_layout_engine::cmd::base64_encode(&png),"name":"cell.png"}));
    assert!(result.is_ok(), "image source must reach table::place_graphic: {result:?}");
    let source = doc.execute("table.placeGraphic", json!({"path":"/c8-layout-read-2-missing-image.png"})).unwrap_err();
    assert!(!source.contains("has no parameter"), "path must reach file::read_source: {source}");
}
