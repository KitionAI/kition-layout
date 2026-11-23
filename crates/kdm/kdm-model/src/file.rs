//! The `.klayout` file as a replica (design note sections 3.1–3.2): the file holds the document's
//! **content**, not its history (kition-sdk data model 7.3, rule M16b). [`save_file`] writes the
//! replica's content snapshot (its state, and of its history only a `system` seal) and the blobs
//! the state references; [`open_file`] loads it as the same document (same docId, same object ids)
//! and repairs once; it never writes. The history lives in the host's history store (the live
//! replica's [`LayoutSession::snapshot`]), never in the file.
//!
//! A file opens on top of the replica the host stored for its document through
//! [`LayoutSession::take_file`] (`kition_kit::session::take_file`, data model 7.3).

use std::collections::BTreeMap;
use std::sync::Arc;

use kition_kit::kdm::{ActorId, KDM_VERSION, Replica, Version};
use kition_kit::session::{BlobSource, KitError, MemoryBlobs};
use kition_layout_format::container::{self, Container, ContainerError, Meta};

use crate::session::LayoutSession;
use crate::{Result, kit_err};

/// The kit id `meta.json` names.
pub const KIT: &str = "kit.layout";

/// What a session's file holds besides its content.
#[derive(Clone, Debug)]
pub struct FileBase {
    /// The replica version the file's content holds (its seal included).
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
}

/// A save: the file's bytes, what the file then holds, and the seal the save made.
#[derive(Debug)]
pub struct SavedFile {
    pub bytes: Vec<u8>,
    pub base: FileBase,
    /// The seal of the fallback in [`save_file`] (a replica nobody sealed): changes to import into
    /// the replica once the file is stored. `None` on the direct path.
    pub seal: Option<Vec<u8>>,
}

/// Why a file did not open, as the host reports it: `not-a-layout: …`, `damaged: …`, or a newer
/// format.
fn container_error(error: ContainerError) -> KitError {
    KitError::new(error.to_string())
}

/// The container's parts with the docId its content holds, checked against `meta.json`.
fn read(bytes: &[u8]) -> Result<(Container, Replica)> {
    let parts = container::read(bytes).map_err(container_error)?;
    let replica = load(&parts.content)?;
    let doc_id = replica.doc_id().to_string();
    if parts.meta.doc_id != doc_id {
        return Err(damaged(format!("meta.json names document {}, the content {doc_id}", parts.meta.doc_id)));
    }
    Ok((parts, replica))
}

/// Opens a `.klayout` file as a replica of a fresh actor: the document the file holds, with its
/// docId and object ids, and of its history only the seal.
pub fn open_file(bytes: &[u8]) -> Result<(LayoutSession, FileBase)> {
    let parts = container::read(bytes).map_err(container_error)?;
    let blobs = MemoryBlobs(parts.blobs.iter().map(|(hash, data)| (hash.clone(), Arc::from(data.as_slice()))).collect());
    let (session, version) = LayoutSession::open_with_version(&parts.content, blobs).map_err(damaged)?;
    let doc_id = session.doc_id().to_string();
    if parts.meta.doc_id != doc_id {
        return Err(damaged(format!("meta.json names document {}, the content {doc_id}", parts.meta.doc_id)));
    }
    Ok((session, FileBase { version, doc_id, extra: parts.extra }))
}

/// The actor that seals a copy of a replica at `version` (the fallback in [`save_file`]): one per
/// version, so saving one version twice writes the same seal, and two versions never share a
/// seal's change id.
fn seal_actor(version: &Version) -> ActorId {
    let hash = container::sha256_hex(&version.encode());
    let value = hash.get(..16).and_then(|hex| u64::from_str_radix(hex, 16).ok()).unwrap_or(0) >> 1;
    ActorId::from_u64(value).unwrap_or_else(|_| ActorId::random())
}

/// What a save writes as `content.bin`: the bytes, the version they hold, and the seal made for
/// them on a copy, if any.
struct Content {
    bytes: Vec<u8>,
    version: Version,
    seal: Option<Vec<u8>>,
}

/// The content `replica` saves as. The direct path: the host sealed the replica
/// (`KitDocument::prepare_save`, [`LayoutSession::seal`]), and its own content snapshot is written.
fn content(replica: &Replica) -> Result<Content> {
    let version = replica.version();
    if replica.is_sealed() {
        return Ok(Content { bytes: replica.sealed_content_snapshot().map_err(kit_err)?, version, seal: None });
    }
    // FALLBACK, delete once a kit-editor release that calls prepareSave is in web/pnpm-lock.yaml:
    // until then the editor saves a replica nobody sealed, and `save` sees it read-only. The
    // content of a copy sealed under an actor derived from the version (the same version gives
    // the same bytes); `mark_saved` imports the seal.
    let mut sealed = replica.fork(seal_actor(&version)).map_err(kit_err)?;
    sealed.seal().map_err(kit_err)?;
    let seal = sealed.export_changes(&version).map_err(kit_err)?;
    Ok(Content { bytes: sealed.sealed_content_snapshot().map_err(kit_err)?, version: sealed.version(), seal: Some(seal) })
}

