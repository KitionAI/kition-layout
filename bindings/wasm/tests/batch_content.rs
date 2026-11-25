//! C8-layout review: raw discriminator writes cannot silently delete their source.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::{KitDocument, document_content, session::model::Exchange};
use kition_layout_kit::LayoutDocument;
use serde_json::json;

#[test]
fn inconsistent_content_conversion_refuses_and_rolls_back() {
    for content in ["group", "graphic", "unassigned"] {
        let mut doc = LayoutDocument::create("Review.klayout", 612, 792).unwrap();
        let frame = doc.execute("frame.add", json!({"rect":[20,20,200,200],"content":"text","text":"Keep this story"})).unwrap();
        let before = doc.inspect().unwrap();
        let mut exchange = Exchange::new();
        let result = exchange.batch(
            &mut doc,
            &json!({"ops":[
                {"op":"set","target":"layout","field":"title","value":"Must roll back"},
                {"op":"set","target":frame["id"].to_string(),"field":"content","value":content}
            ]}),
            false,
        );
        assert!(result.is_err(), "inconsistent {content} conversion was accepted: {result:?}");
        assert_eq!(before, doc.inspect().unwrap(), "failed conversion changed the document");
        exchange.batch(&mut doc, &json!({"ops":[{"op":"set","target":frame["id"].to_string(),"field":"opacity","value":0.5}]}), false).unwrap();
        let reopened = LayoutDocument::open("Review.klayout", &doc.save(None).unwrap()).unwrap();
        assert_eq!(document_content(doc.inspect().unwrap(), &["/selection"]), document_content(reopened.inspect().unwrap(), &["/selection"]));
    }
}
