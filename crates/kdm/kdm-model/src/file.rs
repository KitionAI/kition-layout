//! The `.klayout` file as a replica (design note sections 3.1–3.2): [`open_file`] reads the
//! container ([`kition_layout_format::container`]), loads the snapshot, imports the listed
//! changes in order, registers the blobs and repairs once; it never writes. [`save_file`]
//! re-emits the snapshot and the change entries it read or wrote byte for byte, appends one entry
//! with the changes since, and writes the blobs the state references, or writes a fresh snapshot
//! and no entries when the log has grown past the compaction thresholds
//! ([`kition_layout_format::container::wants_compaction`]). Nothing a save writes is re-encoded.

use std::collections::BTreeMap;
use std::sync::Arc;

use kition_kit::kdm::{KDM_VERSION, Version};
use kition_kit::session::{KitError, MemoryBlobs};
use kition_layout_format::container::{self, App, Container, ContainerError, Meta, SnapshotRef};

use crate::session::LayoutSession;
use crate::{Result, kit_err};

/// The kit id `meta.json` names.
pub const KIT: &str = "kit.layout";

/// What a session's file holds, so the next save re-emits it and appends what changed since.
#[derive(Clone, Debug)]
pub struct FileBase {
    snapshot: Arc<Vec<u8>>,
    snapshot_version: Version,
    changes: Vec<Arc<Vec<u8>>>,
    /// The replica version the file's snapshot and changes add up to.
    version: Version,
    doc_id: String,
    /// Entries this build does not know, written back as they were.
    extra: Vec<(String, Vec<u8>)>,
}

impl FileBase {
    /// The replica version the file holds.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// How many change entries the file has after its snapshot.
    pub fn change_entries(&self) -> usize {
        self.changes.len()
    }
}

/// Why a file did not open, as the host reports it: `not-a-layout: …`, `damaged: …`, or a newer
/// format.
fn container_error(error: ContainerError) -> KitError {
    KitError::new(error.to_string())
}

/// Opens a `.klayout` file as a replica of a fresh actor.
pub fn open_file(bytes: &[u8]) -> Result<(LayoutSession, FileBase)> {
    let parts = container::read(bytes).map_err(container_error)?;
    let blobs = MemoryBlobs(parts.blobs.iter().map(|(hash, data)| (hash.clone(), Arc::from(data.as_slice()))).collect());
    let snapshot_version = Version::decode(&base64_decode(&parts.meta.snapshot.version)).unwrap_or_else(|_| Version::empty());
    let (session, version) =
        LayoutSession::open_with_version(&parts.snapshot, &parts.changes, blobs).map_err(|error| KitError::new(format!("damaged: {error}")))?;
    let doc_id = session.doc_id().to_string();
    if !parts.meta.doc_id.is_empty() && parts.meta.doc_id != doc_id {
        return Err(KitError::new(format!("damaged: meta.json names document {}, the snapshot {doc_id}", parts.meta.doc_id)));
    }
    let base = FileBase {
        snapshot: Arc::new(parts.snapshot),
        snapshot_version,
        changes: parts.changes.into_iter().map(Arc::new).collect(),
        version,
        doc_id,
        extra: parts.extra,
    };
    Ok((session, base))
}

/// The file `session` saves to, on top of `base` (what it opened from or last saved, when it is
/// the same document): the bytes and what the file then holds. A save that has nothing new appends
/// nothing.
pub fn save_file(session: &LayoutSession, base: Option<&FileBase>, app_version: &str) -> Result<(Vec<u8>, FileBase)> {
    let doc_id = session.doc_id().to_string();
    let version = session.version();
    let base = base.filter(|base| base.doc_id == doc_id);
    let (snapshot, snapshot_version, mut changes, extra) = match base {
        Some(base) => (base.snapshot.clone(), base.snapshot_version.clone(), base.changes.clone(), base.extra.clone()),
        None => (Arc::new(session.snapshot()?), version.clone(), Vec::new(), Vec::new()),
    };
    if let Some(base) = base
        && base.version != version
    {
        changes.push(Arc::new(session.export_changes(&base.version)?));
    }
    let plain: Vec<Vec<u8>> = changes.iter().map(|c| (**c).clone()).collect();
    let (snapshot, snapshot_version, changes) = if base.is_some() && container::wants_compaction(snapshot.len(), &plain) {
        (Arc::new(session.snapshot()?), version.clone(), Vec::new())
    } else {
        (snapshot, snapshot_version, changes)
    };
    let blobs: BTreeMap<String, Vec<u8>> = session.referenced_blobs().into_iter().map(|(hash, bytes)| (hash, bytes.to_vec())).collect();
    let parts = Container {
        meta: Meta {
            kdm: KDM_VERSION,
            schema_version: session.replica().schema().schema_version,
            doc_id: doc_id.clone(),
            title: session.document().title.clone(),
            snapshot: SnapshotRef { entry: String::new(), version: base64_encode(&snapshot_version.encode()) },
            app: App { kit: KIT.to_owned(), version: app_version.to_owned() },
            ..Default::default()
        },
        snapshot: (*snapshot).clone(),
        changes: changes.iter().map(|c| (**c).clone()).collect(),
        blobs,
        extra: extra.clone(),
    };
    let bytes = container::write(&parts).map_err(kit_err)?;
    Ok((bytes, FileBase { snapshot, snapshot_version, changes, version, doc_id, extra }))
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub(crate) fn base64_decode(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut buf, mut bits) = (0u32, 0);
    for byte in text.bytes() {
        let Some(v) = ALPHABET.iter().position(|a| *a == byte) else { continue };
        buf = buf << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for bytes in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 7, 128, 9]] {
            assert_eq!(base64_decode(&base64_encode(bytes)), bytes);
        }
        assert_eq!(base64_encode(b"abcd"), "YWJjZA==");
    }
}
