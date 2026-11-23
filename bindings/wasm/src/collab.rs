//! Kition's collaboration members (engine contract 2; design note section 4.2, task W3-22 DE3):
//! the layout as a KDM replica.
//!
//! The engine keeps its own `Session` (the command bridge to each document's replica,
//! `kition_layout_engine::replica`), not a `kition_kit::session::KitSession`, so
//! `kition_kit::export_kit_session!` does not apply; these members are added by hand with the JS
//! names and shapes of `kition-sdk`'s `session::web`, so Kition's host code is the same for every
//! kit. The wasm bindings live in [`crate::web`].
//!
//! A `.klayout` file holds the document's content, not its history (design note 3.1, kition-sdk
//! data model 7.3, rule M16b): opening one alone opens a replica of its content (same docId, same
//! object ids), and opening it with the replica the host stored takes the file into that replica.
//! The members below answer `replica/*` so a host can store the history and relay changes between
//! people editing the same file. Placed images are blobs, encoded for transfer as
//! `(64-char ASCII-hex sha256, u64 LE length, bytes) * N`, as every kit does.

use std::collections::HashMap;
use std::sync::Arc;

use kition_kit::KitDocument;
use kition_layout_engine::Session;
use kition_layout_kdm::kition_kit::kdm::{Origin, Version};
use kition_layout_kdm::kition_kit::session::{Damage, MemoryBlobs};
use serde_json::Value;

use crate::{LayoutDocument, engine_error};

