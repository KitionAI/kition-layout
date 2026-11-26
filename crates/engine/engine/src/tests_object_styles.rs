//! Object styles by hand (plan 2026-11-25, phase 11): Object Style Options change a style and the
//! objects using it follow where they do not override it, Redefine Style takes the selected
//! object's look, a style based on another takes what it leaves out from it, renaming follows
//! every use, and a deleted style hands its objects to another, each one undo step.

use serde_json::json;

use super::*;
use kition_layout_doc::ItemId;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 1})).unwrap();
    s.execute("swatch.add", &json!({"name": "Ink", "color": "#108080"})).unwrap();
    s.execute("swatch.add", &json!({"name": "Rust", "color": "#a04020"})).unwrap();
    s
}

fn rect(s: &mut Session, x: f64) -> u64 {
    s.execute("frame.add", &json!({"rect": [x, 36.0, x + 80.0, 116.0], "content": "graphic"})).unwrap()["id"].as_u64().unwrap()
}

fn item(s: &mut Session, id: u64) -> kition_layout_doc::Item {
    s.doc().unwrap().doc.item(ItemId(id)).unwrap().clone()
}

#[test]
fn style_options_reach_the_objects_that_do_not_override_them() {
    let mut s = session();
    s.execute("style.object.add", &json!({"name": "Box", "fromSelection": false, "fill": "Ink"})).unwrap();
    let (a, b) = (rect(&mut s, 36.0), rect(&mut s, 160.0));
    s.execute("style.object.apply", &json!({"name": "Box", "ids": [a, b]})).unwrap();
    // B overrides the fill.
    s.execute("object.fill", &json!({"ids": [b], "swatch": "Rust"})).unwrap();
    let steps = s.doc().unwrap().undo_steps();
    s.execute("style.object.edit", &json!({"name": "Box", "fill": "[Black]", "fillTint": 0.5, "stroke": "Rust", "strokeWeight": 3})).unwrap();
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1, "one undo step");
    let (ia, ib) = (item(&mut s, a), item(&mut s, b));
    assert_eq!((ia.fill.swatch.as_str(), ia.fill.tint), ("[Black]", 0.5));
    assert_eq!((ia.stroke.swatch.as_str(), ia.stroke.weight), ("Rust", 3.0));
    assert_eq!(ib.fill.swatch, "Rust", "an override stays");
    assert_eq!((ib.stroke.swatch.as_str(), ib.stroke.weight), ("Rust", 3.0), "the part it does not override follows");
    // Leaving the fill out of the style leaves the objects' fills as they are.
    s.execute("style.object.edit", &json!({"name": "Box", "fill": null})).unwrap();
    assert!(s.doc().unwrap().doc.styles.object_style("Box").unwrap().fill.is_none());
    assert_eq!(item(&mut s, a).fill.swatch, "[Black]");
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(item(&mut s, a).fill.swatch, "Ink");
    // Refusals change nothing.
    assert!(s.execute("style.object.edit", &json!({"name": "[None]", "fill": "Ink"})).is_err());
    assert!(s.execute("style.object.edit", &json!({"name": "Box", "fill": "Nope"})).is_err());
    assert!(s.execute("style.object.edit", &json!({"name": "Box", "fillTint": 2})).is_err());
    assert!(s.execute("style.object.edit", &json!({"name": "[Basic Graphics Frame]", "rename": "Plain"})).is_err());
}

