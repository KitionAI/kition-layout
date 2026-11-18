//! The `.klayout` file (design note section 3.1–3.2, the board note's 2.1–2.4 with `kition.layout`):
//! a ZIP whose entries are, in this order,
//!
//! | Entry | Compression | Content |
//! | --- | --- | --- |
//! | `mimetype` | STORE, first, no extra field | the ASCII bytes `application/vnd.kition.layout+zip` |
//! | `meta.json` | DEFLATE | [`Meta`] |
//! | `snapshot.bin` | STORE | a KDM snapshot envelope of the document at the last compaction |
//! | `changes/<seq>.bin` | STORE | KDM change envelopes, in replay order (`000001`, `000002`, …) |
//! | `blobs/<sha256>` | STORE | one blob's bytes, named by the lower-case hex SHA-256 its KDM value carries |
//!
//! Any other entry with a safe name is kept byte for byte and written back ([`Container::extra`]).
//! This module knows bytes only: what the snapshot and changes hold is the KDM's business
//! (`kition-layout-kdm` opens them).
//!
//! The reader refuses, opening nothing and never panicking: a missing or wrong `mimetype` first
//! entry ([`ContainerError::NotALayout`]); a missing `meta.json` or `snapshot.bin`, a listed change
//! entry that is missing, an entry name with `..`, `\`, a leading `/` or a drive letter, duplicate
//! names, encrypted entries, ZIP64, more than [`MAX_ENTRIES`] entries, an entry over
//! [`MAX_ENTRY_BYTES`] or a whole archive over [`MAX_TOTAL_BYTES`] uncompressed, a `meta.json`
//! deflated more than [`MAX_META_RATIO`]:1, and a blob whose bytes do not hash to its name
//! ([`ContainerError::Damaged`]). A newer `formatVersion` is [`ContainerError::TooNew`].

use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read, Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::MIME;

/// `meta.json`'s `format`.
pub const FORMAT: &str = "kition.layout";
/// The container version this build writes and reads.
pub const FORMAT_VERSION: u32 = 1;
/// More entries than this and the file is refused.
pub const MAX_ENTRIES: usize = 100_000;
/// The largest uncompressed entry.
pub const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
/// The largest uncompressed archive.
pub const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// The largest deflate ratio `meta.json` may have.
pub const MAX_META_RATIO: u64 = 100;

const SNAPSHOT: &str = "snapshot.bin";
const META: &str = "meta.json";

/// Why a file is not a readable `.klayout`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContainerError {
    /// Not a Kition Layout file at all (no `mimetype` first entry naming the layout MIME).
    #[error("not-a-layout: {0}")]
    NotALayout(String),
    /// A Kition Layout file that is broken or breaks a limit.
    #[error("damaged: {0}")]
    Damaged(String),
    /// Written by a newer Kition Layout.
    #[error("the file is from a newer Kition Layout (format {0}); please update")]
    TooNew(u32),
}

/// `meta.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub format: String,
    pub format_version: u32,
    /// The KDM version, a copy for hosts that do not load the KDM.
    pub kdm: u32,
    /// The `kit.layout` schema version, a copy.
    pub schema_version: u32,
    /// The KDM document id, a copy (so a host can index the file without the engine).
    pub doc_id: String,
    /// The document's title, a copy for file lists.
    #[serde(default)]
    pub title: String,
    pub snapshot: SnapshotRef,
    /// The change entries in replay order.
    #[serde(default)]
    pub changes: Vec<String>,
    #[serde(default)]
    pub app: App,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SnapshotRef {
    pub entry: String,
    /// Base64 of the KDM version the snapshot holds.
    #[serde(default)]
    pub version: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct App {
    pub kit: String,
    #[serde(default)]
    pub version: String,
}

/// The parts of a `.klayout` file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Container {
    pub meta: Meta,
    pub snapshot: Vec<u8>,
    /// The listed change entries' bytes, in replay order.
    pub changes: Vec<Vec<u8>>,
    /// Blob bytes by their SHA-256 (lower-case hex).
    pub blobs: BTreeMap<String, Vec<u8>>,
    /// Entries this build does not know, by name, kept byte for byte.
    pub extra: Vec<(String, Vec<u8>)>,
}

/// SHA-256 as lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The name of change entry `seq` (from 1).
pub fn change_entry(seq: usize) -> String {
    format!("changes/{seq:06}.bin")
}

