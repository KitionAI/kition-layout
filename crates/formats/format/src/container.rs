//! The `.klayout` file (design note section 3.1–3.2, the board note's 2.1–2.4 with `kition.layout`):
//! a ZIP whose entries are, in this order,
//!
//! | Entry | Compression | Content |
//! | --- | --- | --- |
//! | `mimetype` | STORE, first, no extra field | the ASCII bytes `application/vnd.kition.layout+zip` |
//! | `meta.json` | DEFLATE | [`Meta`] |
//! | `content.json` | DEFLATE (level 6) | the document's content: the KDM content encoding (kition-sdk data model 7.4), its state and nothing else |
//! | `blobs/<sha256>` | STORE | one blob's bytes, named by the lower-case hex SHA-256 its KDM value carries |
//!
//! The file holds no history (kition-sdk data model 7.3, rule M16b): no change log, no deleted
//! content, no person's or agent's origin. The history lives in the host's history store.
//! Any other entry with a safe name is kept byte for byte and written back ([`Container::extra`]).
//! This module knows bytes only: what `content.json` holds is the KDM's business
//! (`kition-layout-kdm` opens it).
//!
//! The reader refuses, opening nothing and never panicking: a missing or wrong `mimetype` first
//! entry ([`ContainerError::NotALayout`]); a missing `meta.json` or `content.json`, an entry name
//! with `..`, `\`, a leading `/` or a drive letter, duplicate
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

const CONTENT: &str = "content.json";
/// The DEFLATE level of `content.json`: fixed, so the same content is the same bytes for one build.
const CONTENT_LEVEL: i64 = 6;
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

/// `meta.json`: what identifies and describes the content, nothing that changes with a save (no
/// time, no counter, no title copy, no version of the writer).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub format: String,
    pub format_version: u32,
    /// The kit the file is for (`kit.layout`).
    pub kit: String,
    /// The KDM version, a copy for hosts that do not load the KDM.
    pub kdm: u32,
    /// The `kit.layout` schema version, a copy.
    pub schema_version: u32,
    /// The KDM document id, a copy (so a host can index the file without the engine).
    pub doc_id: String,
}

/// The parts of a `.klayout` file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Container {
    pub meta: Meta,
    /// `content.json`: the KDM content (`Replica::content_snapshot`).
    pub content: Vec<u8>,
    /// Blob bytes by their SHA-256 (lower-case hex).
    pub blobs: BTreeMap<String, Vec<u8>>,
    /// Entries this build does not know, by name, kept byte for byte.
    pub extra: Vec<(String, Vec<u8>)>,
}