#[test]
fn based_on_redefine_rename_and_text_frame_options() {
    let mut s = session();
    s.execute("style.object.add", &json!({"name": "Base", "fromSelection": false, "fill": "Ink"})).unwrap();
    s.execute("style.object.add", &json!({"name": "Child", "fromSelection": false})).unwrap();
    s.execute("style.object.edit", &json!({"name": "Child", "basedOn": "Base", "fill": null})).unwrap();
    let a = rect(&mut s, 36.0);
    s.execute("style.object.apply", &json!({"name": "Child", "ids": [a]})).unwrap();
    assert_eq!(item(&mut s, a).fill.swatch, "Ink", "Child takes the fill from Base");
    s.execute("style.object.edit", &json!({"name": "Base", "fill": "Rust"})).unwrap();
    assert_eq!(item(&mut s, a).fill.swatch, "Rust", "an edit of Base reaches Child's objects");
    assert!(s.execute("style.object.edit", &json!({"name": "Base", "basedOn": "Child"})).is_err(), "no loop");

    // Redefine Style: Child takes the selected object's look.
    s.execute("object.fill", &json!({"ids": [a], "swatch": "[Paper]"})).unwrap();
    s.execute("select.set", &json!({"ids": [a]})).unwrap();
    s.execute("style.object.edit", &json!({"name": "Child", "fromSelection": true})).unwrap();
    assert_eq!(s.doc().unwrap().doc.styles.object_style("Child").unwrap().fill.as_ref().unwrap().swatch, "[Paper]");

    // A rename follows every use, the style based on it and the default frame styles.
    s.execute("style.object.edit", &json!({"name": "Base", "rename": "Root"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.styles.object_style("Child").unwrap().based_on.as_deref(), Some("Root"));
    assert!(s.execute("style.object.edit", &json!({"name": "Root", "rename": "Child"})).is_err());

    // Text frame options and the paragraph style reach a text frame using the style.
    s.execute("style.paragraph.add", &json!({"name": "Body"})).unwrap();
    let t = s.execute("frame.add", &json!({"rect": [36, 200, 300, 400], "content": "text", "text": "One\rTwo"})).unwrap();
    let (frame, story) = (t["id"].as_u64().unwrap(), t["story"].as_u64().unwrap());
    s.execute("style.object.add", &json!({"name": "Columns", "fromSelection": false})).unwrap();
    s.execute("style.object.apply", &json!({"name": "Columns", "ids": [frame]})).unwrap();
    s.execute(
        "style.object.edit",
        &json!({"name": "Columns", "textFrame": {"columns": 2, "gutter": 18, "inset": [6, 6, 6, 6]}, "paragraphStyle": "Body"}),
    )
    .unwrap();
    let options = item(&mut s, frame).text_frame().unwrap().options.clone();
    assert_eq!((options.columns, options.gutter, options.inset), (2, 18.0, [6.0; 4]));
    let d = s.doc().unwrap().doc.clone();
    assert!(d.story(kition_layout_doc::StoryId(story)).unwrap().paras.iter().all(|p| p.style == "Body"));
    assert!(s.execute("style.object.edit", &json!({"name": "Columns", "textFrame": {"columns": 0}})).is_err());
    s.doc().unwrap().doc.check().unwrap();
}

#[test]
fn a_deleted_style_hands_its_objects_to_the_replacement() {
    let mut s = session();
    s.execute("style.object.add", &json!({"name": "Box", "fromSelection": false, "fill": "Ink"})).unwrap();
    s.execute("style.object.add", &json!({"name": "Other", "fromSelection": false, "fill": "Rust"})).unwrap();
    s.execute("style.object.add", &json!({"name": "Kid", "fromSelection": false})).unwrap();
    s.execute("style.object.edit", &json!({"name": "Kid", "basedOn": "Box"})).unwrap();
    let a = rect(&mut s, 36.0);
    s.execute("style.object.apply", &json!({"name": "Box", "ids": [a]})).unwrap();
    let steps = s.doc().unwrap().undo_steps();
    let r = s.execute("style.object.delete", &json!({"name": "Box", "replaceWith": "Other"})).unwrap();
    assert_eq!(r["objects"], 1);
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);
    let it = item(&mut s, a);
    assert_eq!((it.object_style.as_str(), it.fill.swatch.as_str()), ("Other", "Ink"), "the object keeps its look");
    assert_eq!(s.doc().unwrap().doc.styles.object_style("Kid").unwrap().based_on.as_deref(), Some("Other"));
    assert!(s.doc().unwrap().doc.styles.object_style("Box").is_none());
    // Without a replacement: [None]. Built-in styles stay.
    s.execute("style.object.delete", &json!({"name": "Other"})).unwrap();
    assert_eq!(item(&mut s, a).object_style, "[None]");
    assert!(s.execute("style.object.delete", &json!({"name": "[Basic Text Frame]"})).is_err());
    assert!(s.execute("style.object.delete", &json!({"name": "Kid", "replaceWith": "Kid"})).is_err());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(item(&mut s, a).object_style, "Other");
}
