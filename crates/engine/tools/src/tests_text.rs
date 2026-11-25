//! The Type tool and the loaded text cursor by hand (plan 2026-11-25, stage 5): several clicks in a
//! row, an empty frame becoming a text frame, table borders and keys, kerning keys, input-method
//! marked text, and clicking the linked frame with the loaded cursor.

use kition_layout_compose::Cache;
use kition_layout_doc::build::NewDocument;
use kition_layout_doc::{CellAddr, Document, Item, ItemId, ParaFormat, Selection, Shape, SpreadRef, TableSel, TextSel};
use kition_layout_geom::{Point, Rect, Unit, shapes};
use serde_json::json;

use super::*;

fn ctx<'a>(d: &'a Document, s: &'a Selection, c: &'a Cache, l: &'a CanvasLayout) -> ToolContext<'a> {
    ToolContext { doc: d, selection: s, cache: c, layout: l, zoom: 1.0, layer: d.default_layer(), snap: SnapView::OFF, unit: Unit::Points }
}

fn add_rect(doc: &mut Document, rect: Rect) -> ItemId {
    let layer = doc.default_layer();
    let id = ItemId(doc.alloc());
    doc.insert_item(SpreadRef::Doc(0), Item::new(id, layer, Shape::Rectangle, shapes::rectangle(rect)), None).unwrap();
    id
}

fn exec(id: &str, p: serde_json::Value) -> Vec<Action> {
    vec![Action::Exec(id.into(), p)]
}

#[test]
fn clicks_in_a_row_select_a_word_a_line_a_paragraph_and_the_story() {
    let mut d = Document::new(&NewDocument::default());
    let lid = d.default_layer();
    let (fid, _) = d.add_text_frame(SpreadRef::Doc(0), Rect::new(100.0, 100.0, 300.0, 200.0), lid, "Two words", ParaFormat::default()).unwrap();
    let (s, c, l) = (Selection::default(), Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let off = l.xf(SpreadRef::Doc(0)).translation();
    let (x, y) = (150.0 + off.x, 110.0 + off.y);
    let press = |n: u8| PointerEvent::new(PointerKind::Down, x, y).with_clicks(n);
    let mut t = create("type");
    let unit = |a: Vec<Action>| match a.as_slice() {
        [Action::Exec(id, p)] if id == "text.selectWord" => p["unit"].as_str().map(str::to_string),
        [Action::Exec(id, _)] if id == "text.placeCaret" => None,
        other => panic!("{other:?}"),
    };
    assert_eq!(unit(t.pointer(&cx, &press(1))), None);
    for (n, want) in [(2, "word"), (3, "line"), (4, "paragraph"), (5, "story"), (9, "story")] {
        assert_eq!(unit(t.pointer(&cx, &press(n))).as_deref(), Some(want), "{n} clicks");
    }
    // The editor's own double-click event after counted presses (after the second press, after a
    // drag, after the fourth) changes nothing: the press did it.
    assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::DoubleClick, x + 40.0, y)).is_empty());
    // A double click sent as such (an agent, an editor that counts nothing) selects a word.
    t.pointer(&cx, &press(1));
    t.pointer(&cx, &PointerEvent::new(PointerKind::Up, x, y));
    assert_eq!(unit(t.pointer(&cx, &PointerEvent::new(PointerKind::DoubleClick, x, y))).as_deref(), Some("word"));
    // Moving while pressed extends the selection by the unit the clicks chose.
    let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, x + 40.0, y));
    assert!(matches!(&a[..], [Action::Exec(id, p)] if id == "text.extendTo" && p["unit"] == "word" && p["frame"] == fid.0), "{a:?}");
    t.pointer(&cx, &PointerEvent::new(PointerKind::Up, x + 40.0, y));
    t.pointer(&cx, &press(3));
    let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, x + 40.0, y));
    assert!(matches!(&a[..], [Action::Exec(id, p)] if id == "text.extendTo" && p["unit"] == "line"), "{a:?}");
    // One click, then a drag: a character at a time.
    t.pointer(&cx, &PointerEvent::new(PointerKind::Up, x + 40.0, y));
    t.pointer(&cx, &press(1));
    let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, x + 40.0, y));
    assert!(matches!(&a[..], [Action::Exec(id, p)] if id == "text.extendTo" && p.get("unit").is_none()), "{a:?}");
    // The count arrives in the event's JSON, and is optional there.
    let ev: PointerEvent = serde_json::from_value(json!({"kind": "down", "pos": [1.0, 2.0], "clicks": 3})).unwrap();
    assert_eq!(ev.click_count(), 3);
    let ev: PointerEvent = serde_json::from_value(json!({"kind": "down", "pos": [1.0, 2.0]})).unwrap();
    assert_eq!(ev.click_count(), 1);
}

