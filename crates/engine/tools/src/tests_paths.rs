//! The Pen on a selected path and Direct Selection's anchor points (plan 2026-11-25, stage 8): the
//! Pen adds an anchor on the outline, deletes the anchor under it, continues an open path from an
//! end point and closes the path it draws on its first anchor, its cursor saying which; Direct
//! Selection selects several anchor points (click, Shift+click, marquee across paths), drags them
//! together, and moves them with the arrows and deletes them with Delete, each one undo step.

use kition_layout_compose::Cache;
use kition_layout_doc::build::NewDocument;
use kition_layout_doc::{Document, Item, ItemId, Selection, Shape, SpreadRef};
use kition_layout_geom::{Anchor, PathData, Point, SubPath, Unit};
use serde_json::json;

use super::*;

fn ctx<'a>(d: &'a Document, s: &'a Selection, c: &'a Cache, l: &'a CanvasLayout) -> ToolContext<'a> {
    ToolContext {
        doc: d,
        selection: s,
        cache: c,
        layout: l,
        zoom: 1.0,
        layer: d.default_layer(),
        snap: SnapView::OFF,
        unit: Unit::Points,
        before: None,
    }
}

/// An open path through `points` (spread coordinates), corner anchors.
fn path(d: &mut Document, points: &[(f64, f64)]) -> ItemId {
    let layer = d.default_layer();
    let id = ItemId(d.alloc());
    let anchors = points.iter().map(|(x, y)| Anchor::corner(Point::new(*x, *y))).collect();
    let item = Item::new(id, layer, Shape::Path, PathData::single(SubPath::new(anchors, false)));
    d.insert_item(SpreadRef::Doc(0), item, None).unwrap();
    id
}

fn at(l: &CanvasLayout, x: f64, y: f64) -> Point {
    let off = l.xf(SpreadRef::Doc(0)).translation();
    Point::new(x + off.x, y + off.y)
}

fn ev(kind: PointerKind, p: Point) -> PointerEvent {
    PointerEvent::new(kind, p.x, p.y)
}

fn shift() -> Mods {
    Mods { shift: true, ..Mods::default() }
}

/// A three-anchor open path (100,100) → (200,100) → (200,200), selected.
fn selected_path() -> (Document, ItemId, Selection) {
    let mut d = Document::new(&NewDocument::default());
    let id = path(&mut d, &[(100.0, 100.0), (200.0, 100.0), (200.0, 200.0)]);
    (d, id, Selection::items(vec![id]))
}

#[test]
fn the_pen_adds_and_deletes_anchors_on_a_selected_path() {
    let (d, id, s) = selected_path();
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("pen");
    // On the outline: a click adds an anchor there.
    let on_segment = at(&l, 150.0, 100.0);
    assert_eq!(t.cursor(&cx, on_segment, Mods::default()), Cursor::PenAdd);
    let added = t.pointer(&cx, &ev(PointerKind::Down, on_segment));
    assert!(
        matches!(added.as_slice(), [Action::Exec(cmd, p)] if cmd == "path.addAnchor" && p["id"] == id.0 && p["at"] == json!([150.0, 100.0])),
        "{added:?}"
    );
    assert!(!t.busy(), "adding an anchor starts no path");
    // On the middle anchor: a click deletes it.
    let middle = at(&l, 200.0, 100.0);
    assert_eq!(t.cursor(&cx, middle, Mods::default()), Cursor::PenDelete);
    let deleted = t.pointer(&cx, &ev(PointerKind::Down, middle));
    assert_eq!(deleted, vec![Action::Exec("path.deleteAnchor".into(), json!({"id": id.0, "subpath": 0, "anchor": 1}))]);
    // Away from the path: an ordinary pen.
    assert_eq!(t.cursor(&cx, at(&l, 400.0, 400.0), Mods::default()), Cursor::Pen);
    // A path that is not selected is not edited: its outline is an ordinary pen.
    let none = Selection::default();
    let cx = ctx(&d, &none, &c, &l);
    assert_eq!(t.cursor(&cx, on_segment, Mods::default()), Cursor::Pen);
}

