//! The Selection tool and a frame's placed graphic (plan 2026-11-25, stage 6): the content
//! grabber, double-clicking between frame and graphic, the graphic's own handles, its ghost while
//! it moves, Escape back to the frame, double-clicking a handle to fit the frame, and the key
//! object for Align.

use kition_layout_compose::Cache;
use kition_layout_doc::build::NewDocument;
use kition_layout_doc::{AssetId, Content, Document, Graphic, Item, ItemId, Selection, Shape, SpreadRef};
use kition_layout_geom::{Affine, Point, Rect, Unit, Vec2, shapes};
use serde_json::json;

use super::*;

fn ctx<'a>(d: &'a Document, s: &'a Selection, c: &'a Cache, l: &'a CanvasLayout) -> ToolContext<'a> {
    ToolContext { doc: d, selection: s, cache: c, layout: l, zoom: 1.0, layer: d.default_layer(), snap: SnapView::OFF, unit: Unit::Points }
}

/// A 200 × 100 pt frame at (100, 100) holding a graphic twice its size whose top left is at the
/// frame's (it shows cropped).
fn with_graphic() -> (Document, ItemId) {
    let mut d = Document::new(&NewDocument::default());
    let layer = d.default_layer();
    let id = ItemId(d.alloc());
    let mut it = Item::new(id, layer, Shape::Rectangle, shapes::rectangle(Rect::new(100.0, 100.0, 300.0, 200.0)));
    it.content = Content::Graphic(Graphic {
        asset: AssetId(1),
        size: (200.0, 100.0),
        xf: Affine::translate((100.0, 100.0)) * Affine::scale(2.0),
        auto_fit: Default::default(),
        fit_align: 4,
        crop: [0.0; 4],
    });
    d.insert_item(SpreadRef::Doc(0), it, None).unwrap();
    (d, id)
}

fn rect(d: &mut Document, r: Rect) -> ItemId {
    let layer = d.default_layer();
    let id = ItemId(d.alloc());
    d.insert_item(SpreadRef::Doc(0), Item::new(id, layer, Shape::Rectangle, shapes::rectangle(r)), None).unwrap();
    id
}

fn at(l: &CanvasLayout, x: f64, y: f64) -> Point {
    let off = l.xf(SpreadRef::Doc(0)).translation();
    Point::new(x + off.x, y + off.y)
}

fn ev(kind: PointerKind, p: Point) -> PointerEvent {
    PointerEvent::new(kind, p.x, p.y)
}

fn content(id: ItemId) -> Selection {
    Selection { content: true, ..Selection::items(vec![id]) }
}