/// A drag that starts in an empty rectangle draws a new text frame over it (the rectangle stays as
/// it is); a click turns the rectangle into a text frame with the caret in it.
#[test]
fn type_tool_click_converts_an_empty_frame_and_a_drag_draws_over_it() {
    let mut d = Document::new(&NewDocument::default());
    let id = add_rect(&mut d, Rect::new(100.0, 100.0, 300.0, 300.0));
    let (s, c, l) = (Selection::default(), Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let off = l.xf(SpreadRef::Doc(0)).translation();
    let at = |x: f64, y: f64| (x + off.x, y + off.y);
    let mut t = create("type");
    let (x, y) = at(120.0, 120.0);
    assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Down, x, y)).is_empty());
    let (x1, y1) = at(280.0, 200.0);
    let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, x1, y1));
    assert!(matches!(&a[..], [Action::Begin(_), Action::Preview(cmd, p)] if cmd == "frame.add" && p["content"] == "text"), "{a:?}");
    assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, x1, y1)), vec![Action::Commit]);
    assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Down, x, y)).is_empty());
    let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Up, x, y));
    assert!(matches!(&a[..], [Action::Exec(cmd, p)] if cmd == "text.placeCaret" && p["frame"] == id.0), "{a:?}");
    // A release away from the press with no drag in between isn't a click: nothing changes.
    assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Down, x, y)).is_empty());
    assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, x1, y1)).is_empty());
}

/// A 300 × 300 text frame at (100, 100), turned `degrees` about its centre, holding a 3 × 3 table.
fn table_frame(degrees: f64) -> (Document, ItemId) {
    let mut d = Document::new(&NewDocument::default());
    let layer = d.default_layer();
    let (id, sid) = d.add_text_frame(SpreadRef::Doc(0), Rect::new(100.0, 100.0, 400.0, 400.0), layer, "", ParaFormat::default()).unwrap();
    let tid = d.alloc();
    d.story_mut(sid).unwrap().insert_table(0, kition_layout_doc::Table::new(tid, 3, 3, 0, 0, 240.0));
    d.item_mut(id).unwrap().xf = kition_layout_geom::Affine::rotate_about(degrees.to_radians(), Point::new(250.0, 250.0));
    (d, id)
}

/// In a table cell Shift+Tab goes back a cell and Esc switches between the cell's text and the
/// cell; out of a table Tab types a tab and Esc leaves the text.
#[test]
fn type_tool_keys_in_a_table_cell() {
    let (d, id) = table_frame(0.0);
    let sid = d.item(id).unwrap().text_frame().unwrap().story;
    let table = *d.story(sid).unwrap().tables.keys().next().unwrap();
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let cell = CellAddr { table, row: 1, col: 1 };
    let s = Selection::text(TextSel { cell: Some(cell), frame: Some(id), ..TextSel::caret(sid, 0) });
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("type");
    let shift = Mods { shift: true, ..Default::default() };
    assert_eq!(t.key(&cx, ToolKey::Tab, shift), exec("table.prevCell", json!({})));
    assert_eq!(t.key(&cx, ToolKey::Tab, Mods::default()), exec("text.insert", json!({"text": "\t"})));
    // The caret in a cell: Esc selects the cell.
    assert_eq!(t.key(&cx, ToolKey::Escape, Mods::default()), exec("table.select", json!({})));
    // The cell selected (as table.select leaves it): Esc selects its text, at a point inside it.
    let s = Selection {
        cells: Some(TableSel { story: sid, table, range: kition_layout_doc::CellRange::cell(1, 1) }),
        text: Some(TextSel { cell: Some(cell), frame: Some(id), ..TextSel::caret(sid, 0) }),
        ..Default::default()
    };
    let cx = ctx(&d, &s, &c, &l);
    let a = t.key(&cx, ToolKey::Escape, Mods::default());
    let [Action::Exec(cmd, p)] = a.as_slice() else { panic!("{a:?}") };
    assert_eq!((cmd.as_str(), &p["unit"], &p["frame"]), ("text.selectWord", &json!("story"), &json!(id.0)));
    let cs = c.get(&d, sid, None);
    let placed = kition_layout_compose::find_cell(&cs, table, 1, 1).unwrap().2.rect;
    let pt = Point::new(p["point"][0].as_f64().unwrap(), p["point"][1].as_f64().unwrap());
    assert!(placed.contains(pt), "{pt:?} in {placed:?}");
    // Out of a table.
    let s = Selection::text(TextSel::caret(sid, 0));
    let cx = ctx(&d, &s, &c, &l);
    assert_eq!(t.key(&cx, ToolKey::Tab, shift), exec("text.insert", json!({"text": "\t"})));
    assert!(matches!(t.key(&cx, ToolKey::Escape, Mods::default()).first(), Some(Action::SwitchTool(_))));
}

