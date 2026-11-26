//! Incremental composition (plan 2026-11-25, phase 10: typing in long documents): an edit
//! composes a threaded story again from the paragraph it changed and stops where the layout is
//! back in step with the previous one. The result must be the same as composing the whole story
//! again, value for value, after every kind of edit; and a typed character must lay out a few
//! paragraphs, not the story.

use serde_json::json;

use super::*;
use kition_layout_doc::StoryId;

/// A document of `pages` pages, each with a text frame, threaded into one story of `paras`
/// paragraphs (some with keep options, so the composer re-lays paragraphs at column ends).
fn long_story(pages: usize, paras: usize) -> (Session, u64, Vec<u64>) {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": pages, "width": 612, "height": 792, "facingPages": false})).unwrap();
    let mut frames = Vec::new();
    for page in 0..pages {
        let r = s.execute("frame.add", &json!({"spread": page, "rect": [36, 36, 576, 756], "content": "text", "caret": false})).unwrap();
        frames.push(r["id"].as_u64().unwrap());
    }
    for w in frames.windows(2) {
        s.execute("frame.thread", &json!({"from": w[0], "to": w[1]})).unwrap();
    }
    let sid = s.doc().unwrap().doc.stories.values().find(|st| st.frames.len() == pages).map(|st| st.id.0).unwrap();
    let para = "The aurora is a natural light display in the sky, seen mostly in high-latitude regions. It forms when the solar wind disturbs the magnetosphere. ";
    let text: Vec<String> = (1..=paras).map(|i| format!("{i}. {}", para.repeat(1 + i % 3))).collect();
    s.execute("text.select", &json!({"story": sid, "anchor": 0, "focus": 0})).unwrap();
    s.execute("text.insert", &json!({"text": text.join("\n")})).unwrap();
    // Widow and orphan control and keep-with-next on some paragraphs.
    for (k, attrs) in
        [(7, json!({"keepLinesTogether": true, "keepFirst": 3, "keepLast": 3})), (12, json!({"keepWithNext": 2})), (13, json!({"keepWithNext": 1}))]
    {
        let at = para_start(&mut s, sid, k);
        s.execute("text.select", &json!({"story": sid, "anchor": at, "focus": at})).unwrap();
        s.execute("text.para", &json!({"attrs": attrs})).unwrap();
    }
    (s, sid, frames)
}

fn story_text(s: &mut Session, sid: u64) -> String {
    s.execute("story.get", &json!({"story": sid})).unwrap()["text"].as_str().unwrap().to_owned()
}

/// Byte where paragraph `k` starts.
fn para_start(s: &mut Session, sid: u64, k: usize) -> usize {
    let text = story_text(s, sid);
    if k == 0 { 0 } else { text.match_indices('\n').nth(k - 1).map(|(i, _)| i + 1).unwrap() }
}

/// The session's composition of the story (made from the previous one where it could) is the one
/// a composition from scratch gives.
fn same_as_from_scratch(s: &Session, sid: u64, what: &str) {
    let d = &s.doc().unwrap().doc;
    let cached = s.cache.get(d, StoryId(sid), None);
    let fresh = compose::compose_story(d, StoryId(sid), &Default::default());
    assert!(fresh.line_count() > 0, "{what}: the story is laid out");
    let (a, b) = (format!("{:?}", *cached), format!("{fresh:?}"));
    if a != b {
        let at = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
        let from = at.saturating_sub(300);
        panic!(
            "{what}: the composition differs from one from scratch at byte {at}:\n  reused: …{}\n  scratch: …{}",
            &a[from..(at + 300).min(a.len())],
            &b[from..(at + 300).min(b.len())]
        );
    }
}

