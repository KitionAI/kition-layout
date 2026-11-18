//! Regression (paragraph rules): a paragraph's rules move with its
//! text when the frame centres (or bottom-aligns, or justifies) its text vertically, instead of
//! staying where the text would have been at the top.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

fn run(doc: &mut LayoutDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

/// Rows (document pixels) with ink in the frame's area, and the rows holding a long rule.
fn rows(doc: &LayoutDocument) -> (Vec<usize>, Vec<usize>) {
    let (w, h) = (doc.width() as usize, doc.height() as usize);
    let px = doc.render(0.0, 0.0, 1.0, w as u32, h as u32).unwrap();
    let dark = |x: usize, y: usize| {
        let p = &px[(y * w + x) * 4..(y * w + x) * 4 + 4];
        p[3] > 128 && p[0] < 110 && p[1] < 110 && p[2] < 110
    };
    let mut ink = Vec::new();
    let mut rule = Vec::new();
    for y in 0..h {
        let n = (0..w).filter(|x| dark(*x, y)).count();
        if n > 0 {
            ink.push(y);
        }
        // A rule runs the column's width: more than 200 of the frame's 300 points.
        let mut run_len = 0;
        let mut longest = 0;
        for x in 0..w {
            run_len = if dark(x, y) { run_len + 1 } else { 0 };
            longest = longest.max(run_len);
        }
        if longest > 200 {
            rule.push(y);
        }
    }
    (ink, rule)
}

#[test]
fn rules_move_with_vertically_centred_text() {
    let mut doc = LayoutDocument::create("invite.klayout", 400, 600).unwrap();
    run(
        &mut doc,
        "style.paragraph.create",
        json!({ "name": "Heading", "para": { "ruleBelow": { "on": true, "weight": 3, "offset": 6, "columnWidth": true } }, "chars": { "size": 28 } }),
    );
    let frame = run(&mut doc, "frame.create", json!({ "spread": 0, "rect": [50, 100, 350, 500], "text": "Garden party" }));
    run(&mut doc, "text.select", json!({ "story": frame["story"], "anchor": 0, "focus": 12 }));
    run(&mut doc, "style.paragraph.apply", json!({ "name": "Heading" }));
    run(&mut doc, "text.exitToFrame", json!({}));
    let (_, top_rule) = rows(&doc);
    assert!(!top_rule.is_empty(), "the rule draws");
    run(&mut doc, "object.textFrameOptions", json!({ "ids": [frame["id"]], "verticalJustification": "center" }));
    let (ink, rule) = rows(&doc);
    let rule_y = *rule.first().unwrap();
    // The text is centred in the frame (rows 100 to 500): its glyphs are near the middle.
    let glyphs: Vec<usize> = ink.iter().copied().filter(|y| !rule.contains(y)).collect();
    let text_bottom = *glyphs.last().unwrap();
    assert!(text_bottom > 250, "the text is centred ({text_bottom})");
    assert!(
        rule_y > text_bottom && rule_y - text_bottom < 25,
        "the rule sits just under the text ({rule_y} under {text_bottom}), as at the top ({top_rule:?})"
    );
}