impl LayoutDocument {
    /// Opens a file with the replica a host stored for it (`snapshot`, its history; the Kit UI
    /// protocol's `replica` capability) and that replica's blobs (a blob bundle). A `.klayout`
    /// file of the same document (which holds the content and no history) is taken into the
    /// stored replica ([`kition_layout_kdm::LayoutSession::take_file`]): nothing when the replica
    /// has the file's version; the changes the file holds beyond it when the file's history
    /// starts at a version the replica has; and otherwise (the file was saved elsewhere, from
    /// changes the store never saw) the file's state, as one `system` transaction keyed by object
    /// id. Edits the replica has and the file lacks are kept. A `.klayout` file of another
    /// document, or a replica that does not open, leaves the file as [`KitDocument::open`] opens
    /// it. Empty `bytes` mean there is no file yet, only the replica (a host opening a shared
    /// layout from its sync room): the layout is the replica's alone, and a replica that does not
    /// open is an error. For any other file (IDML) a replica that does not open is ignored and the
    /// file opens as [`KitDocument::open`] opens it.
    pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        if kition_layout_kdm::is_klayout(bytes) {
            let (file, base) = kition_layout_kdm::open_file(bytes).map_err(engine_error)?;
            let stored = decode_blobs(blobs).and_then(|blobs| kition_layout_kdm::LayoutSession::open(snapshot, blobs).map_err(engine_error));
            let Ok(mut stored) = stored else {
                return Self::open(name, bytes);
            };
            if stored.doc_id() != file.doc_id() {
                return Self::open(name, bytes);
            }
            stored.take_file(&file).map_err(|error| format!("damaged: {error}"))?;
            let mut session = crate::files::session();
            session.add_document(kition_layout_engine::DocState::with_kdm(stored, Some(base), None));
            let document = Self::from_session(session);
            if let Ok(state) = document.active() {
                crate::place::history::remember(&state.doc);
            }
            return Ok(document);
        }
        let restored = (|| -> Result<Session, String> {
            let blobs = decode_blobs(blobs)?;
            let mut session = crate::files::session();
            session.open_snapshot(snapshot, blobs).map_err(engine_error)?;
            Ok(session)
        })();
        match restored {
            Ok(session) => Ok(Self::from_session(session)),
            Err(error) if bytes.is_empty() => Err(error),
            Err(_) => Self::open(name, bytes),
        }
    }

    /// Whether the layout has its replica (it always does once open: a `.klayout` file brings
    /// one, and a new or imported layout makes one at once).
    pub fn has_replica(&self) -> bool {
        self.session.kdm().is_some()
    }

    /// The replica's own document id (stable across saves, and the same on every replica of the
    /// layout), for the Kit UI protocol's `replica/info`.
    pub fn doc_id(&self) -> Option<String> {
        self.session.doc_id()
    }

    pub fn version(&mut self) -> Result<Vec<u8>, String> {
        Ok(self.session.version().map_err(engine_error)?.encode())
    }

    /// The changes since `since` (empty bytes: every change).
    pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.session.export_changes(&decode_version(since)?).map_err(engine_error)
    }

    /// Imports a peer's (or stored) changes; add their blobs first with
    /// [`LayoutDocument::add_blobs`]. Only the replica's repair follows: no page is added or
    /// removed because of someone else's typing (design note 4.4).
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage, String> {
        self.session.import_changes(bytes).map_err(engine_error)
    }

    pub fn snapshot(&mut self) -> Result<Vec<u8>, String> {
        self.session.snapshot().map_err(engine_error)
    }

    /// Undoes this replica's last step. `done` is `false`, not an error, when there is nothing to
    /// undo.
    pub fn undo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_undo();
        if done {
            self.session.undo_redo(true, None).map_err(engine_error)?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    pub fn redo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_redo();
        if done {
            self.session.undo_redo(false, None).map_err(engine_error)?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    pub fn can_undo(&self) -> bool {
        self.session.active().is_some_and(|state| state.can_undo())
    }

    pub fn can_redo(&self) -> bool {
        self.session.active().is_some_and(|state| state.can_redo())
    }

    /// Runs a command as `origin` (a person, an agent): the replica records it as theirs. Contract
    /// 1's [`KitDocument::execute`] is this with the local person.
    pub fn execute_as(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        self.refuse_unknown(command, &params)?;
        if command == crate::render::ID {
            return crate::render::run(self, &params);
        }
        let params = crate::addresses::resolve(command, &params, self.session.kdm().map(|kdm| kdm.replica()))?;
        self.session.execute_as(command, &params, origin).map_err(engine_error)
    }

    /// Every blob the replica holds, as one buffer a peer feeds into [`LayoutDocument::add_blobs`].
    pub fn blobs(&mut self) -> Result<Vec<u8>, String> {
        Ok(encode_blobs(&self.session.blobs().map_err(engine_error)?))
    }

    /// Adds a peer's blobs (from their `blobs()` or `blobsSince()`), before importing its changes.
    pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), String> {
        let blobs = decode_blobs(bytes)?;
        self.session.add_blobs(&blobs).map_err(engine_error)
    }

    /// The blobs newly needed since `since` (empty bytes: every blob the state references).
    pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        Ok(encode_blobs(&self.session.blobs_since(&decode_version(since)?).map_err(engine_error)?))
    }

    /// How many times the layout left its replica because it refused a write (a bug); the next
    /// edit makes a fresh replica that collaborators must join from its snapshot. Hosts watch this.
    pub fn kdm_resets(&self) -> u32 {
        self.session.active().map_or(0, |state| u32::try_from(state.kdm_resets()).unwrap_or(u32::MAX))
    }

    /// The schema version the replica reads (`kit.json`'s `model.schemaVersion`).
    pub fn schema_version(&self) -> u32 {
        self.session.kdm().map_or(1, |kdm| kdm.replica().schema().schema_version)
    }

    /// The newest schema version any of the replica's changes was written under (rule M5a).
    pub fn author_schema_version(&self) -> Option<u32> {
        self.session.kdm().and_then(|kdm| kdm.replica().author_schema_version())
    }
}

fn decode_version(since: &[u8]) -> Result<Version, String> {
    if since.is_empty() { Ok(Version::empty()) } else { Version::decode(since).map_err(|error| error.to_string()) }
}

