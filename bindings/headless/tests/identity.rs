//! `document.identity` and `document.fork` (Kit Process Protocol section 5.6, KitModel M7a)
//! through the real `kition-layout` process: a `.klayout` holds the layout's content
//! (`content.bin`, a content snapshot: the state, no history), so a byte copy of a file is the same
//! document. A host reads the identity a file carries without opening the layout, and forks a file
//! into a new document: a new docId (in the replica and in `meta.json`'s copy), the same objects,
//! ids and values, no history, only the blobs the layout references, and everything else in the
//! file as it was.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kit_host::{ClientError, HeadlessKit, LaunchOptions};
use kition_layout_format::container;
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

const LIMIT: Duration = Duration::from_secs(60);

/// Text of a frame deleted before the fork: in neither file (a file holds no history).
const SECRET: &str = "SECRET-DELETED-BEFORE-THE-FORK";

/// A 1x1 PNG.
const PICTURE: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

/// A blob the layout does not reference (one an earlier save left, or another program wrote).
const STRAY: &[u8] = b"a blob no asset of the layout names";

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-layout-identity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

async fn launch(dir: &Path) -> HeadlessKit {
    let mut options = LaunchOptions::new(env!("CARGO_BIN_EXE_kition-layout"));
    options.workspace = Some(dir.to_path_buf());
    HeadlessKit::launch(options).await.unwrap()
}

/// A layout saved twice: first with two text frames, the secret one and a placed picture, then,
/// opened from that file, with the secret frame deleted; plus a stray blob and an entry this build
/// does not know.
fn layout() -> Vec<u8> {
    let mut doc = LayoutDocument::create("Plan.klayout", 612, 792).unwrap();
    doc.execute("frame.create", json!({ "rect": [72, 72, 300, 200], "content": "text", "text": "Kept" })).unwrap();
    doc.execute("frame.create", json!({ "rect": [72, 220, 300, 340], "content": "text", "text": "Also kept" })).unwrap();
    let secret = doc.execute("frame.create", json!({ "rect": [72, 360, 300, 480], "content": "text", "text": SECRET })).unwrap()["id"].clone();
    doc.execute("file.place", json!({ "base64": PICTURE, "name": "dot.png", "x": 320, "y": 72, "width": 100 })).unwrap();
    doc.prepare_save().unwrap();
    let first = doc.seal_and_save(None).unwrap();
    assert!(holds(&first, SECRET), "the control: the first file holds the frame's text");
    let mut doc = LayoutDocument::open("Plan.klayout", &first).unwrap();
    doc.execute("edit.clear", json!({ "ids": [secret] })).unwrap();
    doc.prepare_save().unwrap();
    let saved = doc.seal_and_save(None).unwrap();
    let mut parts = container::read(&saved).unwrap();
    let origins = kition_kit::kdm::Replica::snapshot_origins(&parts.content).unwrap();
    assert_eq!(origins, vec![Some(kition_kit::kdm::Origin::system("content-seal"))], "of the history, the file holds the seal");
    assert_eq!(parts.blobs.len(), 1, "the picture");
    parts.blobs.insert(container::sha256_hex(STRAY), STRAY.to_vec());
    parts.extra.push(("notes/kept.txt".into(), b"an entry another program wrote".to_vec()));
    container::write(&parts).unwrap()
}

/// Every object of the layout with its fields, by id: what must be equal in a fork.
async fn objects(kit: &HeadlessKit, path: &str) -> Value {
    let state = kit.open(path, false, LIMIT).await.unwrap();
    let document = state["documentId"].clone();
    let batch = |request: Value| {
        let mut request = request;
        request["documentId"] = document.clone();
        kit.request("model.batch", request, LIMIT)
    };
    let inventory = batch(json!({ "ops": [], "objects": true })).await.unwrap();
    let ids: Vec<Value> = inventory["objects"].as_array().unwrap().iter().map(|object| object["id"].clone()).collect();
    assert!(ids.len() >= 4, "the layout, a page and its frames: {inventory}");
    let load: Vec<Value> = ids.iter().map(|id| json!({ "target": id })).collect();
    let loaded = batch(json!({ "ops": [], "load": load })).await.unwrap();
    kit.request("document.close", json!({ "documentId": document }), LIMIT).await.unwrap();
    json!({ "objects": inventory["objects"], "loaded": loaded["loaded"] })
}

