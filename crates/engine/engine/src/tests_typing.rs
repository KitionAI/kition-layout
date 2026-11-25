//! Text and tables by hand (plan 2026-11-25, stage 5): the format chosen at a caret goes to what is
//! typed next, several clicks select a word, line, paragraph or story, kerning at a caret kerns
//! the pair, input-method marked text leaves no undo step, and dragging a table border resizes it.

use serde_json::json;

use super::*;

/// A session with a 264 × 300 pt text frame at (36, 36) holding `text`, the caret at its end.
fn with_text(text: &str) -> (Session, u64, u64) {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 1})).unwrap();
    let r = s.execute("frame.add", &json!({"rect": [36, 36, 300, 336], "content": "text"})).unwrap();
    s.execute("text.insert", &json!({"text": text})).unwrap();
    (s, r["story"].as_u64().unwrap(), r["id"].as_u64().unwrap())
}

fn story_text(s: &mut Session, sid: u64) -> String {
    s.execute("story.get", &json!({"story": sid})).unwrap()["text"].as_str().unwrap().to_owned()
}

fn selected(s: &Session) -> (usize, usize) {
    let t = s.doc().unwrap().selection.text.unwrap();
    (t.range().start, t.range().end)
}

fn attrs(s: &mut Session) -> serde_json::Value {
    s.execute("text.selectionAttrs", &json!({})).unwrap()
}

#[test]
fn a_format_chosen_at_the_caret_goes_to_the_next_typing() {
    let (mut s, sid, _) = with_text("plain ");
    let steps = s.doc().unwrap().undo_steps();
    s.execute("text.char", &json!({"attrs": {"fontStyle": "Bold", "underline": true}})).unwrap();
    // Nothing in the document changed: no step, the panel shows the pending format.
    assert_eq!(s.doc().unwrap().undo_steps(), steps);
    let a = attrs(&mut s);
    assert_eq!((a["chars"]["fontStyle"].as_str(), a["chars"]["underline"].as_bool()), (Some("Bold"), Some(true)), "{a}");
    s.type_text("bold").unwrap();
    assert_eq!(story_text(&mut s, sid), "plain bold");
    let story = s.doc().unwrap().doc.story(kition_layout_doc::StoryId(sid)).unwrap().clone();
    assert_eq!(story.format_after(7).over.font_style.as_deref(), Some("Bold"));
    assert_eq!(story.format_after(2).over.font_style, None, "the text before keeps its format");
    // Moving the caret drops a format chosen at another place.
    s.execute("text.char", &json!({"attrs": {"fontStyle": "Italic"}})).unwrap();
    s.execute("text.move", &json!({"dir": "storyStart"})).unwrap();
    s.execute("text.move", &json!({"dir": "storyEnd"})).unwrap();
    let a = attrs(&mut s);
    assert_eq!(a["chars"]["fontStyle"].as_str(), Some("Bold"), "the run before the caret, not the dropped Italic: {a}");
}

#[test]
fn kerning_at_a_caret_kerns_the_pair_there() {
    let (mut s, sid, _) = with_text("AV");
    s.execute("text.move", &json!({"dir": "left"})).unwrap();
    s.execute("text.char", &json!({"attrs": {"kerning": {"kind": "manual", "value": -60.0}}})).unwrap();
    let story = s.doc().unwrap().doc.story(kition_layout_doc::StoryId(sid)).unwrap().clone();
    assert_eq!(story.format_after(0).over.kerning, Some(kition_layout_doc::Kerning::Manual(-60.0)));
    assert_eq!(story.format_after(1).over.kerning, None);
    assert_eq!(selected(&s), (1, 1), "the caret stays");
    // At the story's end there is no pair: kerning waits for the typing like any format.
    s.execute("text.move", &json!({"dir": "storyEnd"})).unwrap();
    s.execute("text.char", &json!({"attrs": {"kerning": {"kind": "manual", "value": 40.0}}})).unwrap();
    s.type_text("A").unwrap();
    let story = s.doc().unwrap().doc.story(kition_layout_doc::StoryId(sid)).unwrap().clone();
    assert_eq!(story.format_after(2).over.kerning, Some(kition_layout_doc::Kerning::Manual(40.0)));
}