/// Encodes blobs for one call's worth of blob transfer: `(64-byte ASCII-hex hash, u64 LE length,
/// bytes) * N`.
pub(crate) fn encode_blobs(blobs: &MemoryBlobs) -> Vec<u8> {
    let mut hashes: Vec<&String> = blobs.0.keys().collect();
    hashes.sort();
    let mut out = Vec::new();
    for hash in hashes {
        let data = &blobs.0[hash];
        out.extend_from_slice(hash.as_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// Reverses [`encode_blobs`]; truncated or malformed input is an error, never a panic.
pub(crate) fn decode_blobs(bytes: &[u8]) -> Result<MemoryBlobs, String> {
    let mut map = HashMap::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let hash_end = i.checked_add(64).ok_or("blob bundle: overflow")?;
        let hash = bytes.get(i..hash_end).ok_or("truncated blob bundle: hash")?;
        let hash = std::str::from_utf8(hash).map_err(|_| "blob bundle: hash is not ASCII".to_owned())?.to_owned();
        let len_end = hash_end.checked_add(8).ok_or("blob bundle: overflow")?;
        let len_bytes: [u8; 8] =
            bytes.get(hash_end..len_end).ok_or("truncated blob bundle: length")?.try_into().map_err(|_| "blob bundle: length".to_owned())?;
        let len = usize::try_from(u64::from_le_bytes(len_bytes)).map_err(|_| "blob bundle: length too large".to_owned())?;
        let data_end = len_end.checked_add(len).ok_or("blob bundle: overflow")?;
        let data = bytes.get(len_end..data_end).ok_or("truncated blob bundle: data")?;
        map.insert(hash, Arc::from(data));
        i = data_end;
    }
    Ok(MemoryBlobs(map))
}

/// The object model (`KitObjectModel 1.0`) over the document's change exchange: `kition-layout`
/// serves `model.describe` and `model.batch` headless through
/// `kition_kit::session::model::Exchange`, and `model.open`, `model.import`, `model.export` and
/// `model.save` through these members, with the placed images the changes reference: an agent's
/// draft (KHP A12) is a replica opened so.
impl kition_kit::session::model::ReplicaPort for LayoutDocument {
    fn model_schema(&self) -> Arc<kition_kit::kdm::Schema> {
        kition_layout_kdm::schema()
    }

    fn model_export(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.export_changes(since)
    }

    fn model_import(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.import_changes(bytes).map(|_| ())
    }

    fn model_command(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        self.execute_as(command, params, origin)
    }

    fn model_commands(&self) -> Vec<kition_kit::KitCommand> {
        self.commands()
    }

    /// The placed images changes reference, before `model.import` takes the changes.
    fn model_add_blobs(&mut self, blobs: &[u8]) -> Result<(), String> {
        self.add_blobs(blobs)
    }

    /// The placed images a replica at `since` needs, so `model.export` answers them with the
    /// changes and the layout opens elsewhere (an agent's draft, KHP A12) with its images.
    fn model_blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.blobs_since(since)
    }

    /// A shared layout with no file yet (a server's headless kit opening a sync room, an agent's
    /// draft): the replica alone, failing rather than falling back to an empty layout.
    fn model_open_replica(name: &str, snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        Self::open_with_replica(name, &[], snapshot, blobs)
    }
}

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;
    use kition_kit::session::model::{Exchange, ModelErrorCode};
    use serde_json::json;

    use super::*;

    fn fixture() -> Vec<u8> {
        let mut doc = LayoutDocument::create("Flyer.klayout", 612, 792).unwrap();
        doc.execute("frame.create", json!({ "rect": [72, 72, 216, 144], "content": "unassigned" })).unwrap();
        doc.save(None).unwrap()
    }

    fn frame(doc: &mut LayoutDocument, x: u32) {
        doc.execute("frame.create", json!({ "rect": [x, 300, x + 40, 340], "content": "unassigned" })).unwrap();
    }

    fn items(doc: &LayoutDocument) -> usize {
        doc.session.doc().unwrap().doc.spreads.iter().map(|sp| sp.items.len()).sum()
    }

    #[test]
    fn a_klayout_file_brings_its_replica() {
        let bytes = fixture();
        let a = LayoutDocument::open("Flyer.klayout", &bytes).unwrap();
        let b = LayoutDocument::open("Flyer.klayout", &bytes).unwrap();
        assert!(a.has_replica());
        assert_eq!(a.doc_id(), b.doc_id(), "the identity travels with the file");
        assert!(!a.dirty() && !a.can_undo());
        let created = LayoutDocument::create("New.klayout", 612, 792).unwrap();
        assert!(created.has_replica() && !created.dirty() && !created.can_undo());
    }

    #[test]
    fn undo_redo_report_done_and_damage() {
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        frame(&mut doc, 300);
        let (done, damage) = doc.undo().unwrap();
        assert!(done && damage != Damage::None);
        assert!(doc.can_redo());
        assert!(doc.redo().unwrap().0);
        doc.undo().unwrap();
        assert!(!doc.undo().unwrap().0, "nothing left to undo is no error");
    }

    #[test]
    fn two_layouts_converge_through_their_changes_and_blobs() {
        let bytes = fixture();
        let mut a = LayoutDocument::open("Flyer.klayout", &bytes).unwrap();
        let mut b = LayoutDocument::open("Flyer.klayout", &bytes).unwrap();
        frame(&mut a, 300);
        b.execute_as("frame.create", json!({ "rect": [400, 400, 440, 440], "content": "unassigned" }), &Origin::person("person:b")).unwrap();
        let (va, vb) = (a.version().unwrap(), b.version().unwrap());
        b.add_blobs(&a.blobs_since(&vb).unwrap()).unwrap();
        b.import_changes(&a.export_changes(&vb).unwrap()).unwrap();
        a.add_blobs(&b.blobs_since(&va).unwrap()).unwrap();
        a.import_changes(&b.export_changes(&va).unwrap()).unwrap();
        assert_eq!(a.session.state_hash().unwrap(), b.session.state_hash().unwrap());
        assert_eq!(a.render(0.0, 0.0, 1.0, a.width(), a.height()).unwrap(), b.render(0.0, 0.0, 1.0, b.width(), b.height()).unwrap());
        assert_eq!(items(&a), 3);
    }

    #[test]
    fn a_replica_opens_without_a_file_and_a_bad_one_is_an_error() {
        let mut a = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        frame(&mut a, 300);
        let changes = a.export_changes(&[]).unwrap();
        let b = LayoutDocument::open_with_replica("Shared.klayout", &[], &changes, &a.blobs().unwrap()).unwrap();
        assert_eq!(b.doc_id(), a.doc_id());
        assert_eq!(items(&b), 2);
        assert!(!b.dirty());
        assert!(LayoutDocument::open_with_replica("Shared.klayout", &[], b"not a replica", &[]).is_err());
        // A .klayout file wins over a stored replica: it is what the person saved.
        let file = a.save(None).unwrap();
        let other = LayoutDocument::create("Other.klayout", 612, 792).unwrap().snapshot().unwrap();
        let c = LayoutDocument::open_with_replica("Flyer.klayout", &file, &other, &[]).unwrap();
        assert_eq!(c.doc_id(), a.doc_id());
    }

    #[test]
    fn blobs_round_trip_and_bad_bundles_are_errors() {
        let mut blobs = MemoryBlobs::default();
        blobs.0.insert("a".repeat(64), Arc::from(b"hello".as_slice()));
        blobs.0.insert("b".repeat(64), Arc::from(b"".as_slice()));
        let back = decode_blobs(&encode_blobs(&blobs)).unwrap();
        assert_eq!(back.0.len(), 2);
        assert!(decode_blobs(&[1, 2, 3]).is_err());
        let mut lying = "a".repeat(64).into_bytes();
        lying.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode_blobs(&lying).is_err());
    }

    /// `kition-layout` serves the object model through `ReplicaPort`: a command and a model edit in
    /// batches reach the engine, and a read-only batch refuses writes.
    #[test]
    fn serves_the_object_model_over_its_change_exchange() {
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        let mut exchange = Exchange::new();
        let described = exchange.describe(&mut doc).unwrap();
        assert_eq!(described["capability"], "KitObjectModel 1.0");
        assert!(described["commands"].as_array().unwrap().iter().any(|command| command["id"] == "frame.create"));
        let command =
            json!({ "ops": [{ "op": "command", "id": "frame.create", "params": { "rect": [300, 300, 340, 340], "content": "unassigned" } }] });
        assert_eq!(exchange.batch(&mut doc, &command, true).unwrap_err().code, ModelErrorCode::ReadOnly);
        exchange.batch(&mut doc, &command, false).unwrap();
        assert_eq!(items(&doc), 2, "the command ran in the engine");
        let title = json!({ "ops": [{ "op": "set", "target": "layout", "field": "title", "value": "Renamed by a script" }] });
        exchange.batch(&mut doc, &title, false).unwrap();
        assert_eq!(doc.session.doc().unwrap().doc.title, "Renamed by a script", "the model edit reached the engine's view");
        let loaded = exchange.batch(&mut doc, &json!({ "ops": [], "load": [{ "target": "layout", "fields": ["title"] }] }), false).unwrap();
        assert_eq!(loaded["loaded"][0]["fields"]["title"], "Renamed by a script");
    }
}
