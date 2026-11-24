//! Regression (cases/regressions/issue-draft-replica-blobs): `kition-layout` opens a layout from its
//! replica (`model.open`, which `kit.json` lists) with the media its changes reference. An agent's
//! headless draft is such a replica (KHP A12): `model.export` answered the changes without the
//! placed images' blobs (`ReplicaPort` kept the defaults of `model_blobs_since` and
//! `model_add_blobs`), so a draft of a layout with an image lost the image, and `model.import` of
//! changes with blobs was refused. Now the replica opens as the same layout, ids included (KHP
//! A57), and edits apart from the file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use kition_layout_kit::LayoutDocument;
use serde_json::{Map, Value, json};

/// A 1x1 PNG.
const PICTURE: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The layout as an agent reads it (`document.inspect`), without what belongs to the session.
fn layout(kit: &mut Headless<LayoutDocument>, id: &str) -> Value {
    let mut inspected = kit.execute(id, "document.inspect", Map::new(), Origin::Agent).unwrap().result.unwrap_or_default();
    for key in ["path", "dirty", "selection", "tool", "title"] {
        inspected.as_object_mut().map(|object| object.remove(key));
    }
    inspected
}

fn graphics(layout: &Value) -> Vec<Value> {
    layout["spreads"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|spread| spread["items"].as_array().cloned().unwrap_or_default())
        .filter(|item| item.get("asset").is_some())
        .collect()
}

#[test]
fn a_layout_opens_from_its_exported_replica_with_its_images() {
    let root = std::env::temp_dir().join(format!("kition-layout-replica-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let mut poster = LayoutDocument::create("Poster.klayout", 612, 792).unwrap();
    poster.execute("file.place", json!({ "base64": PICTURE, "name": "dot.png", "x": 72, "y": 72, "width": 100 })).unwrap();
    std::fs::write(root.join("Poster.klayout"), poster.seal_and_save(None).unwrap()).unwrap();

    let mut kit = Headless::<LayoutDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_layout_kit::EDITOR_STATE).unwrap(),
    );
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let file = kit.open(OpenRequest { file: &folder.file("Poster.klayout").unwrap(), read_only: false, create: None }).unwrap().id;
    kit.execute(&file, "frame.create", params(json!({ "rect": [72, 300, 300, 400], "content": "text", "text": "Spring" })), Origin::Agent).unwrap();
    let before = layout(&mut kit, &file);
    assert_eq!(graphics(&before).len(), 1, "{before}");

    let exported = kit.model("model.export", json!({ "documentId": file })).unwrap();
    assert!(exported["blobs"].is_string(), "the image travels with the changes: {exported}");
    let opened = kit
        .model("model.open", json!({ "name": "Poster.klayout", "snapshot": exported["changes"], "blobs": exported["blobs"] }))
        .unwrap_or_else(|error| panic!("model.open: {}", error.message));
    let replica = opened["documentId"].as_str().unwrap().to_owned();
    assert_ne!(replica, file);
    assert_eq!(layout(&mut kit, &replica), before, "the replica is the layout, ids included");
    let again = kit.model("model.export", json!({ "documentId": replica })).unwrap();
    assert_eq!(again["blobs"], exported["blobs"], "the replica holds the image");

    // The replica takes edits of its own and leaves the file's document alone.
    kit.execute(&replica, "frame.create", params(json!({ "rect": [300, 500, 400, 600], "content": "unassigned" })), Origin::Agent).unwrap();
    assert_eq!(layout(&mut kit, &file), before);
    assert_ne!(layout(&mut kit, &replica), before);
    // What the replica made goes back to the file's document with its media.
    let since = kit.model("model.export", json!({ "documentId": replica })).unwrap();
    kit.model("model.import", json!({ "documentId": file, "changes": since["changes"], "blobs": since["blobs"] })).unwrap();
    assert_eq!(layout(&mut kit, &file), layout(&mut kit, &replica));
    let saved = kit.model("model.save", json!({ "documentId": replica })).unwrap();
    assert!(saved["data"].as_str().is_some_and(|data| !data.is_empty()), "{saved}");
    let _ = std::fs::remove_dir_all(root);
}
