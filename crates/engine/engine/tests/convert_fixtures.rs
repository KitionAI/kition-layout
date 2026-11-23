//! TEMPORARY (task schema-freeze, history separation): rewrites the repository's `.klayout`
//! fixtures, written with `snapshot.bin` and a change log, as content files (`content.bin`, the
//! replica's content snapshot; the same docId, object ids and blobs) and prints each one base64
//! for the commit that replaces them. Delete with that commit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::io::Read;
use std::path::Path;

use kition_layout_format::container::{self, Container, Meta};
use kition_layout_kdm::kition_kit::kdm::{ActorId, KDM_VERSION, Replica};

const FILES: &[&str] = &[
    "cases/layout.agent.act/retitle/input/poster.klayout",
    "cases/layout.agent.converse/follow-up/input/poster.klayout",
    "cases/layout.agent.observe/headline-details/input/poster.klayout",
    "cases/layout.agent.recover/overset-note/input/programme.klayout",
    "conformance/basic.klayout",
    "conformance/check-empty-frame.klayout",
    "conformance/check-low-resolution.klayout",
    "conformance/check-missing-font.klayout",
    "conformance/check-missing-glyphs.klayout",
    "conformance/check-missing-link.klayout",
    "conformance/check-overset.klayout",
    "conformance/check-rgb-in-print.klayout",
    "conformance/check-unsupported-typography.klayout",
    "conformance/overflow.klayout",
    "web/public/sample/Sample.klayout",
    "web/public/templates/editorial.klayout",
    "web/public/templates/event.klayout",
    "web/public/templates/quote.klayout",
    "web/public/templates/sale.klayout",
];

/// An old file (`meta.json` listing `snapshot.bin` and `changes/*`, `blobs/`, other entries) as a
/// content file of the same document.
fn convert(old: &[u8]) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(old)).unwrap();
    let mut entries = Vec::new();
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).unwrap();
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_owned();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        entries.push((name, data));
    }
    let get = |name: &str| entries.iter().find(|(entry, _)| entry == name).map(|(_, data)| data.clone());
    let meta: serde_json::Value = serde_json::from_slice(&get("meta.json").unwrap()).unwrap();
    let mut replica = Replica::load(kition_layout_kdm::schema(), &get("snapshot.bin").unwrap(), ActorId::random()).unwrap();
    for change in meta["changes"].as_array().into_iter().flatten() {
        replica.import_changes(&get(change.as_str().unwrap()).unwrap()).unwrap();
    }
    assert_eq!(meta["docId"].as_str().unwrap(), replica.doc_id().to_string());
    let blobs = entries.iter().filter_map(|(name, data)| name.strip_prefix("blobs/").map(|hash| (hash.to_owned(), data.clone()))).collect();
    let extra = entries
        .iter()
        .filter(|(name, _)| {
            !matches!(name.as_str(), "mimetype" | "meta.json" | "snapshot.bin") && !name.starts_with("changes/") && !name.starts_with("blobs/")
        })
        .cloned()
        .collect();
    let parts = Container {
        meta: Meta {
            kit: "kit.layout".to_owned(),
            kdm: KDM_VERSION,
            schema_version: replica.schema().schema_version,
            doc_id: replica.doc_id().to_string(),
            ..Default::default()
        },
        content: replica.content_snapshot().unwrap(),
        blobs,
        extra,
    };
    container::write(&parts).unwrap()
}

#[test]
fn print_converted_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for name in FILES {
        let old = std::fs::read(root.join(name)).unwrap();
        let new = convert(&old);
        let (opened, _) = kition_layout_kdm::open_file(&new).unwrap();
        assert!(!opened.document().spreads.is_empty(), "{name} opens");
        println!("FIXTURE {name} {} {} {}", old.len(), new.len(), kition_layout_engine::cmd::base64_encode(&new));
    }
    panic!("printed the fixtures");
}