/// Whether `bytes` start like a `.klayout` container: a ZIP whose first local entry is a stored
/// `mimetype` holding the layout MIME (the ODF/EPUB check, without unzipping).
pub fn is_container(bytes: &[u8]) -> bool {
    first_entry_is_mimetype(bytes).is_ok()
}

fn first_entry_is_mimetype(bytes: &[u8]) -> Result<(), String> {
    let at = |range: std::ops::Range<usize>| bytes.get(range).ok_or_else(|| "too short".to_owned());
    if at(0..4)? != b"PK\x03\x04" {
        return Err("not a ZIP archive".into());
    }
    let u16_at = |i: usize| -> Result<u16, String> { at(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]])) };
    let u32_at = |i: usize| -> Result<u32, String> { at(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) };
    if u16_at(8)? != 0 {
        return Err("the mimetype entry is compressed".into());
    }
    let (name_len, extra_len) = (usize::from(u16_at(26)?), usize::from(u16_at(28)?));
    if at(30..30 + name_len)? != b"mimetype" {
        return Err("the first entry is not mimetype".into());
    }
    if extra_len != 0 {
        return Err("the mimetype entry has an extra field".into());
    }
    let size = usize::try_from(u32_at(18)?).map_err(|_| "size".to_owned())?;
    let start = 30 + name_len;
    if at(start..start + size)? != MIME.as_bytes() {
        return Err("the mimetype is not a layout's".into());
    }
    Ok(())
}

/// Whether `name` is a safe entry name: relative, no `..` component, no backslash, no drive.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.split('/').any(|part| part == ".." || part == ".")
        && name.as_bytes().get(1).is_none_or(|b| *b != b':')
        && !name.contains('\0')
}

fn damaged(message: impl Into<String>) -> ContainerError {
    ContainerError::Damaged(message.into())
}

/// Reads a `.klayout` file's parts. Never writes, never panics on bad input.
pub fn read(bytes: &[u8]) -> Result<Container, ContainerError> {
    first_entry_is_mimetype(bytes).map_err(ContainerError::NotALayout)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| damaged(error.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(damaged(format!("{} entries (at most {MAX_ENTRIES})", zip.len())));
    }
    if zip.zip64_comment().is_some() {
        return Err(damaged("ZIP64 archives are not read"));
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(zip.len());
    for index in 0..zip.len() {
        let file = zip.by_index_raw(index).map_err(|error| damaged(error.to_string()))?;
        let name = file.name().to_owned();
        if !safe_name(&name) {
            return Err(damaged(format!("unsafe entry name {name:?}")));
        }
        if !names.insert(name.clone()) {
            return Err(damaged(format!("{name} appears twice")));
        }
        if file.encrypted() {
            return Err(damaged(format!("{name} is encrypted")));
        }
        let size = file.size();
        if size > MAX_ENTRY_BYTES || file.compressed_size() > MAX_ENTRY_BYTES || size > u64::from(u32::MAX) {
            return Err(damaged(format!("{name} is too large")));
        }
        total = total.saturating_add(size);
        if total > MAX_TOTAL_BYTES {
            return Err(damaged("the archive is too large"));
        }
        if name == META && file.compressed_size() > 0 && size / file.compressed_size().max(1) > MAX_META_RATIO {
            return Err(damaged("meta.json is compressed too much"));
        }
        if file.is_dir() {
            continue;
        }
        drop(file);
        let mut file = zip.by_index(index).map_err(|error| damaged(format!("{name}: {error}")))?;
        let mut data = Vec::with_capacity(usize::try_from(size).unwrap_or(0).min(1 << 24));
        // `take` guards against a header that lies about the size.
        (&mut file).take(MAX_ENTRY_BYTES + 1).read_to_end(&mut data).map_err(|error| damaged(format!("{name}: {error}")))?;
        if data.len() as u64 != size {
            return Err(damaged(format!("{name}: the size does not match its header")));
        }
        entries.push((name, data));
    }
    let mut meta_bytes = None;
    let mut snapshot = None;
    let mut by_name: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    let mut extra = Vec::new();
    for (name, data) in entries {
        if name == "mimetype" {
            continue;
        }
        if name == META {
            meta_bytes = Some(data);
        } else if name == SNAPSHOT {
            snapshot = Some(data);
        } else if let Some(hash) = name.strip_prefix("blobs/") {
            if sha256_hex(&data) != hash {
                return Err(damaged(format!("{name}: the bytes do not match their hash")));
            }
            blobs.insert(hash.to_owned(), data);
        } else if name.starts_with("changes/") {
            by_name.insert(name, data);
        } else {
            extra.push((name, data));
        }
    }
    let meta_bytes = meta_bytes.ok_or_else(|| damaged("no meta.json"))?;
    let raw: serde_json::Value = serde_json::from_slice(&meta_bytes).map_err(|error| damaged(format!("meta.json: {error}")))?;
    if raw.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(ContainerError::NotALayout("meta.json does not name kition.layout".into()));
    }
    if let Some(version) = raw.get("formatVersion").and_then(serde_json::Value::as_u64)
        && version > u64::from(FORMAT_VERSION)
    {
        return Err(ContainerError::TooNew(u32::try_from(version).unwrap_or(u32::MAX)));
    }
    let meta: Meta = serde_json::from_value(raw).map_err(|error| damaged(format!("meta.json: {error}")))?;
    if meta.snapshot.entry != SNAPSHOT {
        return Err(damaged(format!("the snapshot entry is {}, not {SNAPSHOT}", meta.snapshot.entry)));
    }
    let snapshot = snapshot.ok_or_else(|| damaged("no snapshot.bin"))?;
    let mut changes = Vec::with_capacity(meta.changes.len());
    for name in &meta.changes {
        // An entry on disk but not listed is ignored (and dropped at the next save); a listed one
        // that is missing makes the file damaged.
        changes.push(by_name.remove(name).ok_or_else(|| damaged(format!("{name} is listed but missing")))?);
    }
    Ok(Container { meta, snapshot, changes, blobs, extra })
}