fn holds(bytes: &[u8], needle: &str) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle.as_bytes())
}

/// What the kit said about a refusal (`error.data.detail`).
fn detail(error: &ClientError) -> String {
    error.data().and_then(|data| data["detail"].as_str()).unwrap_or_default().to_owned()
}

fn items(doc: &LayoutDocument) -> usize {
    let inspected = doc.inspect().unwrap();
    inspected["spreads"].as_array().into_iter().flatten().map(|spread| spread["items"].as_array().map_or(0, Vec::len)).sum()
}

#[tokio::test]
async fn a_layout_tells_its_identity_and_forks_into_a_new_document_without_history() {
    let dir = workspace("embedded");
    std::fs::write(dir.join("Plan.klayout"), layout()).unwrap();
    let kit = launch(&dir).await;
    assert!(kit.serves("document.identity"), "{}", kit.capabilities());

    // The identity the file carries, read without opening it, and the same for a byte copy.
    let source_id = kit.identity("Plan.klayout", LIMIT).await.unwrap().doc_id.expect("a layout file has an identity");
    let meta = container::read(&std::fs::read(dir.join("Plan.klayout")).unwrap()).unwrap().meta;
    assert_eq!(meta.doc_id, source_id, "meta.json's copy names the same document");
    std::fs::copy(dir.join("Plan.klayout"), dir.join("Copy.klayout")).unwrap();
    assert_eq!(kit.identity("Copy.klayout", LIMIT).await.unwrap().doc_id.as_deref(), Some(source_id.as_str()), "a byte copy is the same document");

    // A fork is a new document with the same objects, ids and values.
    let fork = kit.fork("Plan.klayout", "Fork.klayout", LIMIT).await.unwrap();
    let fork_id = fork.doc_id.clone().expect("the fork's identity");
    assert_ne!(fork_id, source_id);
    assert_eq!(fork.name, "Fork.klayout");
    assert!(!fork.version.is_empty());
    assert_eq!(kit.identity("Fork.klayout", LIMIT).await.unwrap().doc_id.as_deref(), Some(fork_id.as_str()), "the forked file carries the new id");
    assert_eq!(kit.identity("Plan.klayout", LIMIT).await.unwrap().doc_id.as_deref(), Some(source_id.as_str()), "the source is unchanged");
    assert_eq!(objects(&kit, "Fork.klayout").await, objects(&kit, "Plan.klayout").await, "the same objects, ids and values");

    // The fork's replica travels as a whole (an agent's draft of it: `model.export`, then
    // `model.open`), though it starts at the fork and has no earlier changes.
    let state = kit.open("Fork.klayout", true, LIMIT).await.unwrap();
    let exported = kit.request("model.export", json!({ "documentId": state["documentId"] }), LIMIT).await.unwrap();
    let draft = kit
        .request("model.open", json!({ "name": "Fork.klayout", "snapshot": exported["changes"], "blobs": exported["blobs"] }), LIMIT)
        .await
        .unwrap();
    assert_ne!(draft["documentId"], state["documentId"], "{draft}");
    kit.request("document.close", json!({ "documentId": state["documentId"] }), LIMIT).await.unwrap();

    // The fork's file: the new id in meta.json too, of the history only its birth, only the
    // referenced blobs, everything else kept.
    let (source_bytes, fork_bytes) = (std::fs::read(dir.join("Plan.klayout")).unwrap(), std::fs::read(dir.join("Fork.klayout")).unwrap());
    let (source, forked) = (container::read(&source_bytes).unwrap(), container::read(&fork_bytes).unwrap());
    assert_eq!(forked.meta.doc_id, fork_id, "meta.json's copy of the docId is the fork's");
    assert_eq!(kition_kit::kdm::Replica::snapshot_origins(&forked.content).unwrap().len(), 1, "a fork holds one change, its birth");
    assert_eq!(forked.meta.kit, source.meta.kit);
    let stray = container::sha256_hex(STRAY);
    assert!(source.blobs.contains_key(&stray), "the source keeps what it held");
    let mut referenced = source.blobs.clone();
    referenced.remove(&stray);
    assert_eq!(forked.blobs, referenced, "the fork holds the picture and no blob its layout does not reference");
    assert_eq!(forked.extra, source.extra, "entries the layout does not know are kept");

    // The fork edits, saves and opens again as any layout, under its own id.
    let mut opened = LayoutDocument::open("Fork.klayout", &fork_bytes).unwrap();
    assert_eq!(opened.doc_id().as_deref(), Some(fork_id.as_str()));
    opened.execute("frame.create", json!({ "rect": [72, 500, 300, 600], "content": "text", "text": "New" })).unwrap();
    opened.prepare_save().unwrap();
    let edited = opened.seal_and_save(None).unwrap();
    let reopened = LayoutDocument::open("Fork.klayout", &edited).unwrap();
    assert_eq!(reopened.doc_id().as_deref(), Some(fork_id.as_str()));
    assert_eq!(items(&reopened), items(&opened));

    // No history: the deleted frame's text is in neither file.
    assert!(!holds(&source_bytes, SECRET), "the source's file holds no deleted content");
    assert!(!holds(&fork_bytes, SECRET), "deleted content does not travel");

    // A fork of a fork is a third document with the same content.
    let again = kit.fork("Fork.klayout", "Again.klayout", LIMIT).await.unwrap().doc_id.expect("the second fork's identity");
    assert!(again != source_id && again != fork_id, "{again}");
    assert_eq!(objects(&kit, "Again.klayout").await, objects(&kit, "Plan.klayout").await);

    // A fork never replaces a file.
    let taken = kit.fork("Plan.klayout", "Fork.klayout", LIMIT).await.unwrap_err();
    assert_eq!(taken.reason(), Some("file_conflict"), "{taken}");
    assert_eq!(std::fs::read(dir.join("Fork.klayout")).unwrap(), fork_bytes, "the first fork is kept");
    kit.shutdown(Duration::from_secs(5)).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn files_without_a_replica_have_no_identity_and_bytes_that_are_no_layout_are_refused() {
    let dir = workspace("plain");
    let kit = launch(&dir).await;
    // An empty `.klayout` (a file made for the kit before it saved one) holds no replica.
    std::fs::write(dir.join("New.klayout"), b"").unwrap();
    assert_eq!(kit.identity("New.klayout", LIMIT).await.unwrap().doc_id, None);
    let fork = kit.fork("New.klayout", "NewCopy.klayout", LIMIT).await.unwrap();
    assert_eq!(fork.doc_id, None);
    assert_eq!(std::fs::read(dir.join("NewCopy.klayout")).unwrap(), b"", "a file without a replica forks unchanged");

    // An IDML package is another program's file: it opens as a new layout each time and carries
    // no identity; its fork is the same bytes.
    let mut doc = LayoutDocument::create("Plan.klayout", 612, 792).unwrap();
    doc.execute("frame.create", json!({ "rect": [72, 72, 300, 200], "content": "text", "text": "Imported" })).unwrap();
    let exported = doc.execute("file.exportBytes", json!({ "format": "idml" })).unwrap();
    let idml = kition_layout_engine::cmd::base64_decode(exported["data"].as_str().unwrap());
    assert!(LayoutDocument::open("Plan.idml", &idml).is_ok(), "the package opens");
    std::fs::write(dir.join("Plan.idml"), &idml).unwrap();
    assert_eq!(kit.identity("Plan.idml", LIMIT).await.unwrap().doc_id, None);
    let fork = kit.fork("Plan.idml", "PlanCopy.idml", LIMIT).await.unwrap();
    assert_eq!(fork.doc_id, None);
    assert_eq!(std::fs::read(dir.join("PlanCopy.idml")).unwrap(), idml, "a package forks unchanged");

    // Bytes that are not a layout are refused, never answered with no identity.
    std::fs::write(dir.join("Text.klayout"), b"words a person wrote elsewhere").unwrap();
    for refused in [kit.identity("Text.klayout", LIMIT).await.unwrap_err(), kit.fork("Text.klayout", "TextCopy.klayout", LIMIT).await.unwrap_err()] {
        assert_eq!(refused.reason(), Some("unsupported_format"), "{refused}: {}", detail(&refused));
    }
    assert!(!dir.join("TextCopy.klayout").exists());
    kit.shutdown(Duration::from_secs(5)).await;
    let _ = std::fs::remove_dir_all(dir);
}
