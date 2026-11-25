//! Regression (cases/regressions/issue-stable-ids): the ids `layout_inspect`, the commands and the
//! diagnostics use name the same objects after the document is read again (KHP A46, A53, A57).
//! They were numbered afresh on every read of the replica, so after a save and reopen, a
//! replica-only open (`model.open`) or the reopen with which Kition rolls back a failed
//! `layout_run`, an id an agent had noted named another object (a story 10 became 7, frames moved
//! by one). Each object now stores its number in the replica.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_kit::session::model::{Exchange, ReplicaPort};
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

const BASIC: &[u8] = include_bytes!("../../../conformance/basic.klayout");

fn run(doc: &mut LayoutDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

/// What `layout_inspect` answers, without what belongs to the editing session (the selection,
/// the tool, whether the document is dirty or can undo).
fn inspected(doc: &LayoutDocument) -> Value {
    let mut v = doc.inspect().unwrap();
    for key in ["dirty", "canUndo", "canRedo", "selection", "tool", "path"] {
        v.as_object_mut().unwrap().remove(key);
    }
    v
}

/// Every item id `layout_inspect` lists, depth first.
fn item_ids(doc: &LayoutDocument) -> Vec<u64> {
    fn walk(item: &Value, out: &mut Vec<u64>) {
        out.extend(item["id"].as_u64());
        for child in item["children"].as_array().into_iter().flatten() {
            walk(child, out);
        }
    }
    let v = doc.inspect().unwrap();
    let mut out = Vec::new();
    for spread in v["spreads"].as_array().into_iter().chain(v["parents"].as_array()).flatten() {
        for item in spread["items"].as_array().into_iter().flatten() {
            walk(item, &mut out);
        }
    }
    out
}

/// A 2x2 PNG.
fn picture() -> String {
    let picture = image::RgbaImage::from_fn(2, 2, |x, y| image::Rgba([(x * 120) as u8, (y * 120) as u8, 160, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, image::ImageFormat::Png).unwrap();
    kition_layout_engine::cmd::base64_encode(&out.into_inner())
}

/// A layout with one of most kinds: text frames and their story, a table, an image, a group, a
/// second layer and an inserted page.
fn made() -> LayoutDocument {
    let mut doc = LayoutDocument::open("basic.klayout", BASIC).unwrap();
    run(&mut doc, "layer.new", json!({ "name": "Notes" }));
    run(&mut doc, "layout.pages.insert", json!({}));
    let a = run(&mut doc, "frame.add", json!({ "rect": [40, 400, 200, 460], "content": "unassigned" }))["id"].clone();
    let b = run(&mut doc, "frame.add", json!({ "rect": [240, 400, 400, 460], "content": "unassigned" }))["id"].clone();
    run(&mut doc, "arrange.group", json!({ "ids": [a, b] }));
    run(&mut doc, "file.place", json!({ "base64": picture(), "name": "dot.png", "x": 72, "y": 500, "width": 40 }));
    run(&mut doc, "frame.add", json!({ "rect": [36, 36, 500, 300], "content": "text", "text": "A story of its own" }));
    run(&mut doc, "table.insert", json!({ "rows": 2, "cols": 2 }));
    doc
}

#[test]
fn inspect_answers_the_same_ids_after_save_and_reopen() {
    let mut doc = made();
    let want = inspected(&doc);
    let bytes = doc.save(None).unwrap();
    doc.mark_saved();
    let again = LayoutDocument::open("basic.klayout", &bytes).unwrap();
    assert_eq!(inspected(&again), want);
    let twice = LayoutDocument::open("basic.klayout", &again.save(None).unwrap()).unwrap();
    assert_eq!(inspected(&twice), want);
}

#[test]
fn a_replica_opened_from_its_changes_answers_the_same_ids() {
    let mut doc = made();
    let want = inspected(&doc);
    let (changes, blobs) = (doc.model_export(&[]).unwrap(), doc.blobs().unwrap());
    let replica = LayoutDocument::model_open_replica("basic.klayout", &changes, &blobs).unwrap();
    assert_eq!(inspected(&replica), want, "model.export, then model.open");
    let snapshot = LayoutDocument::open_with_replica("basic.klayout", &[], &doc.snapshot().unwrap(), &blobs).unwrap();
    assert_eq!(inspected(&snapshot), want, "a snapshot");
}

/// `layout_run` saves after a run; when a later run's step fails, Kition rolls the document back by
/// opening its file again. The ids an agent read before must still hold.
#[test]
fn a_failed_run_rolled_back_by_reopening_answers_the_same_ids() {
    let mut doc = LayoutDocument::open("basic.klayout", BASIC).unwrap();
    // A run that succeeds and is saved.
    run(&mut doc, "frame.add", json!({ "rect": [36, 36, 300, 200], "content": "text", "text": "Headline" }));
    run(&mut doc, "frame.add", json!({ "rect": [36, 400, 300, 500], "content": "graphic" }));
    let saved = doc.save(None).unwrap();
    doc.mark_saved();
    let before = inspected(&doc);
    // A run whose last step fails.
    run(&mut doc, "frame.add", json!({ "rect": [300, 300, 400, 400], "content": "text", "text": "More" }));
    assert!(doc.execute("frame.fitText", json!({ "ids": [999_999] })).is_err(), "the last step fails");
    let rolled_back = LayoutDocument::open("basic.klayout", &saved).unwrap();
    assert_eq!(inspected(&rolled_back), before);
}

/// An item made through the object model (`layout_batch`) gets an id that the next read keeps, and
/// what `layout_query` loads of each object carries that id as `number`.
#[test]
fn an_item_made_through_the_object_model_keeps_its_id() {
    let mut doc = LayoutDocument::open("basic.klayout", BASIC).unwrap();
    let mut exchange = Exchange::new();
    let loaded = exchange.batch(&mut doc, &json!({ "ops": [], "load": [{ "target": "layout", "fields": ["spreads", "items"] }] }), true).unwrap();
    let fields = &loaded["loaded"][0]["fields"];
    let spread = fields["spreads"][0].as_str().unwrap().to_owned();
    let first = fields["items"][0]["object"].as_str().unwrap().to_owned();
    let path = exchange.batch(&mut doc, &json!({ "ops": [], "load": [{ "target": first, "fields": ["path", "number"] }] }), true).unwrap();
    let first_number = path["loaded"][0]["fields"]["number"].clone();
    assert!(item_ids(&doc).iter().any(|id| json!(id) == first_number), "an object's number is its inspect id: {first_number}");
    let before = item_ids(&doc);
    let create = json!({ "ops": [{ "op": "create", "type": "Item", "in": "layout.items", "fields": {
        "shape": "rectangle", "content": "unassigned", "xf": [1, 0, 0, 1, 100, 100], "spread": spread,
        "path": path["loaded"][0]["fields"]["path"],
    } }] });
    exchange.batch(&mut doc, &create, false).unwrap();
    let after = item_ids(&doc);
    let new: Vec<u64> = after.iter().filter(|id| !before.contains(id)).copied().collect();
    assert_eq!(new.len(), 1, "{before:?} -> {after:?}");
    let reopened = LayoutDocument::open("basic.klayout", &doc.save(None).unwrap()).unwrap();
    assert_eq!(item_ids(&reopened), after);
    let replica = LayoutDocument::model_open_replica("basic.klayout", &doc.model_export(&[]).unwrap(), &[]).unwrap();
    assert_eq!(item_ids(&replica), after);
}

/// IDML: the importer numbers objects in the package's reading order with one counter, so two
/// imports of one file answer the same ids, and the layout saved from an import keeps them.
#[test]
fn two_imports_of_one_idml_file_answer_the_same_ids() {
    let idml = made().save(Some("made.idml")).unwrap();
    let one = LayoutDocument::open("made.idml", &idml).unwrap();
    let two = LayoutDocument::open("made.idml", &idml).unwrap();
    assert_eq!(inspected(&one), inspected(&two));
    let saved = LayoutDocument::open("made.klayout", &one.save(None).unwrap()).unwrap();
    let mut want = inspected(&one);
    let mut got = inspected(&saved);
    for v in [&mut want, &mut got] {
        v.as_object_mut().unwrap().remove("title");
    }
    assert_eq!(got, want);
}
