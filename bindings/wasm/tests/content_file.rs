//! A `.klayout` file holds the document's content, not its history (kition-sdk data model 7.3,
//! rule M16b; feature `layout.file.content-only`): `content.bin` holds one change (the seal) and
//! none of the deleted or overwritten content; an unchanged layout saves to the same bytes; a file
//! opened without a store is the same document (docId, object ids) and edits; a replica from the
//! file and one with the whole history converge; undo after a save takes back the last edit; and a
//! file opens on top of a stored replica, by its changes or by its state. Hosts seal before saving
//! (`prepare_save`, the direct path); the one fallback, a save nobody sealed for (the kit editor
//! before its release that calls `prepareSave`), seals a copy and `mark_saved` takes the seal in.
//!
//! The needles below are written once each and share no run of four characters with anything else
//! in the layout, so the engine's compression keeps each one literal in the bytes that hold it:
//! every test checks the session's full snapshot holds them (the control) before it checks the
//! file does not (the method of kition-sdk's `crates/kition-kdm/tests/content_snapshot.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Origin, OriginKind, Replica, Version};
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

/// The file as a host saves it: sealed first (`prepare_save`), then the bytes.
fn host_save(doc: &mut LayoutDocument) -> Vec<u8> {
    doc.prepare_save().unwrap();
    doc.save(Some(FILE)).unwrap()
}

fn content(bytes: &[u8]) -> Vec<u8> {
    container::read(bytes).unwrap().content
}