#[test]
fn type_tool_drags_a_table_column_border() {
    for degrees in [0.0, 90.0] {
        let (d, id) = table_frame(degrees);
        let (s, c, l) = (Selection::default(), Cache::new(), CanvasLayout::new(&d, false));
        let cx = ctx(&d, &s, &c, &l);
        let it = d.item(id).unwrap();
        let sid = it.text_frame().unwrap().story;
        let cs = c.get(&d, sid, None);
        let cell = cs.frames[0].tables[0].cell(1, 0).unwrap();
        let xf = cx.item_canvas_xf(id).unwrap() * d.text_xf(it);
        let on_border = xf * Point::new(cell.rect.x1 + 1.0, (cell.rect.y0 + cell.rect.y1) / 2.0);
        let dragged = xf * Point::new(cell.rect.x1 + 21.0, (cell.rect.y0 + cell.rect.y1) / 2.0);
        let mut t = create("type");
        let expected = if degrees == 0.0 { Cursor::ResizeH } else { Cursor::ResizeV };
        assert_eq!(t.cursor(&cx, on_border, Mods::default()), expected);
        // A click with a 1 px wobble on the border changes nothing.
        assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Down, on_border.x, on_border.y)).is_empty());
        assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, on_border.x + 1.0, on_border.y)).is_empty());
        assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, on_border.x + 1.0, on_border.y)).is_empty());
        // A press on the border starts a resize, not a text selection.
        assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Down, on_border.x, on_border.y)).is_empty());
        assert!(t.busy());
        let shift = Mods { shift: true, ..Default::default() };
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, dragged.x, dragged.y).with_mods(shift));
        assert_eq!(a[0], Action::Begin("Resize Column".into()));
        let Action::Preview(cmd, p) = &a[1] else { panic!("{a:?}") };
        assert_eq!(
            (cmd.as_str(), &p["axis"], &p["index"], &p["frame"], &p["keepTableSize"]),
            ("table.resizeBorder", &json!("column"), &json!(0), &json!(id.0), &json!(true))
        );
        assert!((p["delta"].as_f64().unwrap() - 20.0).abs() < 1e-6, "{p}");
        assert_eq!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, dragged.x, dragged.y)), vec![Action::Commit]);
        // A row border.
        let below = xf * Point::new((cell.rect.x0 + cell.rect.x1) / 2.0, cell.rect.y1 + 1.0);
        let lower = xf * Point::new((cell.rect.x0 + cell.rect.x1) / 2.0, cell.rect.y1 + 11.0);
        t.pointer(&cx, &PointerEvent::new(PointerKind::Down, below.x, below.y));
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Drag, lower.x, lower.y));
        let Action::Preview(_, p) = &a[1] else { panic!("{a:?}") };
        assert_eq!((&p["axis"], &p["index"], &p["keepTableSize"]), (&json!("row"), &json!(1), &json!(false)));
        // Esc cancels a drag in progress.
        assert_eq!(t.key(&cx, ToolKey::Escape, Mods::default()), vec![Action::Cancel]);
        assert!(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, lower.x, lower.y)).is_empty());
        // Inside the cell the Type tool places the caret as usual.
        let inside = xf * Point::new(cell.rect.x0 + 20.0, (cell.rect.y0 + cell.rect.y1) / 2.0);
        let a = t.pointer(&cx, &PointerEvent::new(PointerKind::Down, inside.x, inside.y));
        assert!(matches!(&a[..], [Action::Exec(cmd, _)] if cmd == "text.placeCaret"), "{a:?}");
    }
}

