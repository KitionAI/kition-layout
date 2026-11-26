//! Paths by hand (plan 2026-11-25, stage 8): the commands the Pen and Direct Selection send on a
//! real session. `select.set` takes Direct Selection's anchor points; `path.appendAnchor` continues
//! a chosen subpath at either end and `path.close` closes one subpath; the selected points follow
//! anchors added and deleted; a drag of points on two paths and the arrows' and Delete's steps are
//! each one undo step.

use kition_layout_doc::{ItemId, SpreadRef};
use kition_layout_geom::{Point, Unit};
use kition_layout_tools::{Mods, PointerEvent, PointerKind, SnapView, ToolKey};
use serde_json::json;

use super::*;

const VIEW: tooling::ViewInfo = tooling::ViewInfo { zoom: 1.0, snap: SnapView::OFF, unit: Unit::Points };

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 1})).unwrap();
    s
}

fn path(s: &mut Session, points: &[[f64; 2]]) -> u64 {
    let anchors: Vec<_> = points.iter().map(|p| json!({"p": p})).collect();
    s.execute("path.add", &json!({"anchors": anchors})).unwrap()["id"].as_u64().unwrap()
}

/// Anchor positions of `id`'s first subpath.
fn points(s: &Session, id: u64) -> Vec<[f64; 2]> {
    let d = &s.doc().unwrap().doc;
    let it = d.item(ItemId(id)).unwrap();
    let loc = d.find(ItemId(id)).unwrap();
    let m = d.parent_xf(&loc) * it.xf;
    it.path.subpaths[0].anchors.iter().map(|a| m * a.p).map(|p| [p.x.round(), p.y.round()]).collect()
}

fn steps(s: &Session) -> usize {
    s.doc().unwrap().undo_steps()
}

fn canvas(s: &Session, x: f64, y: f64) -> Point {
    let off = s.layout().xf(SpreadRef::Doc(0)).translation();
    Point::new(x + off.x, y + off.y)
}

#[test]
fn select_set_takes_anchor_points_of_selected_paths() {
    let mut s = session();
    let a = path(&mut s, &[[100.0, 100.0], [200.0, 100.0], [300.0, 100.0]]);
    let b = path(&mut s, &[[100.0, 300.0], [200.0, 300.0]]);
    s.execute("select.set", &json!({"ids": [a, b], "anchors": [[a, 0, 2], [b, 0, 0], [a, 0, 9], [a, 0, 2]]})).unwrap();
    let anchors = |s: &Session| s.doc().unwrap().selection.anchors.iter().map(|(i, si, ai)| (i.0, *si, *ai)).collect::<Vec<_>>();
    assert_eq!(anchors(&s), vec![(a, 0, 2), (b, 0, 0)], "real anchors, each once");
    s.execute("select.set", &json!({"ids": [a], "anchors": [[a, 0, 0]], "add": true})).unwrap();
    assert_eq!(anchors(&s), vec![(a, 0, 2), (b, 0, 0), (a, 0, 0)]);
    // Adding an anchor before a selected one moves its index up; deleting one takes it away.
    s.execute("path.addAnchor", &json!({"id": a, "at": [150, 100]})).unwrap();
    assert_eq!(anchors(&s), vec![(a, 0, 3), (b, 0, 0), (a, 0, 0)]);
    s.execute("path.deleteAnchor", &json!({"id": a, "subpath": 0, "anchor": 0})).unwrap();
    assert_eq!(anchors(&s), vec![(a, 0, 2), (b, 0, 0)]);
    // Selecting objects again drops the points; an anchor of an unselected path is not taken.
    s.execute("select.set", &json!({"ids": [a], "anchors": [[b, 0, 1]]})).unwrap();
    assert!(anchors(&s).is_empty());
    assert!(s.execute("select.set", &json!({"ids": [a], "anchors": [[a, 0]]})).is_err(), "[id, subpath, anchor]");
}

#[test]
fn append_anchor_continues_either_end_of_a_chosen_subpath() {
    let mut s = session();
    let a = path(&mut s, &[[100.0, 100.0], [200.0, 100.0]]);
    s.execute("path.appendAnchor", &json!({"id": a, "subpath": 0, "anchor": {"p": [300, 100]}})).unwrap();
    s.execute("path.appendAnchor", &json!({"id": a, "subpath": 0, "at": "start", "anchor": {"p": [50, 50]}})).unwrap();
    assert_eq!(points(&s, a), vec![[50.0, 50.0], [100.0, 100.0], [200.0, 100.0], [300.0, 100.0]]);
    assert!(s.execute("path.appendAnchor", &json!({"id": a, "subpath": 3, "anchor": {"p": [0, 0]}})).is_err(), "no such subpath");
    assert!(s.execute("path.appendAnchor", &json!({"id": a, "at": "middle", "anchor": {"p": [0, 0]}})).is_err());
    s.execute("path.close", &json!({"id": a, "subpath": 0})).unwrap();
    assert!(s.doc().unwrap().doc.item(ItemId(a)).unwrap().path.subpaths[0].closed);
    assert!(s.execute("path.close", &json!({"id": a, "subpath": 2})).is_err());
}

