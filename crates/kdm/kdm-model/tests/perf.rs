//! Rough costs of the bridge on a long story (design note 2.7 and risk R-D1): a 50-page story's
//! keystroke written as a difference, and a full read. Measured, printed, and bounded loosely
//! (debug builds, shared machines); DE4 sets the real budgets.

mod common;

use std::time::Instant;

use common::*;
use kition_layout_doc::geom::Rect;
use kition_layout_doc::{ParaFormat, SpreadRef};

#[test]
fn a_keystroke_in_a_fifty_page_story() {
    let mut doc = blank();
    let lid = doc.default_layer();
    // About 50 pages of text: 1,500 paragraphs of 100 characters.
    let paragraph = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore.";
    let text: Vec<&str> = std::iter::repeat_n(paragraph, 1500).collect();
    let (_, sid) = doc.add_text_frame(SpreadRef::Doc(0), Rect::new(36.0, 36.0, 576.0, 756.0), lid, &text.join("\n"), ParaFormat::default()).unwrap();
    {
        let s = doc.story_mut(sid).unwrap();
        for k in (0..s.len()).step_by(997) {
            let end = (k + 40).min(s.len());
            let (a, b) = (kition_layout_doc::story::floor_char_boundary(&s.text, k), kition_layout_doc::story::floor_char_boundary(&s.text, end));
            s.format_chars(a..b, |f| f.over.font_style = Some("Bold".into()));
        }
    }
    let started = Instant::now();
    let (mut replica, mut blobs, mut ids) = replica_of(&doc);
    let write_all = started.elapsed();
    let mut after = doc.clone();
    let middle = kition_layout_doc::story::floor_char_boundary(&after.stories[&sid].text, after.stories[&sid].len() / 2);
    after.story_mut(sid).unwrap().insert(middle, "k");
    let started = Instant::now();
    assert!(apply_change(&mut replica, &mut blobs, &doc, &after, &mut ids));
    let keystroke = started.elapsed();
    let started = Instant::now();
    let back = read(&replica, &blobs);
    let read_all = started.elapsed();
    assert_same("the long story", &after, &back);
    println!("150,000 characters: write {write_all:?}, keystroke {keystroke:?}, full read {read_all:?}");
    assert!(keystroke.as_millis() < 2_000, "a keystroke took {keystroke:?}");
}