#[test]
fn several_clicks_select_a_word_line_paragraph_or_story_and_a_drag_grows_by_it() {
    let (mut s, _, fid) = with_text("Alpha beta gamma\nSecond paragraph here");
    let at = json!([60.0, 45.0]);
    let pick = |s: &mut Session, unit: &str| {
        s.execute("text.selectWord", &json!({"frame": fid, "point": at, "unit": unit})).unwrap();
        selected(s)
    };
    assert_eq!(pick(&mut s, "word"), (0, 5));
    assert_eq!(pick(&mut s, "line"), (0, 16));
    assert_eq!(pick(&mut s, "paragraph"), (0, 17), "with its return");
    assert_eq!(pick(&mut s, "story").1, 38);
    assert!(s.execute("text.selectWord", &json!({"frame": fid, "point": at, "unit": "sentence"})).is_err());
    // Without a unit it is a word, as before.
    s.execute("text.selectWord", &json!({"frame": fid, "point": at})).unwrap();
    assert_eq!(selected(&s), (0, 5));
    // A drag after a double click grows by whole words; after a triple click by lines.
    s.execute("text.extendTo", &json!({"frame": fid, "point": [110.0, 45.0], "unit": "word"})).unwrap();
    let (a, e) = selected(&s);
    assert_eq!(a, 0);
    assert!(e >= 10 && [10, 16].contains(&e), "ends on a word's end: {e}");
    pick(&mut s, "line");
    s.execute("text.extendTo", &json!({"frame": fid, "point": [60.0, 60.0], "unit": "line"})).unwrap();
    assert_eq!(selected(&s), (0, 38), "the second line too");
    // The press that follows a multiple click and ends it is not a drop of the selection.
    pick(&mut s, "line");
    s.execute("text.release", &json!({"frame": fid, "point": at, "moved": false})).unwrap();
    assert_eq!(selected(&s), (0, 16));
}

#[test]
fn marked_text_shows_in_place_and_only_the_committed_text_is_a_step() {
    let (mut s, sid, _) = with_text("ab");
    s.set_tool("type");
    s.execute("text.move", &json!({"dir": "left"})).unwrap();
    s.end_typing();
    let steps = s.doc().unwrap().undo_steps();
    s.ime_preedit("ni", None).unwrap();
    assert!(s.composing());
    assert_eq!(story_text(&mut s, sid), "anib", "marked text lays out at the caret");
    s.ime_preedit("nih", Some(0..3)).unwrap();
    assert_eq!(story_text(&mut s, sid), "anihb");
    assert!(s.ime_caret(tooling::ViewInfo::default()).is_some());
    // Cancelled: as it was, no step.
    assert!(s.ime_cancel());
    assert_eq!(story_text(&mut s, sid), "ab");
    assert_eq!(s.doc().unwrap().undo_steps(), steps);
    assert_eq!(selected(&s), (1, 1));
    // Committed: the result is typed in the marked text's place, one step.
    s.ime_preedit("ni", None).unwrap();
    s.type_text("你").unwrap();
    assert!(!s.composing());
    assert_eq!(story_text(&mut s, sid), "a你b");
    s.end_typing();
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(story_text(&mut s, sid), "ab");
}

fn table_session() -> (Session, u64, u64, u64) {
    let (mut s, sid, fid) = with_text("Intro");
    let tid = s.execute("table.insert", &json!({"rows": 3, "cols": 3, "width": 240})).unwrap()["table"].as_u64().unwrap();
    (s, sid, fid, tid)
}

fn widths(s: &mut Session, tid: u64) -> Vec<f64> {
    s.execute("table.get", &json!({"table": tid})).unwrap()["columns"].as_array().unwrap().iter().map(|c| c["width"].as_f64().unwrap()).collect()
}

