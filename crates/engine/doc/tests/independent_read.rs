//! Small engine regressions found while reading command dependencies for C8-layout-read.
use kition_layout_doc::{Document, build::NewDocument};

#[test]
fn moving_a_page_with_a_kept_multipage_spread_refuses_without_mutation() {
    let mut doc = Document::new(&NewDocument { pages: 5, ..Default::default() });
    doc.page_to_spread(3, 1).unwrap();
    assert!(doc.spreads.iter().any(|spread| !spread.allow_shuffle && spread.pages.len() == 3));
    let before = serde_json::to_value(&doc).unwrap();
    let error = doc.move_page(3, 0).unwrap_err();
    assert!(error.to_string().contains("unsupported"), "{error}");
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
}
