//! Images and objects by hand (plan 2026-11-25, stage 6): a placed graphic moved and sized inside
//! its frame and measured there, the frame fitted to it along one side, the key object for Align,
//! the Eyedropper's colour and text formatting, several graphics in the place cursor, and several
//! commands as one undo step.

use kition_layout_doc::{Content, ItemId};
use kition_layout_geom::Rect;
use serde_json::{Value, json};

use super::*;

/// A PNG `w` × `h` pixels: red on the left half, blue on the right.
fn png(w: u32, h: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..h {
        for x in 0..w {
            pixels.extend_from_slice(if x < w / 2 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    kition_layout_render::Rendered { width: w, height: h, pixels }.to_png()
}

fn b64(bytes: &[u8]) -> String {
    cmd::base64_encode(bytes)
}

/// A session with a 200 × 100 pt image placed at (100, 100) at its own size: (session, frame).
fn with_image() -> (Session, u64) {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 1})).unwrap();
    let r = s.execute("file.place", &json!({"base64": b64(&png(200, 100)), "name": "pic.png", "x": 100, "y": 100, "width": 200})).unwrap();
    (s, r["id"].as_u64().unwrap())
}

/// The graphic's box on the spread and the frame's.
fn boxes(s: &Session, id: u64) -> (Rect, Rect) {
    let d = &s.doc().unwrap().doc;
    let it = d.item(ItemId(id)).unwrap();
    let Content::Graphic(g) = &it.content else { panic!("a graphic frame") };
    ((it.xf * g.xf).transform_rect_bbox(Rect::new(0.0, 0.0, g.size.0, g.size.1)), it.bounds())
}