#[test]
fn an_incremental_composition_is_the_composition_from_scratch() {
    let (mut s, sid, frames) = long_story(8, 60);
    same_as_from_scratch(&s, sid, "the story as pasted");
    let mut edits = 0;
    let mut check = |s: &mut Session, what: &str| {
        same_as_from_scratch(s, sid, what);
        edits += 1;
    };
    // Typing at the start, a letter at a time (the line count of the paragraph changes on the way).
    s.execute("text.select", &json!({"story": sid, "anchor": 0, "focus": 0})).unwrap();
    for c in "Typed at the start of the story, letter by letter, long enough to add a line. ".chars() {
        s.type_text(&c.to_string()).unwrap();
        check(&mut s, &format!("typed {c:?} at the start"));
    }
    // Typing in the middle, in a paragraph with keep options and in one kept with the next.
    for k in [7, 12, 30] {
        let at = para_start(&mut s, sid, k) + 5;
        s.execute("text.select", &json!({"story": sid, "anchor": at, "focus": at})).unwrap();
        for c in ["x", "y", " ", "z"] {
            s.type_text(c).unwrap();
            check(&mut s, &format!("typed {c:?} in paragraph {k}"));
        }
    }
    // Deleting: a few characters, a paragraph break (two paragraphs become one), a whole paragraph.
    let at = para_start(&mut s, sid, 20) + 3;
    s.execute("text.select", &json!({"story": sid, "anchor": at, "focus": at + 9})).unwrap();
    s.execute("text.delete", &json!({})).unwrap();
    check(&mut s, "deleted nine characters");
    let at = para_start(&mut s, sid, 25);
    s.execute("text.select", &json!({"story": sid, "anchor": at - 1, "focus": at})).unwrap();
    s.execute("text.delete", &json!({})).unwrap();
    check(&mut s, "joined two paragraphs");
    let (a, b) = (para_start(&mut s, sid, 33), para_start(&mut s, sid, 34));
    s.execute("text.select", &json!({"story": sid, "anchor": a, "focus": b})).unwrap();
    s.execute("text.delete", &json!({})).unwrap();
    check(&mut s, "deleted a paragraph");
    // Pasting several paragraphs.
    let at = para_start(&mut s, sid, 15);
    s.execute("text.select", &json!({"story": sid, "anchor": at, "focus": at})).unwrap();
    s.execute("text.insert", &json!({"text": "Pasted one.\nPasted two, a little longer than the first.\nPasted three.\n"})).unwrap();
    check(&mut s, "pasted three paragraphs");
    // Character and paragraph formats.
    let at = para_start(&mut s, sid, 10);
    s.execute("text.select", &json!({"story": sid, "anchor": at + 4, "focus": at + 40})).unwrap();
    s.execute("text.char", &json!({"attrs": {"fontStyle": "Bold", "size": 14}})).unwrap();
    check(&mut s, "made a range bold");
    s.execute("text.para", &json!({"attrs": {"leftIndent": 24, "spaceBefore": 6}})).unwrap();
    check(&mut s, "indented a paragraph");
    s.execute("text.para", &json!({"attrs": {"align": "leftJustified"}})).unwrap();
    check(&mut s, "justified a paragraph");
    // A frame resized: the text after it flows again.
    s.execute("select.set", &json!({"ids": [frames[2]]})).unwrap();
    s.execute("transform.resize", &json!({"ids": [frames[2]], "from": [36, 36, 576, 756], "to": [36, 36, 576, 600]})).unwrap();
    check(&mut s, "made the third frame shorter");
    s.execute("transform.resize", &json!({"ids": [frames[2]], "from": [36, 36, 576, 600], "to": [36, 36, 576, 756]})).unwrap();
    check(&mut s, "made it tall again");
    // A wrap object on page four, then moved.
    let r = s.execute("frame.add", &json!({"spread": 3, "rect": [200, 300, 400, 420]})).unwrap();
    let pic = r["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [pic]})).unwrap();
    s.execute("object.textWrap", &json!({"ids": [pic], "mode": "boundingBox"})).unwrap();
    check(&mut s, "added a wrap object");
    s.execute("transform.move", &json!({"ids": [pic], "dx": 0, "dy": 150})).unwrap();
    check(&mut s, "moved the wrap object");
    // Undo and redo jump between versions.
    for _ in 0..3 {
        s.execute("edit.undo", &json!({})).unwrap();
        check(&mut s, "undid an edit");
    }
    s.execute("edit.redo", &json!({})).unwrap();
    check(&mut s, "redid an edit");
    // Typing at the very end of the story, in the overset text.
    let end = story_text(&mut s, sid).len();
    s.execute("text.select", &json!({"story": sid, "anchor": end, "focus": end})).unwrap();
    for c in ["!", "\n", "E", "n", "d"] {
        s.type_text(c).unwrap();
        check(&mut s, &format!("typed {c:?} at the end"));
    }
    assert!(edits > 100, "{edits} edits checked");
}

#[test]
fn a_typed_character_lays_out_a_few_paragraphs_of_a_long_story() {
    let (mut s, sid, _) = long_story(10, 120);
    let paragraphs = story_text(&mut s, sid).matches('\n').count() + 1;
    let at = para_start(&mut s, sid, 40) + 10;
    s.execute("text.select", &json!({"story": sid, "anchor": at, "focus": at})).unwrap();
    s.type_text("a").unwrap();
    let mut most = 0;
    for c in "bcdefgh".chars() {
        let before = (compose::paragraphs_laid_on_this_thread(), compose::compositions_on_this_thread());
        s.type_text(&c.to_string()).unwrap();
        // What the editor reads after a key: the layout, the caret, a picture of the page.
        let d = &s.doc().unwrap().doc;
        let _ = s.cache.get(d, StoryId(sid), None);
        let laid = compose::paragraphs_laid_on_this_thread() - before.0;
        let composed = compose::compositions_on_this_thread() - before.1;
        assert!(composed <= 1, "a key composes the story once, not {composed} times");
        most = most.max(laid);
    }
    assert!(paragraphs >= 100, "a long story: {paragraphs} paragraphs");
    // The paragraph typed in, and at most the one after it to see the layout is back in step.
    assert!(most <= 2, "a typed character laid out {most} of {paragraphs} paragraphs");
    same_as_from_scratch(&s, sid, "after typing");
}
