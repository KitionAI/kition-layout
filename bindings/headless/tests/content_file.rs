//! The headless `kition-layout` saves on the direct path (feature `layout.file.content-only`): the
//! host seals the document (`prepare_save`) before every save, so the file an agent's batches write
//! holds the layout's content and, of its history, only that seal; a value an agent wrote and then
//! overwrote is in no file, and the document opened from such a file takes later batches.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit::kdm::Replica;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use kition_layout_kit::LayoutDocument;
use serde_json::{Map, Value, json};

/// Written once, overwritten at once: in no file.
const OVERWRITTEN: &str = "Ug5Le1Fo0Dw4";

fn origins(file: &[u8]) -> Vec<Option<kition_kit::kdm::Origin>> {
    Replica::snapshot_origins(&kition_layout_format::container::read(file).unwrap().content).unwrap()
}

fn seal() -> Vec<Option<kition_kit::kdm::Origin>> {
    vec![Some(kition_kit::kdm::Origin::system("content-seal"))]
}

fn title(kit: &mut Headless<LayoutDocument>, id: &str, value: &str) -> Value {
    kit.model("model.batch", json!({ "documentId": id, "ops": [{ "op": "set", "target": "layout", "field": "title", "value": value }] }))
        .unwrap_or_else(|error| panic!("model.batch: {}", error.message))
}

#[test]
fn an_agents_batches_save_sealed_content() {
    let root = std::env::temp_dir().join(format!("kition-layout-content-file-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let mut poster = LayoutDocument::create("Poster.klayout", 612, 792).unwrap();
    poster.execute("frame.create", json!({ "rect": [72, 72, 300, 200], "content": "text", "text": "Spring" })).unwrap();
    poster.prepare_save().unwrap();
    std::fs::write(root.join("Poster.klayout"), poster.seal_and_save(None).unwrap()).unwrap();
    assert_eq!(origins(&std::fs::read(root.join("Poster.klayout")).unwrap()), seal(), "the fixture's file");

    let mut kit = Headless::<LayoutDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_layout_kit::EDITOR_STATE).unwrap(),
    );
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let id = kit.open(OpenRequest { file: &folder.file("Poster.klayout").unwrap(), read_only: false, create: None }).unwrap().id;
    title(&mut kit, &id, OVERWRITTEN);
    title(&mut kit, &id, "Final");
    kit.execute(
        &id,
        "frame.create",
        Map::from_iter([("rect".into(), json!([72, 300, 300, 400])), ("content".into(), json!("unassigned"))]),
        Origin::Agent,
    )
    .unwrap();
    kit.save(&id, None, None).unwrap();
    let file = std::fs::read(root.join("Poster.klayout")).unwrap();
    assert_eq!(origins(&file), seal(), "of the history, the file holds the seal");
    assert!(!file.windows(OVERWRITTEN.len()).any(|window| window == OVERWRITTEN.as_bytes()), "the overwritten value is in no file");
    let reopened = LayoutDocument::open("Poster.klayout", &file).unwrap();
    assert_eq!(reopened.inspect().unwrap()["title"], "Final");

    // A later batch on the same document, saved again: still the content and one seal.
    title(&mut kit, &id, "Later");
    kit.save(&id, None, None).unwrap();
    let again = std::fs::read(root.join("Poster.klayout")).unwrap();
    assert_eq!(origins(&again), seal());
    assert_eq!(LayoutDocument::open("Poster.klayout", &again).unwrap().inspect().unwrap()["title"], "Later");
    let _ = std::fs::remove_dir_all(root);
}