#[test]
fn alt_arrows_kern_the_pair_at_the_caret_and_track_a_selection() {
    let mut d = Document::new(&NewDocument::default());
    let lid = d.default_layer();
    let (_, sid) = d.add_text_frame(SpreadRef::Doc(0), Rect::new(100.0, 100.0, 300.0, 200.0), lid, "Wa ve", ParaFormat::default()).unwrap();
    d.story_mut(sid).unwrap().format_chars(0..1, |f| f.over.kerning = Some(kition_layout_doc::Kerning::Manual(-40.0)));
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let alt = Mods { alt: true, ..Default::default() };
    let mut t = create("type");
    // The caret between W and a: the pair's kerning, from its manual value.
    let s = Selection::text(TextSel::caret(sid, 1));
    let cx = ctx(&d, &s, &c, &l);
    assert_eq!(t.key(&cx, ToolKey::Right, alt), exec("text.char", json!({"attrs": {"kerning": {"kind": "manual", "value": -20.0}}})));
    let cmd_alt = Mods { alt: true, cmd: true, ..Default::default() };
    assert_eq!(t.key(&cx, ToolKey::Left, cmd_alt), exec("text.char", json!({"attrs": {"kerning": {"kind": "manual", "value": -140.0}}})));
    // A caret at the story's start or end has no pair.
    for at in [0, 5] {
        let s = Selection::text(TextSel::caret(sid, at));
        assert!(t.key(&ctx(&d, &s, &c, &l), ToolKey::Right, alt).is_empty(), "{at}");
    }
    // A selection: its tracking.
    let s = Selection::text(TextSel { anchor: 3, focus: 5, ..TextSel::caret(sid, 3) });
    let cx = ctx(&d, &s, &c, &l);
    assert_eq!(t.key(&cx, ToolKey::Right, alt), exec("text.char", json!({"attrs": {"tracking": 20.0}})));
    // Alt+Shift still extends by words.
    let alt_shift = Mods { alt: true, shift: true, ..Default::default() };
    assert_eq!(t.key(&cx, ToolKey::Right, alt_shift), exec("text.move", json!({"dir": "right", "extend": true, "word": true})));
}

#[test]
fn input_method_marked_text_previews_at_the_caret_and_leaves_no_step() {
    let mut d = Document::new(&NewDocument::default());
    let lid = d.default_layer();
    let (_, sid) = d.add_text_frame(SpreadRef::Doc(0), Rect::new(100.0, 100.0, 300.0, 200.0), lid, "ab", ParaFormat::default()).unwrap();
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let s = Selection::text(TextSel::caret(sid, 1));
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("type");
    assert!(!t.composing());
    let caret = t.ime_caret(&cx).unwrap();
    assert!(caret.1.y > caret.0.y, "the caret line runs down: {caret:?}");
    let a = t.ime_preedit(&cx, "ni", None);
    assert_eq!(a, vec![Action::Begin("Type".into()), Action::Preview("text.insert".into(), json!({"text": "ni", "raw": true}))]);
    assert!(t.composing());
    // Each update replaces the marked text in the same interaction.
    assert_eq!(t.ime_preedit(&cx, "你\u{7}", Some(0..1)), vec![Action::Preview("text.insert".into(), json!({"text": "你", "raw": true}))]);
    // Keys belong to the input method while it composes.
    assert!(t.key(&cx, ToolKey::Left, Mods::default()).is_empty());
    // Cleared: the preview goes, and nothing is left to undo.
    assert_eq!(t.ime_preedit(&cx, "", None), vec![Action::Cancel]);
    assert!(!t.composing());
    assert!(t.ime_cancel().is_empty());
    // Marked text shows underlined where the preview typed it.
    t.ime_preedit(&cx, "x", None);
    let mut d2 = d.clone();
    d2.story_mut(sid).unwrap().replace(1..1, "x");
    let s2 = Selection::text(TextSel::caret(sid, 2));
    let c2 = Cache::new();
    let o = t.overlays(&ctx(&d2, &s2, &c2, &l));
    assert!(matches!(o.as_slice(), [Overlay::Line { .. }]), "{o:?}");
    assert_eq!(t.ime_cancel(), vec![Action::Cancel]);
}

