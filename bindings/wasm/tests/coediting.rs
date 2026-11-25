//! A layout's replica as a host and its pages use it (kition-sdk data model 7.3, Kit UI protocol 7),
//! at the engine: a viewer's editor runs only what leaves the replica alone, and a viewer's page on
//! the editor's line of changes follows the editor's edits as they arrive, in order. Concurrent
//! editing by several people is not here: the KDM keeps one line of changes (kition-sdk plan
//! 2026-11-24, plain model), and co-editing comes back with Kition One's server-ordered changes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::path::Path;

use common::{LayoutKit, NAME, exec};
use kition_convergence::KitAdapter;
use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

/// The edit Kition One's cases make in a layout: writer `writer`'s text frame.
fn ones_edit(doc: &mut LayoutDocument, text: &str, writer: u32) {
    let x = 20 + 120 * writer;
    exec(doc, "frame.add", json!({ "rect": [x, 20, x + 100, 80], "text": text })).unwrap();
}

/// What the editor's page pushed since `sent`, with its blobs, taken by `to`.
fn relay(from: &mut LayoutDocument, sent: &mut Vec<u8>, to: &mut LayoutDocument) {
    let changes = from.export_changes(sent).unwrap();
    let blobs = from.blobs_since(sent).unwrap();
    to.add_blobs(&blobs).unwrap();
    to.import_changes(&changes).unwrap();
    *sent = from.version().unwrap();
}

fn state(doc: &mut LayoutDocument) -> Value {
    LayoutKit.state(doc).unwrap()
}

fn pixels(doc: &LayoutDocument) -> Vec<u8> {
    doc.render(0.0, 0.0, 0.5, doc.width() / 2, doc.height() / 2).unwrap()
}

/// Both pages hold the same layout: the same replica state, the same version, the same view.
fn assert_same(a: &mut LayoutDocument, b: &mut LayoutDocument) {
    assert_eq!(state(a), state(b), "the replicas hold the same state");
    assert_eq!(a.version().unwrap(), b.version().unwrap(), "at the same version of one line of changes");
    assert_eq!(pixels(a), pixels(b), "and so do their views");
    LayoutKit.check_view(a).unwrap();
    LayoutKit.check_view(b).unwrap();
}

/// The file both pages open: a layout saved as `.klayout`.
fn file() -> Vec<u8> {
    common::starting_layout().unwrap().save(None).unwrap()
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
    let frame = commands.iter().find(|command| command.id == "frame.add").unwrap();
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

/// A viewer opened on the editor's replica sees the editor's edits as they arrive.
#[test]
fn a_viewers_page_follows_the_editors() {
    let bytes = file();
    let mut ann = LayoutDocument::open(NAME, &bytes).unwrap();
    let snapshot = ann.snapshot().unwrap();
    let blobs = ann.blobs().unwrap();
    let mut viewer = LayoutDocument::open_with_replica(NAME, &bytes, &snapshot, &blobs).unwrap();
    assert_eq!(viewer.doc_id(), ann.doc_id(), "one document");
    let mut sent = ann.version().unwrap();
    let opened = sent.clone();
    let before = pixels(&viewer);
    ones_edit(&mut ann, "ann", 0);
    relay(&mut ann, &mut sent, &mut viewer);
    assert_ne!(pixels(&viewer), before, "the viewer's canvas shows the new frame");
    assert_same(&mut ann, &mut viewer);
    // A second edit, then every change relayed again: what the viewer holds is skipped.
    ones_edit(&mut ann, "ann again", 1);
    relay(&mut ann, &mut sent, &mut viewer);
    let mut again = opened;
    relay(&mut ann, &mut again, &mut viewer);
    assert_same(&mut ann, &mut viewer);
}
