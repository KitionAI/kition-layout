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

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(ALPHABET[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// TEMPORARY (plain model sample conversion, deleted in the next commit): saves each converted
/// `content.json` (`tests/regen/<path with / as __>.json`, written by kition-sdk's
/// `tools/convert-content`, with the blobs in `tests/regen/blobs/`) through the kit and prints the
/// file as base64, then fails so CI shows the output.
#[test]
fn regenerate_samples() {
    let regen = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/regen");
    let mut blobs = BTreeMap::new();
    for entry in std::fs::read_dir(regen.join("blobs")).unwrap() {
        let path = entry.unwrap().path();
        blobs.insert(path.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&path).unwrap());
    }
    let mut printed = 0;
    for sample in samples() {
        let content = std::fs::read(regen.join(format!("{}.json", sample.replace('/', "__")))).unwrap();
        let bytes = kit_save(&content, &blobs);
        println!("REGEN {sample} {}", base64(&bytes));
        printed += 1;
    }
    panic!("REGEN printed {printed} samples");
}