#[test]
fn input_method_character_ranges_become_byte_ranges() {
    use crate::text::char_range_to_bytes;
    assert_eq!(char_range_to_bytes("雅楽", 0..2), Some(0..6));
    assert_eq!(char_range_to_bytes("がg", 1..2), Some(3..4));
    assert_eq!(char_range_to_bytes("がg", 2..2), Some(4..4));
    // Past the end or backwards: no clause.
    assert_eq!(char_range_to_bytes("が", 1..3), None);
    #[allow(clippy::reversed_empty_ranges)]
    let backwards = 1..0;
    assert_eq!(char_range_to_bytes("がg", backwards), None);
}

/// x's out port links to y. Loaded from x's out port, a click on y breaks the link; loaded from
/// y's in port, a click on x does. The cursor shows what a click does.
#[test]
fn loaded_click_on_the_linked_frame_unthreads() {
    let mut d = Document::new(&NewDocument { pages: 2, ..Default::default() });
    let lid = d.default_layer();
    let (x, _) = d.add_text_frame(SpreadRef::Doc(0), Rect::new(300.0, 100.0, 450.0, 160.0), lid, "x", ParaFormat::default()).unwrap();
    let (y, _) = d.add_text_frame(SpreadRef::Doc(0), Rect::new(300.0, 400.0, 450.0, 500.0), lid, "", ParaFormat::default()).unwrap();
    d.thread(x, y).unwrap();
    let e = add_rect(&mut d, Rect::new(100.0, 400.0, 250.0, 500.0));
    let pic = ItemId(d.alloc());
    let mut graphic = Item::new(pic, lid, Shape::Rectangle, shapes::rectangle(Rect::new(100.0, 600.0, 250.0, 700.0)));
    graphic.content = kition_layout_doc::Content::Graphic(
        serde_json::from_value(json!({"asset": 1, "size": [10.0, 10.0], "xf": [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]})).unwrap(),
    );
    d.insert_item(SpreadRef::Doc(0), graphic, None).unwrap();
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let s = Selection::default();
    let cx = ctx(&d, &s, &c, &l);
    let at = |px: f64, py: f64| l.to_canvas(SpreadRef::Doc(0), Point::new(px, py));
    let click = |t: &mut dyn Tool, p: Point| {
        let mut v = t.pointer(&cx, &PointerEvent::new(PointerKind::Down, p.x, p.y));
        v.extend(t.pointer(&cx, &PointerEvent::new(PointerKind::Up, p.x, p.y)));
        v
    };
    let mut t = thread::LoadedTextTool::new(x, thread::Side::Out);
    assert_eq!(t.cursor(&cx, at(375.0, 450.0), Mods::default()), Cursor::Unthread);
    assert_eq!(t.cursor(&cx, at(175.0, 450.0), Mods::default()), Cursor::ThreadLink);
    assert_eq!(t.cursor(&cx, at(175.0, 650.0), Mods::default()), Cursor::NotAllowed);
    assert_eq!(t.cursor(&cx, at(500.0, 750.0), Mods::default()), Cursor::LoadedText);
    assert_eq!(t.cursor(&cx, at(375.0, 130.0), Mods::default()), Cursor::LoadedText);
    assert_eq!(
        click(&mut t, at(375.0, 450.0)),
        vec![Action::SwitchTool("selection".into()), Action::Exec("frame.unthread".into(), json!({"frame": x.0, "side": "out"}))]
    );
    assert!(matches!(click(&mut t, at(175.0, 450.0)).as_slice(), [Action::Exec(id, p)] if id == "frame.thread" && p["to"] == e.0));
    assert!(click(&mut t, at(175.0, 650.0)).is_empty(), "a picture takes no text");
    // From y's in port, x is the frame before.
    let mut t = thread::LoadedTextTool::new(y, thread::Side::In);
    assert_eq!(t.cursor(&cx, at(375.0, 130.0), Mods::default()), Cursor::Unthread);
    assert_eq!(
        click(&mut t, at(375.0, 130.0)),
        vec![Action::SwitchTool("selection".into()), Action::Exec("frame.unthread".into(), json!({"frame": y.0, "side": "in"}))]
    );
}
