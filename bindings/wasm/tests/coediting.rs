//! Co-editing a layout as Kition One's managed co-editing cases do it (kition-one
//! `e2e/collab-managed.mjs`: converge, viewer, reconnect after 31 s, restart, first-open race), at
//! the engine: each editor's page keeps a replica of the file (`LayoutDocument`), and a room
//! (here [`Room`], the change log Kition Server keeps) relays the changes between them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::path::Path;

use common::{LayoutKit, NAME, exec, objects};
use kition_convergence::KitAdapter;
use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

/// The edit Kition One's cases make in a layout: writer `writer`'s text frame.
fn ones_edit(doc: &mut LayoutDocument, text: &str, writer: u32) {
    let x = 20 + 120 * writer;
    exec(doc, "frame.create", json!({ "rect": [x, 20, x + 100, 80], "text": text })).unwrap();
}

/// A room: every change any replica pushed, in arrival order, with their blobs.
#[derive(Default)]
struct Room {
    log: Vec<(Vec<u8>, Vec<u8>)>,
}

/// One editor's page: its replica and the room version it last pushed from.
struct Page {
    doc: LayoutDocument,
    sent: Vec<u8>,
}

impl Page {
    fn open(bytes: &[u8]) -> Self {
        let mut doc = LayoutDocument::open(NAME, bytes).unwrap();
        let sent = doc.version().unwrap();
        Self { doc, sent }
    }

    /// Pushes what this replica has and the room has not.
    fn push(&mut self, room: &mut Room) {
        let changes = self.doc.export_changes(&self.sent).unwrap();
        let blobs = self.doc.blobs_since(&self.sent).unwrap();
        room.log.push((changes, blobs));
        self.sent = self.doc.version().unwrap();
    }

    /// Takes every change of the room (again: replicas ignore what they hold).
    fn pull(&mut self, room: &Room) {
        for (changes, blobs) in &room.log {
            self.doc.add_blobs(blobs).unwrap();
            self.doc.import_changes(changes).unwrap();
        }
    }
}

fn state(doc: &mut LayoutDocument) -> Value {
    LayoutKit.state(doc).unwrap()
}

fn pixels(doc: &LayoutDocument) -> Vec<u8> {
    doc.render(0.0, 0.0, 0.5, doc.width() / 2, doc.height() / 2).unwrap()
}

/// Both pages hold the same layout: the same replica state, the same view, the same pixels.
fn assert_converged(a: &mut LayoutDocument, b: &mut LayoutDocument) {
    assert_eq!(state(a), state(b), "the replicas converge");
    // Each holds every change of the other (a version's bytes list actors in no fixed order).
    let (va, vb) = (a.version().unwrap(), b.version().unwrap());
    assert_eq!(a.export_changes(&vb).unwrap(), b.export_changes(&vb).unwrap(), "nothing one has is new to the other");
    assert_eq!(a.export_changes(&va).unwrap(), b.export_changes(&va).unwrap());
    assert_eq!(pixels(a), pixels(b), "and so do their views");
    LayoutKit.check_view(a).unwrap();
    LayoutKit.check_view(b).unwrap();
}

fn texts(doc: &LayoutDocument) -> Vec<String> {
    let inspected = doc.inspect().unwrap();
    let mut out: Vec<String> =
        inspected["stories"].as_array().unwrap().iter().filter_map(|story| story["preview"].as_str().map(str::to_owned)).collect();
    out.sort();
    out
}

/// The file both editors open: a layout saved as `.klayout` (it carries its replica).
fn file() -> Vec<u8> {
    common::starting_layout().unwrap().seal_and_save(None).unwrap()
}

/// MC-17/18: two editors edit at once, each with One's own edit, and converge.
#[test]
fn two_editors_editing_at_once_converge() {
    let bytes = file();
    let (mut room, mut ann, mut bob) = (Room::default(), Page::open(&bytes), Page::open(&bytes));
    assert_eq!(ann.doc.doc_id(), bob.doc.doc_id(), "both open the file's own document: one room");
    let frames = objects(&ann.doc).len();
    ones_edit(&mut ann.doc, "ann", 0);
    ones_edit(&mut bob.doc, "bob", 1);
    ann.push(&mut room);
    bob.push(&mut room);
    ann.pull(&room);
    bob.pull(&room);
    assert_converged(&mut ann.doc, &mut bob.doc);
    assert_eq!(objects(&ann.doc).len(), frames + 2, "both frames are kept");
    let texts = texts(&ann.doc);
    assert!(texts.contains(&"ann".to_owned()) && texts.contains(&"bob".to_owned()), "{texts:?}");
    // The view follows each import as it comes: no reopen.
    assert!(!ann.doc.can_redo());
}