/// The `.klayout` bytes of `replica`'s content with `blobs` and the unknown entries `extra`.
fn write(replica: &Replica, blobs: BTreeMap<String, Vec<u8>>, extra: Vec<(String, Vec<u8>)>) -> Result<(Vec<u8>, Content)> {
    let content = content(replica)?;
    let parts = Container {
        meta: Meta {
            kit: KIT.to_owned(),
            kdm: KDM_VERSION,
            schema_version: replica.schema().schema_version,
            doc_id: replica.doc_id().to_string(),
            ..Default::default()
        },
        content: content.bytes.clone(),
        blobs,
        extra,
    };
    let bytes = container::write(&parts).map_err(kit_err)?;
    Ok((bytes, content))
}

/// The file `session` saves to, with `base` (what its file holds, when it is the same document, for
/// the entries this build does not know): the bytes, what the file then holds, and the seal the
/// fallback made. A sealed replica (the direct path) saves its own content snapshot, the same
/// bytes for the same version.
pub fn save_file(session: &LayoutSession, base: Option<&FileBase>) -> Result<SavedFile> {
    let doc_id = session.doc_id().to_string();
    let extra = base.filter(|base| base.doc_id == doc_id).map(|base| base.extra.clone()).unwrap_or_default();
    let blobs: BTreeMap<String, Vec<u8>> = session.referenced_blobs().into_iter().map(|(hash, bytes)| (hash, bytes.to_vec())).collect();
    let (bytes, Content { version, seal, .. }) = write(session.replica(), blobs, extra.clone())?;
    Ok(SavedFile { bytes, base: FileBase { version, doc_id, extra }, seal })
}

/// `replica` as a new `.klayout` file with the blobs its assets reference from `blobs`
/// (`KitModel::export`): its content snapshot, sealed as [`save_file`] seals.
pub(crate) fn export_file(replica: &Replica, blobs: &dyn BlobSource) -> Result<Vec<u8>> {
    let referenced = crate::session::referenced(replica);
    let blobs = referenced.into_iter().filter_map(|hash| blobs.get(&hash).map(|bytes| (hash, bytes.to_vec()))).collect();
    write(replica, blobs, Vec::new()).map(|(bytes, _)| bytes)
}

/// The replica a `.klayout` file embeds (`document.identity`, KitModel M7a): its content snapshot,
/// checked against `meta.json`'s copy of the docId. `None` for a file that holds no replica: an
/// empty file (one made for the kit before it saved a layout there) or an IDML package (another
/// program's file, which opens as a new replica each time). Other bytes answer `not-a-layout: …`
/// or `damaged: …`, as [`open_file`] does; never repairs, never writes.
pub fn embedded_replica(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    if bytes.is_empty() || kition_layout_idml::is_idml(bytes) {
        return Ok(None);
    }
    let (parts, _) = read(bytes)?;
    Ok(Some(parts.content))
}

/// `bytes` (a `.klayout` file) holding the document `snapshot` instead of its own
/// (`document.fork`, KitModel M7a): `content.bin` is the snapshot's content (a copy's birth is its
/// seal, so a copy adds no change), `meta.json`'s docId the snapshot's, and the blobs only the ones
/// its state references (what a save writes), so no earlier identity stays in the file. Entries
/// this build does not know are kept.
pub fn replace_replica(bytes: &[u8], snapshot: &[u8]) -> Result<Vec<u8>> {
    let mut parts = container::read(bytes).map_err(container_error)?;
    let mut replica = load(snapshot)?;
    let referenced = crate::session::referenced(&replica);
    parts.blobs.retain(|hash, _| referenced.contains(hash));
    parts.meta.doc_id = replica.doc_id().to_string();
    parts.content = replica.content_snapshot().map_err(kit_err)?;
    container::write(&parts).map_err(kit_err)
}

fn damaged(error: impl std::fmt::Display) -> KitError {
    KitError::new(format!("damaged: {error}"))
}

/// A layout snapshot loaded as a replica of a fresh actor: `damaged: …` when it does not load.
fn load(snapshot: &[u8]) -> Result<Replica> {
    Replica::load(crate::schema(), snapshot, ActorId::random()).map_err(damaged)
}