/// The version a file's content holds.
fn file_version(bytes: &[u8]) -> Vec<u8> {
    Replica::load(kition_layout_kdm::schema(), &content(bytes), ActorId::random()).unwrap().version().encode()
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

/// A layout whose history holds typed-then-deleted text, a deleted frame and an overwritten value;
/// answers it and the id of the text frame that stays.
fn layout_with_history() -> (LayoutDocument, Value) {
    let mut doc = LayoutDocument::create(FILE, 612, 792).unwrap();
    let kept = doc.execute("frame.create", json!({ "rect": [72, 72, 300, 200], "content": "text", "text": "Kept" })).unwrap();
    let story = kept["story"].clone();
    doc.execute("text.select", json!({ "story": story, "anchor": 4, "focus": 4 })).unwrap();
    doc.execute("text.insert", json!({ "text": TYPED_THEN_DELETED, "raw": true })).unwrap();
    doc.execute("text.select", json!({ "story": story, "anchor": 4, "focus": 4 + TYPED_THEN_DELETED.len() })).unwrap();
    doc.execute("text.delete", json!({})).unwrap();
    let gone = doc.execute("frame.create", json!({ "rect": [72, 300, 300, 400], "content": "text", "text": DELETED_ELEMENT })).unwrap()["id"].clone();
    doc.execute("edit.clear", json!({ "ids": [gone] })).unwrap();
    set_title(&mut doc, OVERWRITTEN);
    set_title(&mut doc, "Final");
    (doc, kept["id"].clone())
}

#[test]
fn the_file_holds_the_content_and_of_the_history_only_the_seal() {
    let (mut doc, _) = layout_with_history();
    let full = doc.snapshot().unwrap();
    for needle in NEEDLES {
        assert!(contains(&full, needle), "the control: the session's history holds {needle}");
    }
    let bytes = host_save(&mut doc);
    for needle in NEEDLES {
        assert!(!contains(&bytes, needle), "the file holds {needle}");
    }
    let held = Replica::snapshot_origins(&content(&bytes)).unwrap();
    assert_eq!(held.len(), 1, "of the history, the file holds one change: {held:?}");
    let origin = held[0].clone().unwrap();
    assert_eq!(origin, Origin::system("content-seal"));
    assert_eq!(origin.kind, OriginKind::System);
    assert!(Replica::snapshot_origins(&full).unwrap().len() > 1, "the control: the history holds every change");
    // The direct path: the seal is the session's own, before the host marks the file saved.
    assert_eq!(doc.version().unwrap(), file_version(&bytes));
    // The state is all there.
    let mut back = LayoutDocument::open(FILE, &bytes).unwrap();
    assert_eq!(state(&mut back), state(&mut doc));
    assert_eq!(back.inspect().unwrap()["title"], "Final");
}

#[test]
fn an_unchanged_layout_saves_to_the_same_bytes() {
    let (mut doc, _) = layout_with_history();
    let first = host_save(&mut doc);
    let at = doc.version().unwrap();
    assert_eq!(host_save(&mut doc), first, "two saves with nothing between");
    assert_eq!(doc.version().unwrap(), at, "a sealed layout seals nothing more");
    doc.mark_saved();
    assert_eq!(host_save(&mut doc), first, "and after the host stored the first");
    let mut opened = LayoutDocument::open(FILE, &first).unwrap();
    assert_eq!(host_save(&mut opened), first, "and from the file opened again");
}

#[test]
fn a_file_opened_without_a_store_is_the_same_document_and_edits() {
    let (mut doc, kept) = layout_with_history();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    let mut back = LayoutDocument::open(FILE, &bytes).unwrap();
    assert_eq!(back.doc_id(), doc.doc_id());
    assert_eq!(items(&back), items(&doc), "the same object ids");
    assert_eq!(back.version().unwrap(), doc.version().unwrap(), "the same version: the seal is the session's");
    assert!(!back.dirty() && !back.can_undo());
    back.execute("object.opacity", json!({ "ids": [kept], "opacity": 0.5 })).unwrap();
    let made = back.execute("frame.create", json!({ "rect": [72, 500, 300, 600], "content": "text", "text": "New" })).unwrap()["id"].clone();
    let saved = host_save(&mut back);
    let again = LayoutDocument::open(FILE, &saved).unwrap();
    assert_eq!(again.doc_id(), doc.doc_id());
    assert_eq!(items(&again), items(&back));
    let ids: Vec<Value> = items(&again).as_array().unwrap().iter().map(|item| item["id"].clone()).collect();
    assert!(ids.contains(&kept) && ids.contains(&made), "{ids:?}");
}

#[test]
fn a_replica_from_the_file_and_one_with_the_whole_history_converge() {
    let (mut full, kept) = layout_with_history();
    let bytes = host_save(&mut full);
    full.mark_saved();
    let mut file = LayoutDocument::open(FILE, &bytes).unwrap();
    // Edits on both after the save.
    full.execute("object.opacity", json!({ "ids": [kept], "opacity": 0.5 })).unwrap();
    full.execute("frame.create", json!({ "rect": [400, 600, 500, 700], "content": "unassigned" })).unwrap();
    file.execute_as("frame.create", json!({ "rect": [72, 600, 172, 700], "content": "unassigned" }), &Origin::person("person:file")).unwrap();
    set_title(&mut file, "From the file");
    for _ in 0..4 {
        let (to_file, to_full) = (full.export_changes(&file.version().unwrap()).unwrap(), file.export_changes(&full.version().unwrap()).unwrap());
        let at = full.version().unwrap();
        full.add_blobs(&file.blobs_since(&at).unwrap()).unwrap();
        full.import_changes(&to_full).unwrap();
        file.import_changes(&to_file).unwrap();
        if full.version().unwrap() == file.version().unwrap() {
            break;
        }
    }
    assert_eq!(version(&mut full), version(&mut file));
    assert_eq!(state(&mut full), state(&mut file), "both replicas hold the same document");
    assert_eq!(full.inspect().unwrap()["title"], "From the file", "the file's edit survives");
    assert_eq!(items(&full), items(&file));
    assert_eq!(items(&full).as_array().unwrap().len(), 3, "the kept frame and one new frame from each");
}

#[test]
fn undo_after_a_save_takes_back_the_last_edit() {
    let mut doc = LayoutDocument::create(FILE, 612, 792).unwrap();
    let id = doc.execute("frame.create", json!({ "rect": [72, 72, 200, 200], "content": "unassigned" })).unwrap()["id"].clone();
    doc.execute("object.opacity", json!({ "ids": [id], "opacity": 0.5 })).unwrap();
    host_save(&mut doc);
    doc.mark_saved();
    assert!(doc.can_undo(), "the seal is no undo step and clears none");
    assert!(doc.undo().unwrap().0);
    assert!(doc.dirty(), "undone past the save");
    assert_eq!(items(&doc).as_array().unwrap().len(), 1, "the undo took back the opacity, not the frame");
    assert!(doc.undo().unwrap().0);
    assert!(items(&doc).as_array().unwrap().is_empty(), "then the frame");
    assert!(doc.redo().unwrap().0);
    assert_eq!(items(&doc).as_array().unwrap().len(), 1);
}

#[test]
fn a_file_opens_on_the_stored_replica_that_holds_its_version() {
    let (mut doc, _) = layout_with_history();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    let stored = doc.snapshot().unwrap();
    let mut opened = LayoutDocument::open_with_replica(FILE, &bytes, &stored, &doc.blobs().unwrap()).unwrap();
    assert_eq!(opened.version().unwrap(), doc.version().unwrap(), "the stored replica, as it was");
    for needle in NEEDLES {
        assert!(contains(&opened.snapshot().unwrap(), needle), "the history keeps {needle}");
    }
    assert_eq!(file_version(&host_save(&mut opened)), file_version(&bytes), "and saves the document it opened, sealing nothing new");
}

#[test]
fn a_file_saved_elsewhere_opens_on_the_stored_replica_by_its_state() {
    let (mut doc, kept) = layout_with_history();
    let stored = doc.snapshot().unwrap();
    let blobs = doc.blobs().unwrap();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    // Elsewhere, from the file alone: an edit, a new frame, a save.
    let mut elsewhere = LayoutDocument::open(FILE, &bytes).unwrap();
    elsewhere.execute("object.opacity", json!({ "ids": [kept], "opacity": 0.25 })).unwrap();
    elsewhere.execute("frame.create", json!({ "rect": [72, 600, 172, 700], "content": "unassigned" })).unwrap();
    let edited = host_save(&mut elsewhere);
    let mut opened = LayoutDocument::open_with_replica(FILE, &edited, &stored, &blobs).unwrap();
    assert_eq!(opened.doc_id(), doc.doc_id());
    assert_eq!(state(&mut opened), state(&mut elsewhere), "the file's state, by object id");
    assert_eq!(items(&opened), items(&elsewhere));
    assert!(contains(&opened.snapshot().unwrap(), DELETED_ELEMENT), "on top of the stored history, which it keeps");
}

/// The fallback (delete with it once a kit-editor release that calls prepareSave is in
/// web/pnpm-lock.yaml): a save nobody sealed for writes the content of a sealed copy, the same
/// bytes for the same version, and `mark_saved` takes the seal into the session, which then
/// exchanges changes with a replica of the file.
#[test]
fn a_save_nobody_sealed_for_seals_a_copy_and_mark_saved_takes_the_seal_in() {
    let (mut doc, _) = layout_with_history();
    let at = doc.version().unwrap();
    let first = doc.save(Some(FILE)).unwrap();
    assert_eq!(doc.version().unwrap(), at, "the session is not sealed by a save");
    assert_eq!(doc.save(Some(FILE)).unwrap(), first, "the same version seals the same way");
    for needle in NEEDLES {
        assert!(!contains(&first, needle), "the file holds {needle}");
    }
    assert_eq!(Replica::snapshot_origins(&content(&first)).unwrap(), vec![Some(Origin::system("content-seal"))]);
    doc.mark_saved();
    assert!(!doc.dirty());
    assert_eq!(doc.version().unwrap(), file_version(&first), "mark_saved took the seal in");
    assert!(doc.can_undo(), "and kept the undo history");
    // A replica of the file and the session exchange changes made after the save.
    let mut file = LayoutDocument::open(FILE, &first).unwrap();
    doc.execute("frame.create", json!({ "rect": [72, 600, 172, 700], "content": "unassigned" })).unwrap();
    file.execute_as("frame.create", json!({ "rect": [300, 600, 400, 700], "content": "unassigned" }), &Origin::person("person:file")).unwrap();
    let (to_file, to_doc) = (doc.export_changes(&file.version().unwrap()).unwrap(), file.export_changes(&doc.version().unwrap()).unwrap());
    doc.import_changes(&to_doc).unwrap();
    file.import_changes(&to_file).unwrap();
    assert_eq!(state(&mut doc), state(&mut file));
}