/// SHA-256 as lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
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
    let mut content = None;
    let mut blobs = BTreeMap::new();
    let mut extra = Vec::new();
    for (name, data) in entries {
        if name == "mimetype" {
            continue;
        }
        if name == META {
            meta_bytes = Some(data);
        } else if name == CONTENT {
            content = Some(data);
        } else if let Some(hash) = name.strip_prefix("blobs/") {
            if sha256_hex(&data) != hash {
                return Err(damaged(format!("{name}: the bytes do not match their hash")));
            }
            blobs.insert(hash.to_owned(), data);
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
    let content = content.ok_or_else(|| damaged("no content.json"))?;
    Ok(Container { meta, content, blobs, extra })
}

/// Writes a `.klayout` file: the entries in the order of the module docs, unknown entries last.
/// `meta.format` and `meta.formatVersion` are set here. The same parts give the same bytes (no
/// entry carries a time).
pub fn write(container: &Container) -> Result<Vec<u8>, ContainerError> {
    let io = |error: std::io::Error| damaged(error.to_string());
    let zip_io = |error: zip::result::ZipError| damaged(error.to_string());
    let mut meta = container.meta.clone();
    meta.format = FORMAT.to_owned();
    meta.format_version = FORMAT_VERSION;
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
        zip.start_file(CONTENT, deflate.compression_level(Some(CONTENT_LEVEL))).map_err(zip_io)?;
        zip.write_all(&container.content).map_err(io)?;
        for (hash, bytes) in &container.blobs {
            zip.start_file(format!("blobs/{hash}"), stored).map_err(zip_io)?;
            zip.write_all(bytes).map_err(io)?;
        }
        let known = |name: &str| name == "mimetype" || name == META || name == CONTENT || name.starts_with("blobs/");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Container {
        let blob = b"\x89PNG fake".to_vec();
        Container {
            meta: Meta { kit: "kit.layout".into(), kdm: 1, schema_version: 1, doc_id: "doc-1".into(), ..Default::default() },
            content: b"content bytes".to_vec(),
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
        assert_eq!(back.content, b"content bytes");
        assert_eq!(back.meta.format, FORMAT);
        assert_eq!(back.meta.kit, "kit.layout");
        assert_eq!(back.blobs, sample().blobs);
        assert_eq!(back.extra, sample().extra, "an unknown entry survives");
        assert_eq!(write(&back).unwrap(), bytes, "writing what was read gives the same bytes");
    }

    /// `meta.json` holds what identifies the content and nothing that changes with a save.
    #[test]
    fn meta_json_names_the_content_only() {
        let bytes = write(&sample()).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut json = String::new();
        zip.by_name(META).unwrap().read_to_string(&mut json).unwrap();
        let meta: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut keys: Vec<&str> = meta.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["docId", "format", "formatVersion", "kdm", "kit", "schemaVersion"]);
        let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
        assert!(names.iter().all(|name| !name.starts_with("changes/") && name != "snapshot.bin"), "{names:?}");
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

    fn meta_json() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "format": FORMAT, "formatVersion": 1, "kit": "kit.layout", "kdm": 1, "schemaVersion": 1, "docId": "d"
        }))
        .unwrap()
    }

    #[test]
    fn refuses_damaged_and_foreign_files() {
        let meta = meta_json();
        let mime = MIME.as_bytes();
        let not = |bytes: &[u8]| matches!(read(bytes), Err(ContainerError::NotALayout(_)));
        let bad = |bytes: &[u8]| matches!(read(bytes), Err(ContainerError::Damaged(_)));
        assert!(not(b"not a zip"));
        assert!(not(&zip_of(&[("meta.json", &meta, true), ("mimetype", mime, false)])), "mimetype not first");
        assert!(not(&zip_of(&[("mimetype", mime, true), ("meta.json", &meta, true)])), "mimetype compressed");
        assert!(not(&zip_of(&[("mimetype", b"application/zip", false)])), "another MIME");
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("content.json", b"s", true)])), "no meta.json");
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true)])), "no content");
        assert!(
            bad(&zip_of(&[
                ("mimetype", mime, false),
                ("meta.json", &meta, true),
                ("snapshot.bin", b"s", false),
                ("changes/000001.bin", b"c", false)
            ])),
            "a file with a change log and no content entry"
        );
        for name in ["../evil", "/abs", "a\\b", "C:/x", "a/../b"] {
            assert!(
                bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true), ("content.json", b"s", true), (name, b"x", false)])),
                "{name}"
            );
        }
        let wrong_hash = format!("blobs/{}", sha256_hex(b"other"));
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true), ("content.json", b"s", true), (&wrong_hash, b"x", false)])));
        let bomb = serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "formatVersion": 1, "kit": "kit.layout", "kdm": 1, "schemaVersion": 1, "docId": "d", "pad": " ".repeat(200_000) })).unwrap();
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &bomb, true), ("content.json", b"s", true)])), "deflate bomb");
        let newer = serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "formatVersion": 9 })).unwrap();
        assert_eq!(
            read(&zip_of(&[("mimetype", mime, false), ("meta.json", &newer, true), ("content.json", b"s", true)])),
            Err(ContainerError::TooNew(9))
        );
        let other = serde_json::to_vec(&serde_json::json!({ "format": "klayout", "version": 1 })).unwrap();
        assert!(not(&zip_of(&[("mimetype", mime, false), ("meta.json", &other, true), ("document.json", b"{}", true)])), "the engine's own snapshot");
    }
}