/// Writes a `.klayout` file: the entries in the order of the module docs. `meta.changes` and
/// `meta.snapshot.entry` are set from [`Container::changes`] here.
pub fn write(container: &Container) -> Result<Vec<u8>, ContainerError> {
    let io = |error: std::io::Error| damaged(error.to_string());
    let zip_io = |error: zip::result::ZipError| damaged(error.to_string());
    let mut meta = container.meta.clone();
    meta.format = FORMAT.to_owned();
    meta.format_version = FORMAT_VERSION;
    meta.snapshot.entry = SNAPSHOT.to_owned();
    meta.changes = (1..=container.changes.len()).map(change_entry).collect();
    let mut out = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("mimetype", stored).map_err(zip_io)?;
        zip.write_all(MIME.as_bytes()).map_err(io)?;
        zip.start_file(META, deflate).map_err(zip_io)?;
        let json = serde_json::to_vec_pretty(&meta).map_err(|error| damaged(error.to_string()))?;
        zip.write_all(&json).map_err(io)?;
        zip.start_file(SNAPSHOT, stored).map_err(zip_io)?;
        zip.write_all(&container.snapshot).map_err(io)?;
        for (name, bytes) in meta.changes.iter().zip(&container.changes) {
            zip.start_file(name.as_str(), stored).map_err(zip_io)?;
            zip.write_all(bytes).map_err(io)?;
        }
        for (hash, bytes) in &container.blobs {
            zip.start_file(format!("blobs/{hash}"), stored).map_err(zip_io)?;
            zip.write_all(bytes).map_err(io)?;
        }
        let known = |name: &str| name == "mimetype" || name == META || name == SNAPSHOT || name.starts_with("changes/") || name.starts_with("blobs/");
        for (name, bytes) in &container.extra {
            if !safe_name(name) || known(name) {
                continue;
            }
            zip.start_file(name.as_str(), deflate).map_err(zip_io)?;
            zip.write_all(bytes).map_err(io)?;
        }
        zip.finish().map_err(zip_io)?;
    }
    Ok(out.into_inner())
}