#[test]
fn the_pen_continues_an_open_path_from_either_end() {
    let (d, id, s) = selected_path();
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("pen");
    // The last anchor: the path goes on from there.
    let end = at(&l, 200.0, 200.0);
    assert_eq!(t.cursor(&cx, end, Mods::default()), Cursor::PenContinue);
    assert!(t.pointer(&cx, &ev(PointerKind::Down, end)).is_empty(), "the path is selected already");
    assert!(t.pointer(&cx, &ev(PointerKind::Up, end)).is_empty(), "a click on the end point adds nothing");
    assert!(t.busy());
    let next = at(&l, 100.0, 300.0);
    t.pointer(&cx, &ev(PointerKind::Down, next));
    let appended = t.pointer(&cx, &ev(PointerKind::Up, next));
    assert!(
        matches!(appended.as_slice(), [Action::Exec(cmd, p)] if cmd == "path.appendAnchor" && p["id"] == id.0 && p["subpath"] == 0 && p.get("at").is_none() && p["anchor"]["p"] == json!([100.0, 300.0])),
        "{appended:?}"
    );
    t.key(&cx, ToolKey::Escape, Mods::default());
    // The first anchor: new anchors go before it, the path keeps its direction.
    let start = at(&l, 100.0, 100.0);
    assert_eq!(t.cursor(&cx, start, Mods::default()), Cursor::PenContinue);
    t.pointer(&cx, &ev(PointerKind::Down, start));
    t.pointer(&cx, &ev(PointerKind::Up, start));
    let before = at(&l, 50.0, 50.0);
    t.pointer(&cx, &ev(PointerKind::Down, before));
    let prepended = t.pointer(&cx, &ev(PointerKind::Up, before));
    assert!(
        matches!(prepended.as_slice(), [Action::Exec(cmd, p)] if cmd == "path.appendAnchor" && p["at"] == "start" && p["subpath"] == 0),
        "{prepended:?}"
    );
    // Dragging from the end point turns its direction lines instead of adding an anchor.
    t.key(&cx, ToolKey::Escape, Mods::default());
    t.pointer(&cx, &ev(PointerKind::Down, end));
    t.pointer(&cx, &ev(PointerKind::Drag, at(&l, 230.0, 220.0)));
    let turned = t.pointer(&cx, &ev(PointerKind::Up, at(&l, 230.0, 220.0)));
    assert_eq!(turned, vec![Action::Exec("path.convertAnchor".into(), json!({"id": id.0, "subpath": 0, "anchor": 2, "to": [230.0, 220.0]}))]);
}