/// The platform's read-only rules (kition-sdk `packages/kit-editor/src/engine/readOnly.ts`
/// `editsDocument`), which a viewer's editor applies to every command: whether `command` is an
/// edit the editor refuses before it runs.
fn viewer_refuses(id: &str, mutates: bool) -> bool {
    const VIEWING_GROUPS: [&str; 10] = ["view", "select", "selection", "go", "navigate", "help", "playback", "transport", "playhead", "caret"];
    const EDITING_COMMANDS: [&str; 5] = ["edit.undo", "edit.redo", "undo", "redo", "u"];
    let group = id.split('.').next().unwrap_or("");
    if EDITING_COMMANDS.contains(&id) {
        return true;
    }
    if group == "file" {
        return !["file.export", "file.saveCopy", "file.saveACopy", "file.print"].iter().any(|prefix| id.starts_with(prefix));
    }
    if VIEWING_GROUPS.contains(&group) {
        return false;
    }
    mutates
}

/// MC-02: what a viewer's editor lets run never writes to the layout's replica, so a viewer has
/// no change to send; the edits it refuses (One's edit among them) are the layout's edits.
#[test]
fn a_viewer_runs_only_what_leaves_the_replica_alone() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.klayout"));
    let bytes = std::fs::read(path).unwrap();
    let samples: Value = serde_json::from_str(include_str!("command_samples.json")).unwrap();
    let tmp = std::env::temp_dir().join(format!("kition-layout-viewer-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let fill = |value: &Value| -> Value { serde_json::from_str(&value.to_string().replace("$tmp", &tmp.to_string_lossy())).unwrap() };
    let commands = LayoutDocument::open("basic.klayout", &bytes).unwrap().commands();
    let frame = commands.iter().find(|command| command.id == "frame.create").unwrap();
    assert!(viewer_refuses(&frame.id, frame.mutates), "One's edit is refused to a viewer");
    let mut ran = 0;
    let mut wrote = Vec::new();
    for command in commands.iter().filter(|command| !viewer_refuses(&command.id, command.mutates)) {
        let entry = &samples[&command.id];
        if entry["skip"].is_string() {
            continue;
        }
        let mut doc = LayoutDocument::open("basic.klayout", &bytes).unwrap();
        // A setup step is the sample's, not the viewer's: the replica is read after it.
        let setup_ok = entry["setup"].as_array().into_iter().flatten().all(|step| doc.execute(step[0].as_str().unwrap(), fill(&step[1])).is_ok());
        if !setup_ok {
            continue;
        }
        let before = doc.version().unwrap();
        if doc.execute(&command.id, entry.get("params").map_or(json!({}), fill)).is_err() {
            continue;
        }
        ran += 1;
        if !doc.export_changes(&before).unwrap().is_empty() && doc.version().unwrap() != before {
            wrote.push(command.id.clone());
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(ran > 50, "the viewer's commands ran ({ran})");
    assert!(wrote.is_empty(), "a viewer's command wrote to the replica: {wrote:?}");
}

/// MC-16/17: a page offline for a while (31 s in One's case), edited on both sides, converges
/// when it reconnects: it pushes what it made offline and takes what it missed, some of it twice.
#[test]
fn a_page_offline_and_edited_on_both_sides_converges_when_it_reconnects() {
    let bytes = file();
    let (mut room, mut ann, mut bob) = (Room::default(), Page::open(&bytes), Page::open(&bytes));
    ones_edit(&mut ann.doc, "before", 0);
    ann.push(&mut room);
    bob.pull(&room);
    // Bob goes offline; both keep editing.
    for (index, text) in ["ann one", "ann two"].into_iter().enumerate() {
        ones_edit(&mut ann.doc, text, 2 + index as u32);
        ann.push(&mut room);
    }
    exec(&mut bob.doc, "layout.pages.insert", json!({ "count": 1 })).unwrap();
    ones_edit(&mut bob.doc, "bob offline", 1);
    let story = common::story(&bob.doc).unwrap();
    exec(&mut bob.doc, "story.replaceRange", json!({ "story": story, "start": 0, "end": 0, "text": "Live: " })).unwrap();
    // Reconnected: Bob pushes, then takes the whole room again (what he had included).
    bob.push(&mut room);
    bob.pull(&room);
    ann.pull(&room);
    assert_converged(&mut ann.doc, &mut bob.doc);
    let texts = texts(&ann.doc);
    for text in ["before", "ann one", "ann two", "bob offline"] {
        assert!(texts.contains(&text.to_owned()), "{text} is kept: {texts:?}");
    }
    assert!(texts.iter().any(|text| text.starts_with("Live: Spring concert")), "{texts:?}");
    assert_eq!(common::pages(&ann.doc), 4);
}

/// MC-24: acknowledged edits survive a restart: a fresh page opens the file as it was saved
/// before them and takes the room's changes; a page that has no file yet opens the room's
/// replica alone. Both read the same layout.
#[test]
fn a_fresh_page_reads_the_rooms_edits_from_the_file_or_the_replica() {
    let bytes = file();
    let (mut room, mut ann, mut bob) = (Room::default(), Page::open(&bytes), Page::open(&bytes));
    ones_edit(&mut ann.doc, "ann", 0);
    ann.push(&mut room);
    ones_edit(&mut bob.doc, "bob", 1);
    bob.push(&mut room);
    ann.pull(&room);
    let mut fresh = Page::open(&bytes);
    fresh.pull(&room);
    assert_converged(&mut ann.doc, &mut fresh.doc);
    let snapshot = ann.doc.snapshot().unwrap();
    let blobs = ann.doc.blobs().unwrap();
    let mut from_replica = LayoutDocument::open_with_replica(NAME, &[], &snapshot, &blobs).unwrap();
    assert_eq!(from_replica.doc_id(), ann.doc.doc_id());
    assert_converged(&mut ann.doc, &mut from_replica);
    // The room's save writes the file (sealed first, as a host saves); reopened, it is the same
    // layout, under the same id.
    ann.doc.prepare_save().unwrap();
    let saved = ann.doc.seal_and_save(None).unwrap();
    let mut reopened = LayoutDocument::open(NAME, &saved).unwrap();
    assert_eq!(reopened.doc_id(), ann.doc.doc_id());
    assert_converged(&mut ann.doc, &mut reopened);
}

/// MC-05: two editors opening a new file at once open one document (one room), and the first
/// edits of each reach the other.
#[test]
fn two_editors_opening_a_new_file_at_once_open_one_document() {
    let bytes = LayoutDocument::create(NAME, 612, 792).unwrap().seal_and_save(None).unwrap();
    let (mut room, mut ann, mut bob) = (Room::default(), Page::open(&bytes), Page::open(&bytes));
    assert_eq!(ann.doc.doc_id(), bob.doc.doc_id());
    assert!(ann.doc.doc_id().is_some());
    ones_edit(&mut bob.doc, "bob", 1);
    ones_edit(&mut ann.doc, "ann", 0);
    bob.push(&mut room);
    ann.push(&mut room);
    ann.pull(&room);
    bob.pull(&room);
    assert_converged(&mut ann.doc, &mut bob.doc);
    assert_eq!(objects(&ann.doc).len(), 2);
}

/// A viewer opened before an editor joined sees the editor's edits as they arrive.
#[test]
fn a_viewers_page_follows_the_editors() {
    let bytes = file();
    let (mut room, mut ann, mut viewer) = (Room::default(), Page::open(&bytes), Page::open(&bytes));
    let before = pixels(&viewer.doc);
    ones_edit(&mut ann.doc, "ann", 0);
    ann.push(&mut room);
    viewer.pull(&room);
    assert_ne!(pixels(&viewer.doc), before, "the viewer's canvas shows the new frame");
    assert_converged(&mut ann.doc, &mut viewer.doc);
}