#[test]
fn the_content_grabber_takes_hold_of_the_graphic() {
    let (d, id) = with_graphic();
    let (s, c, l) = (Selection::default(), Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("selection");
    // Hovering the frame shows its grabber at the middle, and the hand over it.
    let mid = at(&l, 200.0, 150.0);
    t.pointer(&cx, &ev(PointerKind::Move, at(&l, 120.0, 120.0)));
    let shown = t.overlays(&cx);
    assert!(matches!(shown.as_slice(), [Overlay::Path { .. }]), "{shown:?}");
    assert_eq!(t.cursor(&cx, mid, Mods::default()), Cursor::ContentHand);
    assert_eq!(t.cursor(&cx, at(&l, 120.0, 120.0), Mods::default()), Cursor::Arrow);
    // A press on it selects the graphic, not the frame.
    let a = t.pointer(&cx, &ev(PointerKind::Down, mid));
    assert_eq!(a, vec![Action::Exec("select.set".into(), json!({"ids": [id.0], "content": true}))]);
    // With the graphic selected, a drag moves it inside the frame, its ghost showing.
    let s = content(id);
    let cx = ctx(&d, &s, &c, &l);
    let a = t.pointer(&cx, &ev(PointerKind::Drag, at(&l, 230.0, 140.0)));
    assert_eq!(a.first(), Some(&Action::Begin("Move Content".into())));
    assert!(
        matches!(a.get(1), Some(Action::Preview(cmd, p)) if cmd == "transform.move" && p["content"] == true && p["dx"] == 30.0 && p["dy"] == -10.0),
        "{a:?}"
    );
    assert_eq!(t.ghost(), Some(id));
    assert_eq!(t.cursor(&cx, mid, Mods::default()), Cursor::HandGrab);
    assert_eq!(t.pointer(&cx, &ev(PointerKind::Up, at(&l, 230.0, 140.0))), vec![Action::Commit]);
    assert_eq!(t.ghost(), None);
}

#[test]
fn a_double_click_toggles_frame_and_graphic() {
    let (d, id) = with_graphic();
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let p = at(&l, 120.0, 120.0);
    let mut t = create("selection");
    // The editor counts: the second press toggles; its own double click after it does nothing.
    let s = Selection::items(vec![id]);
    let cx = ctx(&d, &s, &c, &l);
    let a = t.pointer(&cx, &ev(PointerKind::Down, p).with_clicks(2));
    assert_eq!(a, vec![Action::Exec("select.set".into(), json!({"ids": [id.0], "content": true}))]);
    t.pointer(&cx, &ev(PointerKind::Up, p));
    assert!(t.pointer(&cx, &ev(PointerKind::DoubleClick, p)).is_empty());
    // From the graphic back to the frame.
    let s = content(id);
    let cx = ctx(&d, &s, &c, &l);
    let a = t.pointer(&cx, &ev(PointerKind::Down, p).with_clicks(2));
    assert_eq!(a, vec![Action::Exec("select.set".into(), json!({"ids": [id.0], "content": false}))]);
    // A double click sent as such (nothing counted) toggles too.
    let s = Selection::items(vec![id]);
    let cx = ctx(&d, &s, &c, &l);
    t.pointer(&cx, &ev(PointerKind::Down, p));
    t.pointer(&cx, &ev(PointerKind::Up, p));
    let a = t.pointer(&cx, &ev(PointerKind::DoubleClick, p));
    assert_eq!(a, vec![Action::Exec("select.set".into(), json!({"ids": [id.0], "content": true}))]);
    // The editor's double click event can arrive before the counted second press: that press
    // does not toggle back.
    let s = content(id);
    let cx = ctx(&d, &s, &c, &l);
    let a = t.pointer(&cx, &ev(PointerKind::Down, p).with_clicks(2));
    assert!(!a.contains(&Action::Exec("select.set".into(), json!({"ids": [id.0], "content": false}))), "{a:?}");
    t.pointer(&cx, &ev(PointerKind::Up, p));
}

#[test]
fn the_graphics_own_handles_size_it() {
    let (d, id) = with_graphic();
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let s = content(id);
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("selection");
    // The graphic's box runs to (500, 300); its bottom-right handle is there, not the frame's.
    let corner = at(&l, 500.0, 300.0);
    assert_eq!(t.cursor(&cx, corner, Mods::default()), Cursor::ResizeNwSe);
    assert_eq!(t.pointer(&cx, &ev(PointerKind::Down, corner)), vec![Action::Begin("Resize Content".into())]);
    let a = t.pointer(&cx, &ev(PointerKind::Drag, at(&l, 400.0, 250.0)));
    assert_eq!(a, vec![Action::Preview("transform.set".into(), json!({"ids": [id.0], "content": true, "bounds": [100.0, 100.0, 400.0, 250.0]}))]);
    assert_eq!(t.ghost(), Some(id));
    // Shift keeps the graphic's proportions.
    let shift = Mods { shift: true, ..Mods::default() };
    let a = t.pointer(&cx, &ev(PointerKind::Drag, at(&l, 300.0, 280.0)).with_mods(shift));
    assert!(matches!(&a[..], [Action::Preview(_, p)] if p["bounds"] == json!([100.0, 100.0, 460.0, 280.0])), "{a:?}");
    assert_eq!(t.pointer(&cx, &ev(PointerKind::Up, corner)), vec![Action::Commit]);
    // The frame's corner (300, 200) is no handle now: a press there is on the graphic.
    let a = t.pointer(&cx, &ev(PointerKind::Down, at(&l, 299.0, 199.0)));
    assert!(a.is_empty(), "{a:?}");
    t.pointer(&cx, &ev(PointerKind::Up, at(&l, 299.0, 199.0)));
}

#[test]
fn escape_goes_back_to_the_frame_and_the_arrows_move_the_graphic() {
    let (d, id) = with_graphic();
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let s = content(id);
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("selection");
    assert_eq!(t.key(&cx, ToolKey::Escape, Mods::default()), vec![Action::Exec("select.set".into(), json!({"ids": [id.0]}))]);
    let a = t.key(&cx, ToolKey::Right, Mods::default());
    assert!(matches!(&a[..], [Action::Exec(cmd, p)] if cmd == "transform.move" && p["content"] == true && p["dx"] == 1.0), "{a:?}");
}

#[test]
fn a_double_click_on_a_handle_fits_the_frame_to_its_graphic() {
    let (d, id) = with_graphic();
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let s = Selection::items(vec![id]);
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("selection");
    let fit = |axis: &str| vec![Action::Exec("object.fit".into(), json!({"mode": "fitFrameToContent", "axis": axis}))];
    assert_eq!(t.pointer(&cx, &ev(PointerKind::Down, at(&l, 300.0, 200.0)).with_clicks(2)), fit("both"), "a corner");
    assert_eq!(t.pointer(&cx, &ev(PointerKind::Down, at(&l, 300.0, 150.0)).with_clicks(2)), fit("width"), "the right side");
    assert_eq!(t.pointer(&cx, &ev(PointerKind::Down, at(&l, 200.0, 200.0)).with_clicks(2)), fit("height"), "the bottom");
    // Nothing counted: the double click event does it.
    t.pointer(&cx, &ev(PointerKind::Down, at(&l, 300.0, 150.0)));
    t.pointer(&cx, &ev(PointerKind::Up, at(&l, 300.0, 150.0)));
    assert_eq!(t.pointer(&cx, &ev(PointerKind::DoubleClick, at(&l, 300.0, 150.0))), fit("width"));
    // A frame without a graphic: an ordinary press on its handle.
    let mut d2 = Document::new(&NewDocument::default());
    let plain = rect(&mut d2, Rect::new(100.0, 100.0, 300.0, 200.0));
    let (s2, l2) = (Selection::items(vec![plain]), CanvasLayout::new(&d2, false));
    let cx2 = ctx(&d2, &s2, &c, &l2);
    assert_eq!(t.pointer(&cx2, &ev(PointerKind::Down, at(&l2, 300.0, 200.0)).with_clicks(2)), vec![Action::Begin("Resize".into())]);
}

#[test]
fn a_click_on_one_of_several_selected_makes_it_the_key_object() {
    let mut d = Document::new(&NewDocument::default());
    let a = rect(&mut d, Rect::new(10.0, 10.0, 50.0, 50.0));
    let b = rect(&mut d, Rect::new(100.0, 100.0, 150.0, 150.0));
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let s = Selection::items(vec![a, b]);
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("selection");
    let p = at(&l, 120.0, 120.0);
    assert!(t.pointer(&cx, &ev(PointerKind::Down, p)).is_empty());
    assert_eq!(t.pointer(&cx, &ev(PointerKind::Up, p)), vec![Action::Exec("select.set".into(), json!({"ids": [a.0, b.0], "key": b.0}))]);
    // Again on the key object: no key object.
    let s = Selection { key: Some(b), ..Selection::items(vec![a, b]) };
    let cx = ctx(&d, &s, &c, &l);
    t.pointer(&cx, &ev(PointerKind::Down, p));
    assert_eq!(t.pointer(&cx, &ev(PointerKind::Up, p)), vec![Action::Exec("select.set".into(), json!({"ids": [a.0, b.0], "key": null}))]);
    // A drag still moves them all, and makes no key object.
    t.pointer(&cx, &ev(PointerKind::Down, p));
    let moved = t.pointer(&cx, &ev(PointerKind::Drag, p + Vec2::new(20.0, 0.0)));
    assert_eq!(moved.first(), Some(&Action::Begin("Move".into())));
    assert_eq!(t.pointer(&cx, &ev(PointerKind::Up, p + Vec2::new(20.0, 0.0))), vec![Action::Commit]);
}

#[test]
fn the_eyedropper_samples_an_image_and_picks_up_text() {
    let (mut d, img) = with_graphic();
    let lid = d.default_layer();
    let (text, _) =
        d.add_text_frame(SpreadRef::Doc(0), Rect::new(100.0, 300.0, 300.0, 400.0), lid, "Source", kition_layout_doc::ParaFormat::default()).unwrap();
    let plain = rect(&mut d, Rect::new(400.0, 100.0, 450.0, 150.0));
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let mut t = create("eyedropper");
    // Objects selected: a placed image gives its colour at the point (Shift: to the stroke).
    let s = Selection::items(vec![plain]);
    let cx = ctx(&d, &s, &c, &l);
    let a = t.pointer(&cx, &ev(PointerKind::Down, at(&l, 120.0, 120.0)));
    assert_eq!(a, vec![Action::Exec("object.matchAttributes".into(), json!({"from": img.0, "point": [120.0, 120.0]}))]);
    let shift = Mods { shift: true, ..Mods::default() };
    let a = t.pointer(&cx, &ev(PointerKind::Down, at(&l, 120.0, 120.0)).with_mods(shift));
    assert!(matches!(&a[..], [Action::Exec(_, p)] if p["target"] == "stroke"), "{a:?}");
    // An object without a picture: its attributes, as before.
    let other = rect(&mut d, Rect::new(500.0, 100.0, 550.0, 150.0));
    let l = CanvasLayout::new(&d, false);
    let cx = ctx(&d, &s, &c, &l);
    assert_eq!(
        t.pointer(&cx, &ev(PointerKind::Down, at(&l, 520.0, 120.0))),
        vec![Action::Exec("object.matchAttributes".into(), json!({"from": other.0}))]
    );
    // Text selected: text gives its formatting; a plain object gives nothing.
    let s = Selection { text: Some(kition_layout_doc::TextSel::caret(kition_layout_doc::StoryId(1), 0)), ..Selection::default() };
    let cx = ctx(&d, &s, &c, &l);
    let a = t.pointer(&cx, &ev(PointerKind::Down, at(&l, 120.0, 310.0)));
    assert_eq!(a, vec![Action::Exec("object.matchAttributes".into(), json!({"from": text.0, "point": [120.0, 310.0]}))]);
    assert!(t.pointer(&cx, &ev(PointerKind::Down, at(&l, 520.0, 120.0))).is_empty());
}
