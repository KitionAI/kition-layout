//! The agent's preview (`layout_inspect` with `preview: true`) through the kit's render command
//! (`kit.json` `agent.render`, Kition platform rule K21): the host runs it as the agent on the
//! document it opened, with `scale`, `maxSize` and a diagnostic's `target` as they come, and reads
//! a top-level base64 `png`. Nothing is saved.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use kition_layout_kit::LayoutDocument;
use serde_json::{Map, Value, json};

const MANIFEST: &str = include_str!("../../../kit.json");

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

#[test]
fn the_host_previews_a_target_through_the_render_command() {
    let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
    let command = manifest["agent"]["render"].as_str().unwrap();
    let root = std::env::temp_dir().join(format!("kition-layout-render-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let fixture = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/check-overset.klayout"));
    std::fs::copy(fixture, root.join("programme.klayout")).unwrap();
    let before = std::fs::read(root.join("programme.klayout")).unwrap();
    let mut kit = Headless::<LayoutDocument>::with_replica_port(KitInfo::from_manifest(MANIFEST, &[], kition_layout_kit::EDITOR_STATE).unwrap());
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let id = kit.open(OpenRequest { file: &folder.file("programme.klayout").unwrap(), read_only: false, create: None }).unwrap().id;
    for asked in [
        json!({ "scale": 1 }),
        json!({ "scale": 1, "maxSize": 512 }),
        json!({ "scale": 2, "slide": 0 }),
        json!({ "scale": 1, "target": { "path": "item:10", "page": 1 } }),
    ] {
        let executed = kit.execute(&id, command, params(asked.clone()), Origin::Agent).unwrap_or_else(|error| panic!("{asked}: {error:?}"));
        let png = executed.result.as_ref().and_then(|result| result["png"].as_str()).unwrap_or_else(|| panic!("{asked}: {executed:?}"));
        assert!(png.starts_with("iVBORw0KGgo"), "{asked}: a base64 PNG");
        assert!(!executed.changed, "{asked}: a preview is never an edit");
    }
    assert_eq!(std::fs::read(root.join("programme.klayout")).unwrap(), before, "nothing is saved");
    let _ = std::fs::remove_dir_all(root);
}