fn close(a: Rect, b: [f64; 4]) -> bool {
    [a.x0, a.y0, a.x1, a.y1].iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

fn steps(s: &Session) -> usize {
    s.doc().unwrap().undo_steps()
}

#[test]
fn fitting_a_placed_image_changes_its_scale_once_the_frame_differs() {
    // The walkthrough's "Fit leaves the scale at 25.2 %": a graphic placed without a frame gets a
    // frame its own size, so every fitting has nothing to change. A frame of another shape does.
    let (mut s, id) = with_image();
    let (g0, f0) = boxes(&s, id);
    assert!(close(g0, [f0.x0, f0.y0, f0.x1, f0.y1]), "the new frame is the graphic's size: {g0:?} {f0:?}");
    for mode in ["fillProportionally", "fitProportionally", "fitContentToFrame", "centerContent", "fitFrameToContent"] {
        s.execute("object.fit", &json!({"mode": mode, "ids": [id]})).unwrap();
        assert!(close(boxes(&s, id).0, [g0.x0, g0.y0, g0.x1, g0.y1]), "{mode} on a frame the graphic's size changes nothing");
    }
    // The frame made square (the graphic keeps its size): now the fittings scale it.
    s.execute("transform.resize", &json!({"ids": [id], "from": [100, 100, 300, 200], "to": [100, 100, 300, 300]})).unwrap();
    s.execute("object.fit", &json!({"mode": "fillProportionally", "ids": [id]})).unwrap();
    assert!(close(boxes(&s, id).0, [0.0, 100.0, 400.0, 300.0]), "fills the square, centred: {:?}", boxes(&s, id).0);
    s.execute("object.fit", &json!({"mode": "fitProportionally", "ids": [id]})).unwrap();
    assert!(close(boxes(&s, id).0, [100.0, 150.0, 300.0, 250.0]), "fits inside: {:?}", boxes(&s, id).0);
    s.execute("object.fit", &json!({"mode": "fitContentToFrame", "ids": [id]})).unwrap();
    assert!(close(boxes(&s, id).0, [100.0, 100.0, 300.0, 300.0]), "stretched to the frame");
}

#[test]
fn the_graphic_moves_and_sizes_inside_its_frame() {
    let (mut s, id) = with_image();
    let before = steps(&s);
    s.execute("transform.move", &json!({"ids": [id], "dx": 30, "dy": -10, "content": true})).unwrap();
    let (g, f) = boxes(&s, id);
    assert!(close(g, [130.0, 90.0, 330.0, 190.0]), "{g:?}");
    assert!(close(f, [100.0, 100.0, 300.0, 200.0]), "the frame stays: {f:?}");
    assert_eq!(steps(&s), before + 1);
    // Its own box, on the spread.
    s.execute("transform.set", &json!({"ids": [id], "content": true, "bounds": [100, 100, 500, 300]})).unwrap();
    assert!(close(boxes(&s, id).0, [100.0, 100.0, 500.0, 300.0]));
    assert!(close(boxes(&s, id).1, [100.0, 100.0, 300.0, 200.0]), "the frame stays");
    // Read back and set as the Properties panel does: 200 % across and down.
    let info = s.execute("transform.info", &json!({"ids": [id], "content": true})).unwrap();
    assert_eq!((info["scaleX"].as_f64(), info["scaleY"].as_f64(), info["width"].as_f64()), (Some(200.0), Some(200.0), Some(400.0)), "{info}");
    s.execute("transform.set", &json!({"ids": [id], "content": true, "scaleX": 50, "scaleY": 50})).unwrap();
    let info = s.execute("transform.info", &json!({"ids": [id], "content": true})).unwrap();
    assert_eq!(
        (info["scaleX"].as_f64(), info["width"].as_f64(), info["x"].as_f64(), info["y"].as_f64()),
        (Some(50.0), Some(100.0), Some(100.0), Some(100.0)),
        "about the top left: {info}"
    );
    s.execute("transform.set", &json!({"ids": [id], "content": true, "x": 120, "y": 90})).unwrap();
    assert!(close(boxes(&s, id).0, [120.0, 90.0, 220.0, 140.0]), "{:?}", boxes(&s, id).0);
    // Turning a graphic on its own is refused, and nonsense too; nothing panics.
    assert!(s.execute("transform.set", &json!({"ids": [id], "content": true, "rotation": 10})).is_err());
    assert!(s.execute("transform.set", &json!({"ids": [id], "content": true, "bounds": [1, 1, 1, 1]})).is_err());
    let text = s.execute("frame.add", &json!({"rect": [10, 10, 50, 50], "content": "text", "text": "x"})).unwrap()["id"].as_u64().unwrap();
    assert!(s.execute("transform.info", &json!({"ids": [text], "content": true})).is_err(), "a text frame holds no graphic");
    s.execute("transform.move", &json!({"ids": [text], "dx": 5, "dy": 5, "content": true})).unwrap();
}

#[test]
fn fit_frame_to_content_along_one_side() {
    let (mut s, id) = with_image();
    s.execute("transform.set", &json!({"ids": [id], "content": true, "bounds": [80, 60, 380, 210]})).unwrap();
    s.execute("object.fit", &json!({"ids": [id], "mode": "fitFrameToContent", "axis": "width"})).unwrap();
    assert!(close(boxes(&s, id).1, [80.0, 100.0, 380.0, 200.0]), "across only: {:?}", boxes(&s, id).1);
    s.execute("object.fit", &json!({"ids": [id], "mode": "fitFrameToContent", "axis": "height"})).unwrap();
    assert!(close(boxes(&s, id).1, [80.0, 60.0, 380.0, 210.0]), "then down: {:?}", boxes(&s, id).1);
    assert!(s.execute("object.fit", &json!({"ids": [id], "mode": "fitFrameToContent", "axis": "diagonal"})).is_err());
}

#[test]
fn the_key_object_is_one_of_the_selection() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 1})).unwrap();
    let a = s.execute("frame.add", &json!({"rect": [10, 10, 50, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("frame.add", &json!({"rect": [100, 100, 150, 150]})).unwrap()["id"].as_u64().unwrap();
    let key = |s: &Session| s.doc().unwrap().selection.key.map(|k| k.0);
    s.execute("select.set", &json!({"ids": [a, b], "key": b})).unwrap();
    assert_eq!(key(&s), Some(b));
    s.execute("select.set", &json!({"ids": [a, b]})).unwrap();
    assert_eq!(key(&s), Some(b), "the same selection keeps it");
    s.execute("object.align", &json!({"edge": "left", "to": "keyObject"})).unwrap();
    assert!((s.doc().unwrap().doc.item(ItemId(a)).unwrap().bounds().x0 - 100.0).abs() < 1e-6, "aligned to the key object");
    s.execute("select.set", &json!({"ids": [a, b], "key": null})).unwrap();
    assert_eq!(key(&s), None);
    s.execute("select.set", &json!({"ids": [a], "key": b})).unwrap();
    assert_eq!(key(&s), None, "only a selected object");
    assert!(s.execute("select.set", &json!({"ids": [a], "key": "b"})).is_err());
}

#[test]
fn the_eyedropper_samples_an_image_and_picks_up_text_formatting() {
    let (mut s, img) = with_image();
    let shape = s.execute("frame.add", &json!({"rect": [400, 400, 450, 450], "content": "unassigned"})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [shape]})).unwrap();
    // The left half is red, the right blue.
    let r = s.execute("object.matchAttributes", &json!({"from": img, "point": [120, 150]})).unwrap();
    assert_eq!(r["color"], "#ff0000", "{r}");
    let fill = |s: &Session| s.doc().unwrap().doc.item(ItemId(shape)).unwrap().fill.swatch.clone();
    assert_eq!(fill(&s), r["swatch"].as_str().unwrap());
    let r = s.execute("object.matchAttributes", &json!({"from": img, "point": [280, 150], "target": "stroke"})).unwrap();
    assert_eq!(r["color"], "#0000ff");
    assert_eq!(s.doc().unwrap().doc.item(ItemId(shape)).unwrap().stroke.swatch, r["swatch"].as_str().unwrap());
    // Outside the graphic: the object's attributes, as without a point.
    s.execute("object.matchAttributes", &json!({"from": img, "point": [10, 10]})).unwrap();
    // Text: the formatting under the point goes to the selected text.
    let src = s.execute("frame.add", &json!({"rect": [36, 400, 300, 450], "content": "text", "text": "Source"})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [src]})).unwrap();
    s.execute("text.char", &json!({"attrs": {"size": 30, "underline": true}})).unwrap();
    s.execute("text.para", &json!({"attrs": {"align": "center"}})).unwrap();
    let dst = s.execute("frame.add", &json!({"rect": [36, 500, 300, 560], "content": "text", "text": "Target"})).unwrap();
    let story = dst["story"].as_u64().unwrap();
    s.execute("text.select", &json!({"story": story, "anchor": 0, "focus": 6})).unwrap();
    s.execute("object.matchAttributes", &json!({"from": src, "point": [40, 420]})).unwrap();
    let a = s.execute("text.selectionAttrs", &json!({})).unwrap();
    assert_eq!(
        (a["chars"]["size"].as_f64(), a["chars"]["underline"].as_bool(), a["para"]["align"].as_str()),
        (Some(30.0), Some(true), Some("center")),
        "{a}"
    );
    // With text selected, a click on a graphic colours the text.
    s.execute("text.select", &json!({"story": story, "anchor": 0, "focus": 6})).unwrap();
    let r = s.execute("object.matchAttributes", &json!({"from": img, "point": [280, 150]})).unwrap();
    let a = s.execute("text.selectionAttrs", &json!({})).unwrap();
    assert_eq!(a["chars"]["fill"], r["swatch"], "{a}");
    assert!(s.execute("object.matchAttributes", &json!({"from": img, "point": [280, 150], "target": "both"})).is_err());
}

