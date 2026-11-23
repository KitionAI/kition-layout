//! The document keeps the text a hyperlink starts from, and the text a data merge placeholder
//! covers, as a mark on that text (the `hyperlink` and `placeholder` marks of a story's text): a
//! character carries one hyperlink and one placeholder at most, and a mark needs a character. The
//! commands refuse what the document could not keep (schema-freeze,
//! cases/regressions/issue-schema-freeze-text-ranges).

use kition_layout_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn refused(s: &mut Session, id: &str, p: Value) -> String {
    s.execute(id, &p).err().unwrap_or_else(|| panic!("{id} {p} should be refused")).to_string()
}

#[test]
fn a_hyperlink_on_text_another_starts_from_is_refused() {
    let mut s = Session::new();
    run(&mut s, "file.new", json!({"pages": 1}));
    let frame = run(&mut s, "frame.create", json!({"rect": [72, 72, 400, 300], "content": "text", "text": "Visit us today"}));
    run(&mut s, "text.select", json!({"story": frame["story"], "anchor": 0, "focus": 8}));
    run(&mut s, "hyperlink.create", json!({"url": "https://a.example"}));
    run(&mut s, "text.select", json!({"story": frame["story"], "anchor": 6, "focus": 14}));
    assert!(refused(&mut s, "hyperlink.create", json!({"url": "https://b.example"})).contains("already starts a hyperlink"));
    run(&mut s, "text.select", json!({"story": frame["story"], "anchor": 9, "focus": 14}));
    run(&mut s, "hyperlink.create", json!({"url": "https://b.example"}));
    assert_eq!(run(&mut s, "hyperlink.list", json!({})).as_array().map(Vec::len), Some(2));
}

#[test]
fn a_placeholder_needs_text_no_other_placeholder_covers() {
    let mut s = Session::new();
    run(&mut s, "file.new", json!({"facingPages": false, "pages": 1}));
    let csv = kition_layout_engine::cmd::base64_encode(b"Name,Link\nAda,https://example.com\n");
    run(&mut s, "data.source.select", json!({"bytes": csv, "name": "people.csv"}));
    let frame = run(&mut s, "frame.create", json!({"rect": [10, 10, 200, 40], "content": "text", "text": "Go there"}));
    run(&mut s, "text.select", json!({"story": frame["story"], "anchor": 2}));
    assert!(refused(&mut s, "data.placeholder.add", json!({"field": "Link", "role": "hyperlink"})).contains("select the text"));
    run(&mut s, "text.select", json!({"story": frame["story"], "anchor": 0, "focus": 2}));
    run(&mut s, "data.placeholder.add", json!({"field": "Link", "role": "hyperlink"}));
    run(&mut s, "text.select", json!({"story": frame["story"], "anchor": 1, "focus": 5}));
    assert!(refused(&mut s, "data.placeholder.add", json!({"field": "Link", "role": "hyperlink"})).contains("another placeholder"));
    run(&mut s, "text.select", json!({"story": frame["story"], "anchor": 3, "focus": 8}));
    run(&mut s, "data.placeholder.add", json!({"field": "Link", "role": "hyperlink"}));
    let index = s.active_index().unwrap();
    assert_eq!(s.documents()[index].doc.data_merge.placeholders.len(), 2);
}