#[test]
fn the_pen_continues_a_selected_open_path_on_a_session() {
    let mut s = session();
    let a = path(&mut s, &[[100.0, 100.0], [200.0, 100.0]]);
    s.set_tool("pen");
    let click = |s: &mut Session, x: f64, y: f64| {
        let p = canvas(s, x, y);
        s.pointer(&PointerEvent::new(PointerKind::Down, p.x, p.y), VIEW).unwrap();
        s.pointer(&PointerEvent::new(PointerKind::Up, p.x, p.y), VIEW).unwrap();
    };
    let before = steps(&s);
    click(&mut s, 200.0, 100.0);
    click(&mut s, 200.0, 200.0);
    assert_eq!(points(&s, a), vec![[100.0, 100.0], [200.0, 100.0], [200.0, 200.0]], "one path, longer");
    assert_eq!(steps(&s), before + 1, "one step for the anchor");
    click(&mut s, 100.0, 100.0);
    assert!(s.doc().unwrap().doc.item(ItemId(a)).unwrap().path.subpaths[0].closed, "the first anchor closes it");
    // On the closed, selected path: a click on its outline adds an anchor, one on an anchor deletes it.
    click(&mut s, 150.0, 100.0);
    assert_eq!(points(&s, a).len(), 4);
    click(&mut s, 150.0, 100.0);
    assert_eq!(points(&s, a), vec![[100.0, 100.0], [200.0, 100.0], [200.0, 200.0]]);
}

#[test]
fn direct_selection_points_move_and_go_as_one_step() {
    let mut s = session();
    let a = path(&mut s, &[[100.0, 100.0], [200.0, 100.0], [300.0, 100.0]]);
    let b = path(&mut s, &[[100.0, 300.0], [200.0, 300.0], [300.0, 300.0]]);
    s.execute("select.set", &json!({"ids": [], "content": true})).unwrap();
    s.set_tool("directSelection");
    // A marquee around the middle anchors of both paths.
    let (from, to) = (canvas(&s, 150.0, 50.0), canvas(&s, 250.0, 350.0));
    s.pointer(&PointerEvent::new(PointerKind::Down, from.x, from.y), VIEW).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Drag, to.x, to.y), VIEW).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, to.x, to.y), VIEW).unwrap();
    assert_eq!(s.doc().unwrap().selection.anchors.len(), 2);
    // Drag one of them: both move, one undo step.
    let before = steps(&s);
    let (p0, p1) = (canvas(&s, 200.0, 100.0), canvas(&s, 210.0, 120.0));
    s.pointer(&PointerEvent::new(PointerKind::Down, p0.x, p0.y), VIEW).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Drag, p1.x, p1.y), VIEW).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, p1.x, p1.y), VIEW).unwrap();
    assert_eq!(points(&s, a)[1], [210.0, 120.0]);
    assert_eq!(points(&s, b)[1], [210.0, 320.0]);
    assert_eq!(steps(&s), before + 1);
    // The arrows: both points, one step each press.
    s.tool_key(ToolKey::Right, Mods::default(), VIEW).unwrap();
    assert_eq!((points(&s, a)[1], points(&s, b)[1]), ([211.0, 120.0], [211.0, 320.0]));
    assert_eq!(steps(&s), before + 2);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!((points(&s, a)[1], points(&s, b)[1]), ([210.0, 120.0], [210.0, 320.0]), "one undo takes both back");
    // Delete: both points go, the paths stay (each keeps two), one step.
    let before = steps(&s);
    s.tool_key(ToolKey::Delete, Mods::default(), VIEW).unwrap();
    assert_eq!(points(&s, a), vec![[100.0, 100.0], [300.0, 100.0]]);
    assert_eq!(points(&s, b), vec![[100.0, 300.0], [300.0, 300.0]]);
    assert_eq!(steps(&s), before + 1);
    assert!(s.doc().unwrap().selection.anchors.is_empty());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(points(&s, a).len(), 3, "one undo brings both back");
    assert_eq!(points(&s, b).len(), 3);
}