#[test]
fn several_graphics_wait_in_the_place_cursor() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 1})).unwrap();
    let first = s.execute("place.load", &json!({"base64": b64(&png(10, 10)), "name": "one.png"})).unwrap();
    let second = s.execute("place.load", &json!({"base64": b64(&png(20, 10)), "name": "two.png", "queue": true})).unwrap();
    let third = s.execute("place.load", &json!({"base64": b64(&png(30, 10)), "name": "three.png", "queue": true})).unwrap();
    assert_eq!(third["loaded"], 3);
    let front = |s: &Session| s.place_queue().first().map(|q| q.0.0);
    assert_eq!(front(&s), first["asset"].as_u64());
    assert_eq!(s.place_cycle(true), 3);
    assert_eq!(front(&s), second["asset"].as_u64(), "the arrow brings the next");
    s.place_cycle(false);
    assert_eq!(front(&s), first["asset"].as_u64(), "and back");
    // A click places the front one; the next waits, the cursor stays loaded.
    s.execute("place.drop", &json!({"x": 50, "y": 50})).unwrap();
    assert_eq!((front(&s), s.tool_id()), (second["asset"].as_u64(), "placeGun"));
    // Escape drops the front one; the last drop gives the Selection tool back.
    assert_eq!(s.place_skip(), 1);
    assert_eq!(front(&s), third["asset"].as_u64());
    s.execute("place.drop", &json!({"x": 150, "y": 50})).unwrap();
    assert_eq!((s.place_queue().len(), s.tool_id()), (0, "selection"));
    assert_eq!(s.place_skip(), 0, "nothing left: nothing happens");
    // A load without `queue` replaces the cursor's load.
    s.execute("place.load", &json!({"base64": b64(&png(10, 10)), "name": "a.png"})).unwrap();
    s.execute("place.load", &json!({"base64": b64(&png(10, 10)), "name": "b.png"})).unwrap();
    assert_eq!(s.place_queue().len(), 1);
}

#[test]
fn several_commands_are_one_undo_step() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 1})).unwrap();
    let id = s.execute("frame.add", &json!({"rect": [10, 10, 50, 50], "content": "unassigned"})).unwrap()["id"].as_u64().unwrap();
    s.execute("object.fill", &json!({"ids": [id], "swatch": "[Black]"})).unwrap();
    s.execute("object.stroke", &json!({"ids": [id], "swatch": "[None]"})).unwrap();
    let before = steps(&s);
    let swap: Vec<(String, Value)> =
        vec![("object.fill".into(), json!({"ids": [id], "swatch": "[None]"})), ("object.stroke".into(), json!({"ids": [id], "swatch": "[Black]"}))];
    s.execute_steps("Swap Fill and Stroke", &swap).unwrap();
    assert_eq!(steps(&s), before + 1);
    let it = |s: &Session| s.doc().unwrap().doc.item(ItemId(id)).cloned().unwrap();
    assert_eq!((it(&s).fill.swatch.as_str(), it(&s).stroke.swatch.as_str()), ("[None]", "[Black]"));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!((it(&s).fill.swatch.as_str(), it(&s).stroke.swatch.as_str()), ("[Black]", "[None]"), "one undo takes both back");
    // A failing step leaves the document as it was, with no step.
    let bad: Vec<(String, Value)> = vec![("object.fill".into(), json!({"ids": [id], "swatch": "[None]"})), ("no.such".into(), json!({}))];
    assert!(s.execute_steps("Broken", &bad).is_err());
    assert_eq!(it(&s).fill.swatch, "[Black]");
    assert_eq!(steps(&s), before);
}
