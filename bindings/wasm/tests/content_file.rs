//! A `.klayout` file holds the document's content, not its history (kition-sdk data model 7.3,
//! 7.4, rule M16b; feature `layout.file.content-only`): `content.json` holds the state, none of the
//! deleted or overwritten content, and no version, actor or origin; an unchanged layout saves to
//! the same bytes; a file opened without a store is the same document (docId, object ids, state)
//! and edits; undo after a save takes back the last edit; and a file opens on top of a stored
//! replica, as it is when both hold the same state, else by the file's state written by object id.
//!
//! The needles below are written once each and share no run of four characters with anything else
//! in the layout: every test checks the session's change log holds them (the control) before it
//! checks the file does not, in its bytes and in its content entry unzipped.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Replica, Version};
use kition_kit::session::model::Exchange;
use kition_layout_format::container;
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

const FILE: &str = "Plan.klayout";
const TYPED_THEN_DELETED: &str = "Jx4Hb8Ty3Nc6";
const DELETED_ELEMENT: &str = "Qv7Zp9Wm2Rk5";
const OVERWRITTEN: &str = "Ug5Le1Fo0Dw4";
const NEEDLES: [&str; 3] = [TYPED_THEN_DELETED, DELETED_ELEMENT, OVERWRITTEN];

fn contains(bytes: &[u8], needle: &str) -> bool {
    let utf16: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.windows(needle.len()).any(|window| window == needle.as_bytes()) || bytes.windows(utf16.len()).any(|window| window == utf16.as_slice())
}

/// The file as a host saves it.
fn host_save(doc: &mut LayoutDocument) -> Vec<u8> {
    doc.save(Some(FILE)).unwrap()
}

fn content(bytes: &[u8]) -> Vec<u8> {
    container::read(bytes).unwrap().content
}

/// Whether a file holds `needle`, in its bytes or in its content entry unzipped.
fn file_holds(bytes: &[u8], needle: &str) -> bool {
    contains(bytes, needle) || contains(&content(bytes), needle)
}

/// The replicated state of `doc`, as its replica answers it.
fn state(doc: &mut LayoutDocument) -> Value {
    Replica::load(kition_layout_kdm::schema(), &doc.snapshot().unwrap(), ActorId::random()).unwrap().state_json().unwrap()
}

fn version(doc: &mut LayoutDocument) -> Version {
    Version::decode(&doc.version().unwrap()).unwrap()
}

/// The items of every spread and their ids, as `inspect` answers them.
fn items(doc: &LayoutDocument) -> Value {
    let inspected = doc.inspect().unwrap();
    Value::Array(inspected["spreads"].as_array().unwrap().iter().flat_map(|spread| spread["items"].as_array().unwrap().clone()).collect())
}

fn set_title(doc: &mut LayoutDocument, title: &str) {
    Exchange::new().batch(doc, &json!({ "ops": [{ "op": "set", "target": "layout", "field": "title", "value": title }] }), false).unwrap();
}

/// A layout whose change log holds typed-then-deleted text, a deleted frame and an overwritten
/// value; answers it, the id of the text frame that stays and the version it was made at.
fn layout_with_history() -> (LayoutDocument, Value, Vec<u8>) {
    let mut doc = LayoutDocument::create(FILE, 612, 792).unwrap();
    let made_at = doc.version().unwrap();
    let kept = doc.execute("frame.add", json!({ "rect": [72, 72, 300, 200], "content": "text", "text": "Kept" })).unwrap();
    let story = kept["story"].clone();
    doc.execute("text.select", json!({ "story": story, "anchor": 4, "focus": 4 })).unwrap();
    doc.execute("text.insert", json!({ "text": TYPED_THEN_DELETED, "raw": true })).unwrap();
    doc.execute("text.select", json!({ "story": story, "anchor": 4, "focus": 4 + TYPED_THEN_DELETED.len() })).unwrap();
    doc.execute("text.delete", json!({})).unwrap();
    let gone = doc.execute("frame.add", json!({ "rect": [72, 300, 300, 400], "content": "text", "text": DELETED_ELEMENT })).unwrap()["id"].clone();
    doc.execute("edit.delete", json!({ "ids": [gone] })).unwrap();
    set_title(&mut doc, OVERWRITTEN);
    set_title(&mut doc, "Final");
    (doc, kept["id"].clone(), made_at)
}

#[test]
fn an_unchanged_layout_saves_to_the_same_bytes_and_its_content_holds_no_history() {
    let (mut doc, _, made_at) = layout_with_history();
    let log = doc.export_changes(&made_at).unwrap();
    for needle in NEEDLES {
        assert!(contains(&log, needle), "the control: the session's change log holds {needle}");
    }
    let first = host_save(&mut doc);
    for needle in NEEDLES {
        assert!(!file_holds(&first, needle), "the file holds {needle}");
    }
    // The content is the state, with no version, lineage, actor or origin (data model 7.4).
    let text = String::from_utf8(content(&first)).unwrap();
    let doc_id = doc.doc_id().unwrap();
    assert!(text.starts_with(&format!("{{\"kdm\":1,\"kit\":\"kit.layout\",\"docId\":\"{doc_id}\",\"schemaVersion\":1,\"objects\":{{\n")), "{text}");
    assert!(!text.contains("\"version\"") && !text.contains("lineage") && !text.contains("person:"), "{text}");
    let at = version(&mut doc);
    assert_eq!(host_save(&mut doc), first, "two saves with nothing between");
    assert_eq!(version(&mut doc), at, "a save writes nothing into the replica");
    doc.mark_saved();
    assert_eq!(host_save(&mut doc), first, "and after the host stored the first");
    let mut opened = LayoutDocument::open(FILE, &first).unwrap();
    assert_eq!(state(&mut opened), state(&mut doc), "the state is all there, under the same ids");
    assert_eq!(items(&opened), items(&doc), "the same object ids");
    assert_eq!(opened.inspect().unwrap()["title"], "Final");
    assert_eq!(host_save(&mut opened), first, "and from the file opened again");
}