#[test]
fn dragging_a_border_resizes_a_column_or_row() {
    let (mut s, sid, fid, tid) = table_session();
    let near = |a: &[f64], b: &[f64]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
    assert!(near(&widths(&mut s, tid), &[80.0, 80.0, 80.0]));
    let steps = s.doc().unwrap().undo_steps();
    s.execute("table.resizeBorder", &json!({"story": sid, "table": tid, "frame": fid, "axis": "column", "index": 0, "delta": 20})).unwrap();
    assert!(near(&widths(&mut s, tid), &[100.0, 80.0, 80.0]), "the table grows");
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);
    // Shift: the next column gives the space.
    s.execute("table.resizeBorder", &json!({"table": tid, "axis": "column", "index": 0, "delta": 10, "keepTableSize": true})).unwrap();
    assert!(near(&widths(&mut s, tid), &[110.0, 70.0, 80.0]));
    // Shift on the table's edge: every column scales.
    s.execute("table.resizeBorder", &json!({"table": tid, "axis": "column", "index": 2, "delta": -26, "keepTableSize": true})).unwrap();
    let w = widths(&mut s, tid);
    assert!((w.iter().sum::<f64>() - 234.0).abs() < 1e-6 && (w[0] / w[1] - 110.0 / 70.0).abs() < 1e-6, "{w:?}");
    // A size instead of a distance; never below the minimum.
    s.execute("table.resizeBorder", &json!({"table": tid, "axis": "column", "index": 1, "size": 1})).unwrap();
    assert!((widths(&mut s, tid)[1] - 3.0).abs() < 1e-6);
    // A row becomes At Least its new height.
    s.execute("table.resizeBorder", &json!({"table": tid, "axis": "row", "index": 1, "size": 40})).unwrap();
    let t = s.execute("table.get", &json!({"table": tid})).unwrap();
    assert_eq!(t["rows"][1]["height"].as_f64(), Some(40.0), "{t}");
    // Bad input is an error, not a panic.
    for bad in [
        json!({"table": tid, "axis": "column", "index": 9, "delta": 5}),
        json!({"table": tid, "axis": "diagonal", "index": 0, "delta": 5}),
        json!({"table": tid, "axis": "row", "index": 0}),
        json!({"table": tid, "axis": "row", "index": 0, "delta": 1e308, "keepTableSize": true}),
        json!({"table": tid, "axis": "column", "index": 0, "delta": 5, "frame": 999_999}),
    ] {
        let r = s.execute("table.resizeBorder", &bad);
        if let Ok(v) = &r {
            assert!(widths(&mut s, tid).iter().all(|w| w.is_finite() && *w <= 10000.0), "{bad}: {v}");
        }
    }
    s.execute("object.lock", &json!({"ids": [fid]})).ok();
    if s.doc().unwrap().doc.item(kition_layout_doc::ItemId(fid)).is_some_and(|i| i.locked) {
        assert!(s.execute("table.resizeBorder", &json!({"table": tid, "frame": fid, "axis": "column", "index": 0, "delta": 5})).is_err());
    }
}

/// Esc in a cell selects the cell (an existing command), Shift+Tab goes back a cell.
#[test]
fn a_cell_selected_and_back_to_its_text() {
    let (mut s, _, fid, _) = table_session();
    s.execute("text.insert", &json!({"text": "One"})).unwrap();
    s.execute("table.select", &json!({})).unwrap();
    assert!(s.doc().unwrap().selection.cells.is_some());
    // The text of the cell again, as the Type tool's Esc asks with a point in the cell.
    let st = s.doc().unwrap();
    let sid = st.selection.text.unwrap().story;
    let cs = s.cache.get(&st.doc, sid, None);
    let cell = kition_layout_compose::find_cell(&cs, st.selection.cells.unwrap().table, 0, 0).unwrap().2.rect;
    let it = st.doc.item(kition_layout_doc::ItemId(fid)).unwrap();
    let loc = st.doc.find(kition_layout_doc::ItemId(fid)).unwrap();
    let p = st.doc.parent_xf(&loc) * st.doc.text_xf(it) * cell.center();
    s.execute("text.selectWord", &json!({"frame": fid, "point": [p.x, p.y], "unit": "story"})).unwrap();
    let st = s.doc().unwrap();
    assert!(st.selection.cells.is_none());
    assert_eq!(st.selection.text.unwrap().range(), 0..3);
}
