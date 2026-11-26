//! Parent pages by hand (plan 2026-11-25, phase 11): Parent Page Options (prefix, name, based-on
//! parent, number of pages), Duplicate Parent Spread and Delete Parent Spread, each one undo step;
//! a page shows the objects of its parent and of the parents that one is based on.

use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 3, "facingPages": true})).unwrap();
    s
}

fn parents(s: &mut Session) -> Vec<Value> {
    s.execute("file.inspect", &json!({})).unwrap()["parents"].as_array().unwrap().clone()
}

fn parent(s: &mut Session, prefix: &str) -> Value {
    parents(s).into_iter().find(|p| p["prefix"] == prefix).unwrap_or_else(|| panic!("no parent {prefix}"))
}

/// A black square on parent `prefix`'s right page at (`x`, `y`) page-relative: its id.
fn square_on(s: &mut Session, prefix: &str, x: f64, y: f64) -> u64 {
    let pi = s.doc().unwrap().doc.parents.iter().position(|p| p.parent.as_ref().is_some_and(|i| i.prefix == prefix)).unwrap();
    let page = s.doc().unwrap().doc.parents[pi].pages.last().unwrap().x;
    let r = s.execute("frame.add", &json!({"spread": prefix, "rect": [page + x, y, page + x + 40.0, y + 40.0], "content": "graphic"})).unwrap();
    let id = r["id"].as_u64().unwrap();
    s.execute("object.fill", &json!({"ids": [id], "swatch": "[Black]"})).unwrap();
    id
}

fn dark_at(s: &mut Session, page: usize, x: u32, y: u32) -> bool {
    let d = s.doc().unwrap().doc.clone();
    let mut rr = kition_layout_render::Renderer::new();
    rr.threads = 0;
    let img = rr.render_page(&d, &s.cache, page, 1.0, false, &Default::default()).unwrap();
    img.pixel(x, y)[0] < 60
}

#[test]
fn parent_options_rename_rebase_and_resize_a_parent_in_one_step_each() {
    let mut s = session();
    s.execute("layout.parents.new", &json!({})).unwrap();
    assert_eq!(parents(&mut s).len(), 2);
    let a_square = square_on(&mut s, "A", 50.0, 50.0);
    square_on(&mut s, "B", 200.0, 50.0);
    // Page 1 (a right page) takes B; B is based on nothing yet: only B's square shows.
    s.execute("layout.pages.applyParent", &json!({"pages": [1], "parent": "B"})).unwrap();
    assert!(dark_at(&mut s, 0, 220, 70));
    assert!(!dark_at(&mut s, 0, 70, 70));

    let steps = s.doc().unwrap().undo_steps();
    let r = s.execute("layout.parents.set", &json!({"parent": "B", "prefix": "C", "name": "Chapter", "basedOn": "A"})).unwrap();
    assert_eq!(r["label"], "C-Chapter");
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1, "one undo step");
    let c = parent(&mut s, "C");
    assert_eq!((c["name"].as_str(), c["basedOn"].as_str()), (Some("Chapter"), Some("A-Parent")));
    // The page still uses the renamed parent, and A's square now shows under C's.
    let page = &s.execute("file.inspect", &json!({})).unwrap()["spreads"][0]["pages"][0];
    assert_eq!(page["parent"], "C-Chapter", "{page}");
    assert!(dark_at(&mut s, 0, 220, 70));
    assert!(dark_at(&mut s, 0, 70, 70), "the based-on parent's object shows on the page");
    // And under C while C is edited (the parent spread shows what it is based on).
    assert_eq!(s.doc().unwrap().doc.parent_chain(Some(s.doc().unwrap().doc.parents[1].id), kition_layout_doc::PageSide::Right).len(), 2);

    // A cycle, a used prefix or a bad page count is refused and changes nothing.
    assert!(s.execute("layout.parents.set", &json!({"parent": "A", "basedOn": "C"})).is_err());
    assert!(s.execute("layout.parents.set", &json!({"parent": "C", "basedOn": "C"})).is_err());
    assert!(s.execute("layout.parents.set", &json!({"parent": "C", "prefix": "A"})).is_err());
    assert!(s.execute("layout.parents.set", &json!({"parent": "C", "pages": 0})).is_err());
    assert!(s.execute("layout.parents.set", &json!({"parent": "Z", "name": "Nope"})).is_err());
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);

    // A one-page A: pages go from the end, so the object on its right page goes with it.
    let left = {
        let x = s.doc().unwrap().doc.parents[0].pages[0].x;
        s.execute("frame.add", &json!({"spread": "A", "rect": [x + 10.0, 10.0, x + 50.0, 50.0], "content": "graphic"})).unwrap()["id"]
            .as_u64()
            .unwrap()
    };
    s.execute("layout.parents.set", &json!({"parent": "A", "pages": 1})).unwrap();
    assert_eq!(parent(&mut s, "A")["pages"], 1);
    assert!(s.doc().unwrap().doc.item(kition_layout_doc::ItemId(a_square)).is_none(), "the right page's object went with it");
    assert!(s.doc().unwrap().doc.item(kition_layout_doc::ItemId(left)).is_some());
    s.execute("layout.parents.set", &json!({"parent": "A", "pages": 3})).unwrap();
    assert_eq!(parent(&mut s, "A")["pages"], 3);
    s.doc().unwrap().doc.check().unwrap();

    // Based on nothing again.
    s.execute("layout.parents.set", &json!({"parent": "C", "basedOn": null})).unwrap();
    assert!(parent(&mut s, "C")["basedOn"].is_null());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(parent(&mut s, "C")["basedOn"], "A-Parent");
}