/// Two people's sessions that open the same file and save it without an edit write the same
/// whole file, byte for byte: the file holds nothing of the session that wrote it (no actor of
/// its own, no time, no view, no selection), even after each looked at it its own way.
#[test]
fn two_sessions_that_open_one_file_and_save_unedited_write_the_same_bytes() {
    let (mut doc, kept, _) = layout_with_history();
    let file = host_save(&mut doc);
    let mut one = LayoutDocument::open(FILE, &file).unwrap();
    let mut two = LayoutDocument::open(FILE, &file).unwrap();
    // Each person looks at it differently: a selection, a render, an inspect.
    one.execute("select.set", json!({ "ids": [kept] })).unwrap();
    let _ = one.render(0.0, 0.0, 0.5, 300, 400).unwrap();
    let _ = two.inspect().unwrap();
    assert!(!one.dirty() && !two.dirty(), "looking is no edit");
    let (from_one, from_two) = (host_save(&mut one), host_save(&mut two));
    assert!(from_one == from_two, "two sessions saved other bytes for the same unedited file");
    assert!(from_one == file, "and the file they opened");
}

#[test]
fn a_file_opened_without_a_store_is_the_same_document_and_edits() {
    let (mut doc, kept, _) = layout_with_history();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    let mut back = LayoutDocument::open(FILE, &bytes).unwrap();
    assert_eq!(back.doc_id(), doc.doc_id());
    assert_eq!(items(&back), items(&doc), "the same object ids");
    assert_eq!(state(&mut back), state(&mut doc), "the same state");
    assert!(!back.dirty() && !back.can_undo());
    back.execute("object.opacity", json!({ "ids": [kept], "opacity": 0.5 })).unwrap();
    let made = back.execute("frame.add", json!({ "rect": [72, 500, 300, 600], "content": "text", "text": "New" })).unwrap()["id"].clone();
    let saved = host_save(&mut back);
    let again = LayoutDocument::open(FILE, &saved).unwrap();
    assert_eq!(again.doc_id(), doc.doc_id());
    assert_eq!(items(&again), items(&back));
    let ids: Vec<Value> = items(&again).as_array().unwrap().iter().map(|item| item["id"].clone()).collect();
    assert!(ids.contains(&kept) && ids.contains(&made), "{ids:?}");
}

#[test]
fn undo_after_a_save_takes_back_the_last_edit() {
    let mut doc = LayoutDocument::create(FILE, 612, 792).unwrap();
    let id = doc.execute("frame.add", json!({ "rect": [72, 72, 200, 200], "content": "unassigned" })).unwrap()["id"].clone();
    doc.execute("object.opacity", json!({ "ids": [id], "opacity": 0.5 })).unwrap();
    host_save(&mut doc);
    doc.mark_saved();
    assert!(doc.can_undo(), "a save is no undo step and clears none");
    assert!(doc.undo().unwrap().0);
    assert!(doc.dirty(), "undone past the save");
    assert_eq!(items(&doc).as_array().unwrap().len(), 1, "the undo took back the opacity, not the frame");
    assert!(doc.undo().unwrap().0);
    assert!(items(&doc).as_array().unwrap().is_empty(), "then the frame");
    assert!(doc.redo().unwrap().0);
    assert_eq!(items(&doc).as_array().unwrap().len(), 1);
}

#[test]
fn a_file_opens_on_the_stored_replica_that_holds_its_state() {
    let (mut doc, _, _) = layout_with_history();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    let stored = doc.snapshot().unwrap();
    let mut opened = LayoutDocument::open_with_replica(FILE, &bytes, &stored, &doc.blobs().unwrap()).unwrap();
    assert_eq!(version(&mut opened), version(&mut doc), "the stored replica, as it was");
    assert_eq!(state(&mut opened), state(&mut doc));
    assert_eq!(host_save(&mut opened), bytes, "and saves the document it opened to the same bytes");
}

#[test]
fn a_file_saved_elsewhere_opens_on_the_stored_replica_by_its_state() {
    let (mut doc, kept, _) = layout_with_history();
    let stored = doc.snapshot().unwrap();
    let stored_version = version(&mut doc);
    let blobs = doc.blobs().unwrap();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    // Elsewhere, from the file alone: an edit, a new frame, a save.
    let mut elsewhere = LayoutDocument::open(FILE, &bytes).unwrap();
    elsewhere.execute("object.opacity", json!({ "ids": [kept], "opacity": 0.25 })).unwrap();
    elsewhere.execute("frame.add", json!({ "rect": [72, 600, 172, 700], "content": "unassigned" })).unwrap();
    let edited = host_save(&mut elsewhere);
    let mut opened = LayoutDocument::open_with_replica(FILE, &edited, &stored, &blobs).unwrap();
    assert_eq!(opened.doc_id(), doc.doc_id());
    assert_eq!(state(&mut opened), state(&mut elsewhere), "the file's state, by object id");
    assert_eq!(items(&opened), items(&elsewhere));
    let opened_version = version(&mut opened);
    assert!(opened_version.includes(&stored_version), "on top of the stored replica, on its line of changes");
    assert_ne!(opened_version, stored_version, "with the file's state as a change");
}
