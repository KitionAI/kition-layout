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
//! The members below answer `replica/*` so a host can store the history and a replica on the same
//! line of changes (an agent's draft) can take them in order. Placed images are blobs, encoded for transfer as
//! `(64-char ASCII-hex sha256, u64 LE length, bytes) * N`, as every kit does.

use std::collections::HashMap;
use std::sync::Arc;

use kition_kit::KitDocument;
use kition_layout_engine::Session;
use kition_layout_kdm::kition_kit::kdm::{Origin, UndoScope, Version};
use kition_layout_kdm::kition_kit::session::{Damage, MemoryBlobs};
use serde_json::{Value, json};

use crate::{LayoutDocument, engine_error};

impl LayoutDocument {
    /// Opens a file with the replica a host stored for it (`snapshot`, its history; the Kit UI
    /// protocol's `replica` capability) and that replica's blobs (a blob bundle). A `.klayout`
    /// file of the same document (which holds the content and no history) is taken into the
    /// stored replica ([`kition_layout_kdm::LayoutSession::take_file`]): nothing when the replica
    /// holds the file's state; otherwise (the file was saved elsewhere, or changed outside) the
    /// file's state, as one `system` transaction keyed by object id, on the stored replica's line
    /// of changes. A `.klayout` file of another
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

    /// The replicated state, the replica made first if there is none yet.
    fn replicated_state(&mut self) -> Result<Value, String> {
        self.session.version().map_err(engine_error)?;
        let kdm = self.session.kdm().ok_or("the layout has no replica")?;
        kdm.replica().state_json().map_err(engine_error)
    }