#[test]
fn duplicate_parent_copies_its_objects_under_the_next_prefix() {
    let mut s = session();
    let square = square_on(&mut s, "A", 50.0, 50.0);
    let pi = 0;
    let x = s.doc().unwrap().doc.parents[pi].pages[1].x;
    s.execute("frame.add", &json!({"spread": "A", "rect": [x + 50.0, 300.0, x + 300.0, 400.0], "content": "text", "text": "Running head"})).unwrap();
    let steps = s.doc().unwrap().undo_steps();
    let r = s.execute("layout.parents.duplicate", &json!({"parent": "A-Parent"})).unwrap();
    assert_eq!(r["prefix"], "B");
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);
    let b = parent(&mut s, "B");
    assert_eq!((b["name"].as_str(), b["items"].as_u64(), b["pages"].as_u64()), (Some("Parent"), Some(2), Some(2)));
    let d = s.doc().unwrap().doc.clone();
    let copy = &d.parents[1];
    assert!(copy.items.iter().all(|it| it.id.0 != square), "new objects, not the same ones");
    // The text frame has a story of its own with the same text.
    let story = copy.items.iter().find_map(|it| it.text_frame().map(|t| t.story)).unwrap();
    assert_eq!(d.story(story).unwrap().text, "Running head");
    assert!(d.parents[0].items.iter().all(|it| it.text_frame().is_none_or(|t| t.story != story)));
    d.check().unwrap();
    assert!(s.execute("layout.parents.duplicate", &json!({"parent": "Q"})).is_err());
}

#[test]
fn delete_parent_hands_its_pages_and_based_parents_to_the_replacement() {
    let mut s = session();
    s.execute("layout.parents.new", &json!({})).unwrap();
    s.execute("layout.parents.new", &json!({})).unwrap();
    let doomed = square_on(&mut s, "B", 50.0, 50.0);
    s.execute("layout.pages.applyParent", &json!({"pages": [1, 2], "parent": "B"})).unwrap();
    s.execute("layout.parents.set", &json!({"parent": "C", "basedOn": "B"})).unwrap();
    let steps = s.doc().unwrap().undo_steps();
    let r = s.execute("layout.parents.delete", &json!({"parent": "B", "replaceWith": "A"})).unwrap();
    assert_eq!((r["deleted"].as_str(), r["pages"].as_u64()), (Some("B-Parent"), Some(2)));
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);
    assert!(s.doc().unwrap().doc.item(kition_layout_doc::ItemId(doomed)).is_none());
    let info = s.execute("file.inspect", &json!({})).unwrap();
    let names: Vec<_> =
        info["spreads"].as_array().unwrap().iter().flat_map(|sp| sp["pages"].as_array().unwrap().clone()).map(|p| p["parent"].clone()).collect();
    assert_eq!(names, vec![json!("A-Parent"), json!("A-Parent"), json!("A-Parent")]);
    assert_eq!(parent(&mut s, "C")["basedOn"], "A-Parent");
    s.doc().unwrap().doc.check().unwrap();

    // Without a replacement the pages use none; undo brings the parent and its pages back.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(parents(&mut s).len(), 3);
    s.execute("layout.parents.delete", &json!({"parent": "B"})).unwrap();
    let info = s.execute("file.inspect", &json!({})).unwrap();
    assert!(info["spreads"][0]["pages"][0]["parent"].is_null());
    assert!(parent(&mut s, "C")["basedOn"].is_null());
    assert!(s.execute("layout.parents.delete", &json!({"parent": "A", "replaceWith": "A"})).is_err());
    // The last parent can go too: the pages then use none.
    s.execute("layout.parents.delete", &json!({"parent": "A"})).unwrap();
    s.execute("layout.parents.delete", &json!({"parent": "C"})).unwrap();
    assert!(parents(&mut s).is_empty());
    s.execute("layout.pages.insert", &json!({"count": 1})).unwrap();
    s.doc().unwrap().doc.check().unwrap();
}
