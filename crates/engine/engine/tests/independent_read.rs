//! C8-layout-read: small engine defects found while reading handler dependencies.
use kition_layout_engine::Session;
use serde_json::json;

#[test]
fn section_updates_preserve_unspecified_values_at_every_page() {
    for page in [1, 3] {
        let mut session = Session::new();
        session.execute("file.new", &json!({"pages":5})).unwrap();
        session
            .execute(
                "layout.section",
                &json!({
                    "page":page,"startNumber":7,"style":"lowerRoman","prefix":"X","includePrefix":true,"marker":"Before"
                }),
            )
            .unwrap();
        session.execute("layout.section", &json!({"page":page,"marker":"After"})).unwrap();
        let section = session.doc().unwrap().doc.sections.iter().find(|section| section.start == page - 1).unwrap();
        assert_eq!(section.start_number, Some(7));
        assert_eq!(serde_json::to_value(section.style).unwrap(), "lowerRoman");
        assert_eq!(section.prefix, "X");
        assert!(section.include_prefix);
        assert_eq!(section.marker, "After");
    }
}

#[test]
fn locked_items_do_not_relocate_between_spreads() {
    use kition_layout_engine::doc::{ItemId, SpreadRef};
    let mut session = Session::new();
    session.execute("file.new", &json!({"pages":3})).unwrap();
    let id = session.execute("frame.create", &json!({"rect":[10,10,50,50],"content":"unassigned"})).unwrap()["id"].as_u64().unwrap();
    session.execute("object.lock", &json!({"ids":[id]})).unwrap();
    let before = serde_json::to_value(&session.doc().unwrap().doc).unwrap();
    session.execute("transform.move", &json!({"ids":[id],"dx":15,"toSpread":{"kind":"doc","index":1}})).unwrap();
    assert_eq!(session.doc().unwrap().doc.find(ItemId(id)).unwrap().spread, SpreadRef::Doc(0));
    assert_eq!(serde_json::to_value(&session.doc().unwrap().doc).unwrap(), before);
}