#[test]
fn the_pen_closes_on_the_first_anchor_and_says_so() {
    let d = Document::new(&NewDocument::default());
    let (s, c, l) = (Selection::default(), Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("pen");
    for (x, y) in [(100.0, 100.0), (200.0, 100.0)] {
        t.pointer(&cx, &ev(PointerKind::Down, at(&l, x, y)));
        t.pointer(&cx, &ev(PointerKind::Up, at(&l, x, y)));
    }
    // The path the engine made (selected), as the pen picks it up.
    let mut d2 = d.clone();
    let id = path(&mut d2, &[(100.0, 100.0), (200.0, 100.0)]);
    let s2 = Selection::items(vec![id]);
    let cx = ctx(&d2, &s2, &c, &l);
    t.pointer(&cx, &ev(PointerKind::Down, at(&l, 200.0, 200.0)));
    t.pointer(&cx, &ev(PointerKind::Up, at(&l, 200.0, 200.0)));
    let first = at(&l, 100.0, 100.0);
    assert_eq!(t.cursor(&cx, first, Mods::default()), Cursor::PenClose);
    assert_eq!(t.cursor(&cx, at(&l, 300.0, 300.0), Mods::default()), Cursor::Pen, "while drawing, a click places an anchor");
    let closed = t.pointer(&cx, &ev(PointerKind::Down, first));
    assert_eq!(closed, vec![Action::Exec("path.close".into(), json!({"id": id.0}))]);
    assert!(!t.busy());
}

/// Two open paths, both selected: (100,100)–(200,100) and (100,300)–(200,300).
fn two_paths() -> (Document, ItemId, ItemId) {
    let mut d = Document::new(&NewDocument::default());
    let a = path(&mut d, &[(100.0, 100.0), (200.0, 100.0)]);
    let b = path(&mut d, &[(100.0, 300.0), (200.0, 300.0)]);
    (d, a, b)
}

#[test]
fn direct_selection_marquees_anchor_points_across_paths() {
    let (d, a, b) = two_paths();
    let (s, c, l) = (Selection::default(), Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("directSelection");
    t.pointer(&cx, &ev(PointerKind::Down, at(&l, 150.0, 50.0)));
    t.pointer(&cx, &ev(PointerKind::Drag, at(&l, 250.0, 350.0)));
    let picked = t.pointer(&cx, &ev(PointerKind::Up, at(&l, 250.0, 350.0)));
    assert_eq!(
        picked,
        vec![Action::Exec("select.set".into(), json!({"ids": [a.0, b.0], "anchors": [[a.0, 0, 1], [b.0, 0, 1]], "content": true}))],
        "the right-hand anchor of each path"
    );
}

#[test]
fn direct_selection_clicks_add_points_and_drags_them_together() {
    let (d, a, b) = two_paths();
    let mut s = Selection::items(vec![a, b]);
    s.anchors = vec![(a, 0, 1)];
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("directSelection");
    // Shift+click another path's anchor: it joins the selected points.
    let other = at(&l, 200.0, 300.0);
    let added = t.pointer(&cx, &ev(PointerKind::Down, other).with_mods(shift()));
    assert_eq!(added[0], Action::Exec("select.set".into(), json!({"ids": [a.0, b.0], "anchors": [[a.0, 0, 1], [b.0, 0, 1]], "content": true})));
    t.pointer(&cx, &ev(PointerKind::Up, other).with_mods(shift()));
    // Dragging one selected point moves every selected point: one preview of both paths.
    s.anchors = vec![(a, 0, 1), (b, 0, 1)];
    let cx = ctx(&d, &s, &c, &l);
    let press = at(&l, 200.0, 100.0);
    let begun = t.pointer(&cx, &ev(PointerKind::Down, press));
    assert_eq!(begun, vec![Action::Begin("Move Anchors".into())], "already selected: the selection stays");
    let moved = t.pointer(&cx, &ev(PointerKind::Drag, at(&l, 210.0, 120.0)));
    assert_eq!(
        moved,
        vec![Action::PreviewSteps(vec![
            ("path.moveAnchors".into(), json!({"id": a.0, "anchors": [[0, 1]], "dx": 10.0, "dy": 20.0})),
            ("path.moveAnchors".into(), json!({"id": b.0, "anchors": [[0, 1]], "dx": 10.0, "dy": 20.0})),
        ])]
    );
    assert_eq!(t.pointer(&cx, &ev(PointerKind::Up, at(&l, 210.0, 120.0))), vec![Action::Commit], "one undo step");
    // Shift+click a selected point without dragging takes it away, on release.
    t.pointer(&cx, &ev(PointerKind::Down, press).with_mods(shift()));
    let released = t.pointer(&cx, &ev(PointerKind::Up, press).with_mods(shift()));
    assert_eq!(
        released,
        vec![Action::Commit, Action::Exec("select.set".into(), json!({"ids": [a.0, b.0], "anchors": [[b.0, 0, 1]], "content": true}))]
    );
}

#[test]
fn direct_selection_arrows_move_and_delete_removes_the_selected_points() {
    let mut d = Document::new(&NewDocument::default());
    let a = path(&mut d, &[(100.0, 100.0), (200.0, 100.0), (300.0, 100.0)]);
    let b = path(&mut d, &[(100.0, 300.0), (200.0, 300.0)]);
    let mut s = Selection::items(vec![a, b]);
    s.anchors = vec![(a, 0, 0), (a, 0, 2), (b, 0, 0)];
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("directSelection");
    let right = t.key(&cx, ToolKey::Right, Mods::default());
    assert_eq!(
        right,
        vec![Action::Steps(
            "Move Anchors".into(),
            vec![
                ("path.moveAnchors".into(), json!({"id": a.0, "anchors": [[0, 0], [0, 2]], "dx": 1.0, "dy": 0.0})),
                ("path.moveAnchors".into(), json!({"id": b.0, "anchors": [[0, 0]], "dx": 1.0, "dy": 0.0})),
            ]
        )]
    );
    let down = t.key(&cx, ToolKey::Down, shift());
    assert!(matches!(down.as_slice(), [Action::Steps(_, steps)] if steps[0].1["dy"] == 10.0), "Shift: ten times");
    // Delete: the last anchors first; the two-anchor path loses one and goes whole (no second step for it).
    let deleted = t.key(&cx, ToolKey::Delete, Mods::default());
    assert_eq!(
        deleted,
        vec![Action::Steps(
            "Delete Anchor Points".into(),
            vec![
                ("path.deleteAnchor".into(), json!({"id": a.0, "subpath": 0, "anchor": 2})),
                ("path.deleteAnchor".into(), json!({"id": a.0, "subpath": 0, "anchor": 0})),
                ("path.deleteAnchor".into(), json!({"id": b.0, "subpath": 0, "anchor": 0})),
            ]
        )]
    );
    // One path's points: a single command.
    s.anchors = vec![(a, 0, 1)];
    let cx = ctx(&d, &s, &c, &l);
    assert_eq!(
        t.key(&cx, ToolKey::Left, Mods::default()),
        vec![Action::Exec("path.moveAnchors".into(), json!({"id": a.0, "anchors": [[0, 1]], "dx": -1.0, "dy": 0.0}))]
    );
    // No points selected: the arrows move the objects, as before.
    s.anchors.clear();
    let cx = ctx(&d, &s, &c, &l);
    assert!(matches!(t.key(&cx, ToolKey::Left, Mods::default()).as_slice(), [Action::Exec(cmd, _)] if cmd == "transform.move"));
}

/// Plan 2026-11-25, phase 10: while the Pen drags out a smooth point, the segment from the anchor
/// before shows as it will be drawn (bent by both handles), with the handles.
#[test]
fn the_pen_previews_the_curve_while_a_smooth_point_is_dragged() {
    let d = Document::new(&NewDocument::default());
    let (s, c, l) = (Selection::default(), Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("pen");
    let first = at(&l, 100.0, 100.0);
    t.pointer(&cx, &ev(PointerKind::Down, first));
    t.pointer(&cx, &ev(PointerKind::Up, first));
    t.pointer(&cx, &ev(PointerKind::Down, at(&l, 200.0, 100.0)));
    t.pointer(&cx, &ev(PointerKind::Drag, at(&l, 240.0, 140.0)));
    let overlays = t.overlays(&cx);
    let curve = overlays.iter().find_map(|o| match o {
        Overlay::Path { path, dashed: false, .. } => Some(path.clone()),
        _ => None,
    });
    let curve = curve.expect("a curve segment while dragging");
    let els: Vec<kition_layout_geom::PathEl> = curve.elements().to_vec();
    let (p0, p3) = (at(&l, 100.0, 100.0), at(&l, 200.0, 100.0));
    assert!(matches!(els.first(), Some(kition_layout_geom::PathEl::MoveTo(p)) if (*p - p0).hypot() < 1e-9), "{els:?}");
    // The in handle mirrors the drag: (160, 60).
    let in_handle = at(&l, 160.0, 60.0);
    assert!(
        matches!(els.get(1), Some(kition_layout_geom::PathEl::CurveTo(_, b, e)) if (*b - in_handle).hypot() < 1e-9 && (*e - p3).hypot() < 1e-9),
        "{els:?}"
    );
    // And the two handles, as one line through the anchor.
    assert!(overlays.iter().any(
        |o| matches!(o, Overlay::Line { a, b, dashed: false, .. } if (*a - in_handle).hypot() < 1e-9 && (*b - at(&l, 240.0, 140.0)).hypot() < 1e-9)
    ));
}

/// Plan 2026-11-25, phase 10: Shift keeps a Direct Selection anchor drag at 45° steps.
#[test]
fn direct_selection_shift_constrains_an_anchor_drag_to_45_degrees() {
    let (d, id, mut s) = selected_path();
    s.anchors = vec![(id, 0, 1)];
    let (c, l) = (Cache::new(), CanvasLayout::new(&d, false));
    let cx = ctx(&d, &s, &c, &l);
    let mut t = create("directSelection");
    let press = at(&l, 200.0, 100.0);
    t.pointer(&cx, &ev(PointerKind::Down, press));
    let moved = |out: Vec<Action>| match out.first() {
        Some(Action::Preview(id, p)) if id == "path.moveAnchors" => (p["dx"].as_f64().unwrap(), p["dy"].as_f64().unwrap()),
        other => panic!("{other:?}"),
    };
    // Nearly level: level.
    let (dx, dy) = moved(t.pointer(&cx, &ev(PointerKind::Drag, at(&l, 230.0, 108.0)).with_mods(shift())));
    assert!(dy.abs() < 1e-9 && dx > 30.0, "({dx}, {dy})");
    // Nearly diagonal: diagonal.
    let (dx, dy) = moved(t.pointer(&cx, &ev(PointerKind::Drag, at(&l, 230.0, 127.0)).with_mods(shift())));
    assert!((dx - dy).abs() < 1e-9 && dx > 0.0, "({dx}, {dy})");
    // Without Shift the point follows the pointer.
    let (dx, dy) = moved(t.pointer(&cx, &ev(PointerKind::Drag, at(&l, 230.0, 108.0))));
    assert!((dx - 30.0).abs() < 1e-9 && (dy - 8.0).abs() < 1e-9, "({dx}, {dy})");
}
