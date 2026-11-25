//! Every committed `.klayout` sample (`conformance/`, `cases/`, `web/public/`) loads and saves to its
//! own bytes (kition-sdk data model 7.4; feature `layout.file.content-only`): its content is the
//! canonical `content.json` the kit writes, so the samples are the kit's own files. Loaded and
//! saved through the kit's model (`KitModel::load`, `KitModel::export`, the bytes `save_file`
//! writes) without the repair on open, so a sample of a broken layout stays as it is.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kition_layout_format::container;
use kition_layout_kdm::kition_kit::kdm::{ActorId, Replica};
use kition_layout_kdm::kition_kit::session::{KitModel, MemoryBlobs};
use kition_layout_kdm::{LayoutModel, schema};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The committed samples, by their path from the repository root, sorted.
fn samples() -> Vec<String> {
    let root = repo();
    let mut found = Vec::new();
    let mut stack: Vec<PathBuf> = ["conformance", "cases", "web/public"].iter().map(|dir| root.join(dir)).collect();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "klayout") {
                found.push(path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// The file the kit saves for `content` (a `content.json`), with `blobs`.
fn kit_save(content: &[u8], blobs: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let replica = Replica::load(schema(), content, ActorId::random()).unwrap();
    let blobs = MemoryBlobs(blobs.iter().map(|(hash, bytes)| (hash.clone(), Arc::from(bytes.as_slice()))).collect());
    let model = LayoutModel::load(&replica, &blobs).unwrap();
    model.export(&replica, "Sample.klayout", &blobs).unwrap()
}

#[test]
fn every_committed_sample_loads_and_saves_to_its_own_bytes() {
    let samples = samples();
    assert!(samples.len() >= 19, "the samples: {samples:?}");
    for sample in samples {
        let bytes = std::fs::read(repo().join(&sample)).unwrap();
        let parts = container::read(&bytes).unwrap_or_else(|error| panic!("{sample}: {error}"));
        assert!(parts.extra.is_empty(), "{sample}: a sample holds no entries the kit does not write");
        assert_eq!(kit_save(&parts.content, &parts.blobs), bytes, "{sample} saves to its own bytes");
    }
}