    /// Takes a draft's changes (from a replica on this one's line, an agent's draft) in as one
    /// undo step of the person (`mergeChanges`, as `kition-sdk`'s `export_kit_session!` answers
    /// it), also under the agent's origin Kition's editor passes (KitModel M12a), with the
    /// layout's repair in the same step: the engine's layout follows, and one Undo takes the
    /// whole draft back to the exact state before. Add the draft's blobs first. Answers
    /// `{"damage", "summary", "scope"}`: `summary` the `StateDiff` of what the merge and its repair
    /// changed (read at the objects they wrote), `scope` the undo scope of the step (`"person"`,
    /// or `null` when the draft, repaired, changes nothing and no step was recorded). A refused
    /// merge changes nothing.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<Value, String> {
        let since = self.session.version().map_err(engine_error)?;
        let (scope, damage) = self.session.merge_changes(bytes, origin).map_err(engine_error)?;
        let kdm = self.session.kdm().ok_or("the layout has no replica")?;
        let summary = kdm.replica().diff_since(&since).map_err(engine_error)?;
        Ok(json!({ "damage": damage, "summary": summary.to_json(), "scope": scope.map(UndoScope::as_str) }))
    }

    /// What merging a draft's changes would change in the layout now, the layout's repair
    /// included as [`LayoutDocument::merge_changes`] runs it, as `StateDiff` JSON: what accepting
    /// the draft gives. Nothing changes. Refuses what [`LayoutDocument::merge_changes`] refuses.
    pub fn compare_changes(&self, bytes: &[u8]) -> Result<Value, String> {
        let kdm = self.session.kdm().ok_or("the layout has no replica, so no draft of it")?;
        Ok(kdm.compare_changes(bytes).map_err(engine_error)?.to_json())
    }

    /// Writes the state a snapshot holds (a checkpoint of this layout, the Kit UI protocol's
    /// `replica/restore`) over the layout by object id, with the layout's repair in it, as one
    /// undo step of `origin`'s scope: the engine's layout follows. Add the blobs it references
    /// first. Nothing changes when the states are equal; another document's snapshot is refused.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<Damage, String> {
        self.session.restore_state(snapshot, origin).map(|(_, damage)| damage).map_err(engine_error)
    }

    /// An agent's reply (JSON text) with every object id it holds as a value or a key named as
    /// the tool interface names it (its tool id, KitObjectModel OM9); unchanged without a replica
    /// or when the schema declares no tool ids. The layout keeps its own ids. The editor sends
    /// every `ai/inspect`, `document/check` and `ai/run` reply through it, so the tool ids are read
    /// only at the objects the reply names (`ToolIds::show_reply`), never for the whole layout.
    pub fn model_reply(&self, value: &str) -> Result<String, String> {
        let value: Value = serde_json::from_str(value).map_err(|error| format!("the reply is not JSON: {error}"))?;
        let Some(kdm) = self.session.kdm() else { return Ok(value.to_string()) };
        Ok(kition_kit::kdm::tool_ids::ToolIds::show_reply(kdm.replica(), &value).to_string())
    }

    /// What changed since the version `since` (empty bytes: since the replica began), a receipt
    /// as `kition-sdk`'s `changesSince` answers it: `{"changes": StateDiff}`, or
    /// `{"changes": null, "unavailable": "<why>"}` when the replica's log does not hold `since`
    /// (or there is no replica).
    pub fn changes_since(&self, since: &[u8]) -> Value {
        let Some(kdm) = self.session.kdm() else { return json!({ "changes": null, "unavailable": "the layout has no replica" }) };
        let replica = kdm.replica();
        match decode_version(since).and_then(|since| replica.changes_since(&since).map_err(|error| error.to_string())) {
            Ok(changes) => json!({ "changes": changes.to_recorded_json(replica.schema()) }),
            Err(error) => json!({ "changes": null, "unavailable": error }),
        }
    }

    /// `model/describe` (`KitObjectModel 1.0`) over the layout's change exchange.
    pub fn model_describe(&mut self) -> Result<Value, String> {
        kition_kit::session::model::Exchange::new().describe(self).map_err(|error| error.to_json().to_string())
    }

    /// `model/batch` over the layout's change exchange: the request and the answer as JSON; a
    /// failure's message is the error's JSON (`{"code", "index", "message"}`).
    pub fn model_batch(&mut self, request: &Value) -> Result<Value, String> {
        kition_kit::session::model::Exchange::new().batch(self, request, false).map_err(|error| error.to_json().to_string())
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
        doc.execute("frame.add", json!({ "rect": [72, 72, 216, 144], "content": "unassigned" })).unwrap();
        doc.save(None).unwrap()
    }

    fn frame(doc: &mut LayoutDocument, x: u32) {
        doc.execute("frame.add", json!({ "rect": [x, 300, x + 40, 340], "content": "unassigned" })).unwrap();
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

    /// A draft (a replica on the layout's line, an agent's) merges in as one step of the person's
    /// Undo, also under the agent's origin Kition's editor passes (KitModel M12a): the comparison
    /// beforehand names what the merge does and changes nothing, the engine's layout follows the
    /// merge, the edits made here meanwhile (a retitle and a frame beside the draft's, in the same
    /// items tree) stay, one Undo gives back the exact state before the merge and one Redo the exact
    /// merged state, and a refused merge changes nothing.
    #[test]
    fn a_draft_merges_in_as_one_undo_step_and_the_layout_follows() {
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        let base = doc.version().unwrap();
        let mut draft = LayoutDocument::open_with_replica("Flyer.klayout", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
        frame(&mut draft, 400);
        let changes = draft.export_changes(&base).unwrap();
        doc.model_batch(&json!({ "ops": [{ "op": "set", "target": "layout", "field": "title", "value": "Meanwhile" }] })).unwrap();
        frame(&mut doc, 100);
        let before = items(&doc);
        let state_before = doc.replicated_state().unwrap();
        let version = doc.version().unwrap();
        let preview = doc.compare_changes(&changes).unwrap();
        assert_eq!(doc.version().unwrap(), version, "comparing changes nothing");
        let origin = Origin::agent("agent:helper");
        let answer = doc.merge_changes(&changes, &origin).unwrap();
        assert_eq!(answer["summary"], preview, "the comparison named what the merge did: {answer}");
        assert_eq!(answer["scope"], "person", "the person's step, under the agent's origin: {answer}");
        assert_ne!(answer["damage"], json!("none"), "{answer}");
        assert_eq!(items(&doc), before + 1, "the engine's layout follows the merge");
        assert_eq!(doc.session.doc().unwrap().doc.title, "Meanwhile", "the edit made here stays");
        assert!(doc.dirty());
        let merged = doc.replicated_state().unwrap();
        assert!(doc.undo().unwrap().0);
        assert_eq!(items(&doc), before, "one undo takes the whole draft back");
        assert_eq!(doc.replicated_state().unwrap(), state_before, "to the exact state before the merge");
        assert_eq!(doc.session.doc().unwrap().doc.title, "Meanwhile", "the person's edits stay");
        assert!(doc.redo().unwrap().0);
        assert_eq!(doc.replicated_state().unwrap(), merged, "one redo: the exact merged state");
        let at = doc.version().unwrap();
        assert!(doc.merge_changes(b"not changes", &origin).is_err());
        assert_eq!(doc.version().unwrap(), at, "a refused merge changes nothing");
    }

    /// An earlier state (a checkpoint's snapshot) is written over the layout by object id as one
    /// undo step: the replica holds that state again, with the same ids, and Undo takes it back.
    #[test]
    fn a_restored_state_keeps_the_ids_and_undoes_as_one_step() {
        use kition_kit::kdm::{ActorId, Replica};
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        let checkpoint = doc.snapshot().unwrap();
        let before = items(&doc);
        frame(&mut doc, 300);
        assert_eq!(items(&doc), before + 1);
        let edited = doc.replicated_state().unwrap();
        let origin = Origin::person("person:local");
        let damage = doc.restore_state(&checkpoint, &origin).unwrap();
        assert_ne!(damage, Damage::None);
        assert_eq!(items(&doc), before, "the engine's layout follows the restore");
        let then = Replica::load(kition_layout_kdm::schema(), &checkpoint, ActorId::random()).unwrap().state_json().unwrap();
        assert_eq!(doc.replicated_state().unwrap()["objects"], then["objects"], "the checkpoint's objects, under their ids");
        let at = doc.version().unwrap();
        assert_eq!(doc.restore_state(&checkpoint, &origin).unwrap(), Damage::None, "the same state again changes nothing");
        assert_eq!(doc.version().unwrap(), at);
        let restored = doc.replicated_state().unwrap();
        assert!(doc.undo().unwrap().0);
        assert_eq!(items(&doc), before + 1, "one undo takes the restore back");
        assert_eq!(doc.replicated_state().unwrap(), edited, "to the exact state before the restore");
        assert!(doc.redo().unwrap().0);
        assert_eq!(doc.replicated_state().unwrap(), restored, "one redo: the exact restored state");
        let other = LayoutDocument::create("Other.klayout", 612, 792).unwrap().snapshot().unwrap();
        assert!(doc.restore_state(&other, &origin).is_err(), "another document's state is refused");
    }

    /// An agent's reply names objects by their tool ids; text that merely holds an id stays.
    #[test]
    fn a_reply_names_objects_by_their_tool_ids() {
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        let state = doc.replicated_state().unwrap();
        let item = state["objects"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, object)| object["$type"] == "Item")
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| panic!("an item object: {state}"));
        let shown: Value =
            serde_json::from_str(&doc.model_reply(&json!({ "target": item, "note": format!("about {item}") }).to_string()).unwrap()).unwrap();
        assert!(shown["target"].as_str().is_some_and(|id| id != item), "{shown}");
        assert_eq!(shown["note"], json!(format!("about {item}")), "text that holds an id stays");
        assert!(doc.model_reply("not json").is_err());
    }

    /// `modelReply` reads the tool ids of the objects a reply names alone (the editor sends every
    /// `ai/inspect`, `document/check` and `ai/run` reply through it): in a layout of many objects, a
    /// reply naming one gives the ids the whole layout's tool ids give, and a reply naming none
    /// comes back as it was.
    #[test]
    fn a_reply_reads_only_the_objects_it_names() {
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        for x in 0..40 {
            frame(&mut doc, 10 * x);
        }
        let state = doc.replicated_state().unwrap();
        let item = state["objects"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, object)| object["$type"] == "Item")
            .map(|(id, _)| id.clone())
            .last()
            .unwrap_or_else(|| panic!("an item object: {state}"));
        let reply = json!({ "target": item, "items": [{ "id": item, "note": "one of many" }] });
        let shown: Value = serde_json::from_str(&doc.model_reply(&reply.to_string()).unwrap()).unwrap();
        let replica = doc.session.kdm().unwrap().replica();
        let whole = kition_kit::kdm::tool_ids::ToolIds::of_replica(replica);
        assert_eq!(shown, whole.show_reply_json(&reply), "the ids the whole layout's tool ids give");
        assert!(shown["target"].as_str().is_some_and(|id| id != item), "{shown}");
        let none = json!({ "pages": 2, "note": "no object here", "items": [] }).to_string();
        assert_eq!(doc.model_reply(&none).unwrap(), none, "a reply naming no object comes back as it was");
    }

    /// A receipt names what changed since a version; a version the log does not hold is
    /// unavailable, never an empty change set.
    #[test]
    fn changes_since_answers_a_receipt_or_unavailable() {
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        let since = doc.version().unwrap();
        frame(&mut doc, 300);
        let receipt = doc.changes_since(&since);
        assert!(receipt["changes"].is_object(), "{receipt}");
        assert!(receipt["changes"].to_string().contains("Item"), "the new frame is in the receipt: {receipt}");
        let other = LayoutDocument::create("Other.klayout", 612, 792).unwrap().version().unwrap();
        let refused = doc.changes_since(&other);
        assert!(refused["changes"].is_null() && refused["unavailable"].is_string(), "{refused}");
    }

    /// The wasm `modelDescribe` and `modelBatch` members answer the object model over the
    /// layout's change exchange.
    #[test]
    fn model_describe_and_model_batch_answer_the_object_model() {
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        let described = doc.model_describe().unwrap();
        assert_eq!(described["capability"], "KitObjectModel 1.0");
        let title = json!({ "ops": [{ "op": "set", "target": "layout", "field": "title", "value": "From modelBatch" }] });
        doc.model_batch(&title).unwrap();
        assert_eq!(doc.session.doc().unwrap().doc.title, "From modelBatch");
        let bad = doc.model_batch(&json!({ "ops": [{ "op": "set", "target": "nothing-here", "field": "title", "value": 1 }] })).unwrap_err();
        assert!(serde_json::from_str::<Value>(&bad).is_ok_and(|error| error["code"].is_string()), "{bad}");
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
        assert!(described["commands"].as_array().unwrap().iter().any(|command| command["id"] == "frame.add"));
        let command = json!({ "ops": [{ "op": "command", "id": "frame.add", "params": { "rect": [300, 300, 340, 340], "content": "unassigned" } }] });
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