/// Whether a save should write a fresh snapshot instead of appending: the change entries
/// together exceed a quarter of the snapshot, or there are more than 200 of them (the board's
/// proposed thresholds).
pub fn wants_compaction(snapshot_len: usize, changes: &[Vec<u8>]) -> bool {
    let total: usize = changes.iter().map(Vec::len).sum();
    changes.len() > 200 || total.saturating_mul(4) > snapshot_len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Container {
        let blob = b"\x89PNG fake".to_vec();
        Container {
            meta: Meta {
                kdm: 1,
                schema_version: 1,
                doc_id: "doc-1".into(),
                title: "Flyer".into(),
                app: App { kit: "kit.layout".into(), version: "0.0.0".into() },
                ..Default::default()
            },
            snapshot: b"snapshot bytes".to_vec(),
            changes: vec![b"one".to_vec(), b"two".to_vec()],
            blobs: BTreeMap::from([(sha256_hex(&blob), blob)]),
            extra: vec![("later/thing.json".into(), b"{}".to_vec())],
        }
    }

    #[test]
    fn round_trips_every_part_with_mimetype_first_and_stored() {
        let bytes = write(&sample()).unwrap();
        assert!(is_container(&bytes));
        assert_eq!(&bytes[30..38], b"mimetype");
        assert_eq!(&bytes[38..38 + MIME.len()], MIME.as_bytes());
        let back = read(&bytes).unwrap();
        assert_eq!(back.snapshot, b"snapshot bytes");
        assert_eq!(back.changes, vec![b"one".to_vec(), b"two".to_vec()]);
        assert_eq!(back.meta.changes, vec!["changes/000001.bin", "changes/000002.bin"]);
        assert_eq!(back.meta.format, FORMAT);
        assert_eq!(back.blobs, sample().blobs);
        assert_eq!(back.extra, sample().extra, "an unknown entry survives");
        assert_eq!(write(&back).unwrap(), bytes, "writing what was read gives the same bytes");
    }

    /// A ZIP built entry by entry, for damaged files.
    fn zip_of(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut out);
            for (name, bytes, deflate) in entries {
                let method = if *deflate { zip::CompressionMethod::Deflated } else { zip::CompressionMethod::Stored };
                zip.start_file(*name, zip::write::SimpleFileOptions::default().compression_method(method)).unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        out.into_inner()
    }

    fn meta_json(changes: &[&str]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "format": FORMAT, "formatVersion": 1, "kdm": 1, "schemaVersion": 1, "docId": "d",
            "snapshot": { "entry": "snapshot.bin", "version": "" }, "changes": changes
        }))
        .unwrap()
    }

    #[test]
    fn refuses_damaged_and_foreign_files() {
        let meta = meta_json(&[]);
        let mime = MIME.as_bytes();
        let not = |bytes: &[u8]| matches!(read(bytes), Err(ContainerError::NotALayout(_)));
        let bad = |bytes: &[u8]| matches!(read(bytes), Err(ContainerError::Damaged(_)));
        assert!(not(b"not a zip"));
        assert!(not(&zip_of(&[("meta.json", &meta, true), ("mimetype", mime, false)])), "mimetype not first");
        assert!(not(&zip_of(&[("mimetype", mime, true), ("meta.json", &meta, true)])), "mimetype compressed");
        assert!(not(&zip_of(&[("mimetype", b"application/zip", false)])), "another MIME");
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("snapshot.bin", b"s", false)])), "no meta.json");
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true)])), "no snapshot");
        let listed = meta_json(&["changes/000001.bin"]);
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &listed, true), ("snapshot.bin", b"s", false)])), "listed change missing");
        for name in ["../evil", "/abs", "a\\b", "C:/x", "a/../b"] {
            assert!(
                bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true), ("snapshot.bin", b"s", false), (name, b"x", false)])),
                "{name}"
            );
        }
        let wrong_hash = format!("blobs/{}", sha256_hex(b"other"));
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true), ("snapshot.bin", b"s", false), (&wrong_hash, b"x", false)])));
        let bomb = serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "formatVersion": 1, "kdm": 1, "schemaVersion": 1, "docId": "d", "snapshot": { "entry": "snapshot.bin" }, "pad": " ".repeat(200_000) })).unwrap();
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &bomb, true), ("snapshot.bin", b"s", false)])), "deflate bomb");
        let newer =
            serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "formatVersion": 9, "snapshot": { "entry": "snapshot.bin" } })).unwrap();
        assert_eq!(
            read(&zip_of(&[("mimetype", mime, false), ("meta.json", &newer, true), ("snapshot.bin", b"s", false)])),
            Err(ContainerError::TooNew(9))
        );
        let other = serde_json::to_vec(&serde_json::json!({ "format": "klayout", "version": 1 })).unwrap();
        assert!(not(&zip_of(&[("mimetype", mime, false), ("meta.json", &other, true), ("document.json", b"{}", true)])), "the engine's own snapshot");
        // A change entry on disk that meta.json does not list is ignored.
        let ok = read(&zip_of(&[
            ("mimetype", mime, false),
            ("meta.json", &meta, true),
            ("snapshot.bin", b"s", false),
            ("changes/000009.bin", b"c", false),
        ]))
        .unwrap();
        assert!(ok.changes.is_empty());
    }

    #[test]
    fn compaction_follows_the_thresholds() {
        assert!(!wants_compaction(1000, &[vec![0; 100]]));
        assert!(wants_compaction(1000, &[vec![0; 251]]));
        assert!(wants_compaction(1_000_000, &vec![vec![0; 1]; 201]));
    }
}
