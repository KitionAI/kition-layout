//! [`LayoutSession`]: one document's replica, its [`LayoutModel`] view and its blobs, for the
//! engine's command bridge (design note section 4; task W3-22 DE3).
//!
//! It is `kition_kit::session::KitSession<LayoutModel>`'s flow on concrete, `Send` parts, as Kition
//! Vector's and Photo's sessions are: transactions with an origin, the replica's undo, the change
//! exchange, the repair after an import, an undo or a redo (inside a draft's merge and a restore),
//! and the view patched from the replica.
//! The engine enumerates and ships the image blobs, which `KitSession`'s boxed blob store does not
//! allow. Edits are diff-bridged: the engine changes its own [`Document`] and
//! [`LayoutSession::record`] writes what changed as one transaction ([`write_changes`]), then
//! adopts it as the view, so no command needs rewriting (E12).
//!
//! **Repair.** [`repair::repair_in`] (the validation rules V1–V10, V12, V13, the read repair V6
//! and the engine ids) runs inside a draft's merge (`Replica::merge_changes_repaired`), its preview
//! and a restore, so its writes are part of that one change and undo step: one undo gives back the
//! exact state before, one redo the exact repaired state (data model 6.6 D9). A merge or a restore
//! that, repaired, leaves the document as it was records nothing. After an import, an undo or a
//! redo, and when a replica opens (a file written by another program), it runs as a `system`
//! transaction labelled `repair`, never an undo step, and only when there is something to repair.
//!
//! **Undo groups per author.** [`LayoutSession::begin_undo_group`] names the principal it is for:
//! a transaction of another principal ends the group first, so a person's gesture never takes in
//! an agent's edit (or the other way round).

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use kition_kit::kdm::diff::StateDiff;
use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, OriginKind, Replica, Tx, UndoOutcome, UndoScope, Version};
use kition_kit::session::{BlobSource, Damage, KitError, KitModel, MemoryBlobs};
use kition_layout_doc::Document;

use crate::{Ids, LayoutModel, Result, kit_err, repair, write_changes, write_document};

/// An open undo group: whose it is, and whether a transaction in it already made it a step.
#[derive(Clone, Debug)]
struct Group {
    principal: String,
    counted: bool,
}

/// One open document as a KDM replica and the engine's view of it. See the module docs.
pub struct LayoutSession {
    replica: Replica,
    model: LayoutModel,
    blobs: MemoryBlobs,
    group: Option<Group>,
    /// The hashes each record, import, undo and redo started referencing, with the version right
    /// after it, for [`LayoutSession::blobs_since`] (chronological, so `since` splits it into
    /// "already has" and "needs").
    blob_log: Vec<(Version, Vec<String>)>,
}

impl std::fmt::Debug for LayoutSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayoutSession").field("doc", &self.replica.doc_id()).field("actor", &self.replica.actor()).finish_non_exhaustive()
    }
}

/// Whether a transaction of `origin` is an undo step of this replica: a person's, an agent's or
/// an extension's, never an import's or the system's.
fn undoable(origin: &Origin) -> bool {
    !matches!(origin.kind, OriginKind::System | OriginKind::Import)
}

/// The blob hashes the replica's assets reference.
pub(crate) fn referenced(replica: &Replica) -> BTreeSet<String> {
    replica
        .objects_of_type("Asset")
        .into_iter()
        .filter_map(|asset| replica.get(&asset, "blob").ok())
        .filter_map(|blob| blob.get("hash").and_then(serde_json::Value::as_str).map(str::to_owned))
        .collect()
}

/// [`Replica::transact_unless_unchanged`] with a kit error kept aside: the KDM's closure must fail
/// with a [`KdmError`]. A transaction that leaves the state as it was writes nothing.
fn transact_unless_unchanged(replica: &mut Replica, origin: &Origin, f: impl FnOnce(&mut Tx<'_>) -> Result<()>) -> Result<()> {
    let mut kit_error = None;
    let result = replica.transact_unless_unchanged(origin, |tx| {
        f(tx).map_err(|error| match error {
            KitError::Kdm(error) => error,
            other => {
                let message = other.to_string();
                kit_error = Some(other);
                KdmError::Engine(message)
            }
        })
    });
    match (result, kit_error) {
        (_, Some(error)) => Err(error),
        (result, None) => result.map(|((), _)| ()).map_err(kit_err),
    }
}

impl LayoutSession {
    /// A new replica holding `document`, written in one creating transaction of `origin` (a
    /// `system` one: never an undo step). The view is `document` itself, with its local ids.
    pub fn create(document: &Arc<Document>, origin: &Origin) -> Result<Self> {
        let mut blobs = MemoryBlobs::default();
        let mut ids = Ids::default();
        let (mut replica, ()) = Replica::create_with(LayoutModel::schema(), DocId::new(), ActorId::random(), origin, |tx| {
            write_document(tx, document, &mut ids, &mut blobs).map(|_| ())
        })
        .map_err(kit_err)?;
        replica.take_changed();
        Ok(Self { replica, model: LayoutModel::from_parts(document.clone(), ids), blobs, group: None, blob_log: Vec::new() })
    }

    /// Opens a replica from a snapshot (a replica's, which keeps its line of changes, or a file's
    /// content, which starts a new one) as a replica of a fresh actor; `blobs` holds what it
    /// references. The state is repaired once (a `system` transaction, only when there is
    /// something to repair), so a file written by another program opens consistent.
    pub fn open(snapshot: &[u8], blobs: MemoryBlobs) -> Result<Self> {
        Self::open_with_version(snapshot, blobs).map(|(session, _)| session)
    }

    /// [`LayoutSession::open`], also answering the version the snapshot holds (before the
    /// repair), which is what the file holds.
    pub fn open_with_version(snapshot: &[u8], blobs: MemoryBlobs) -> Result<(Self, Version)> {
        let mut replica = Replica::load(LayoutModel::schema(), snapshot, ActorId::random()).map_err(kit_err)?;
        let loaded = replica.version();
        repair_all(&mut replica)?;
        replica.take_changed();
        let model = LayoutModel::load(&replica, &blobs)?;
        Ok((Self { replica, model, blobs, group: None, blob_log: Vec::new() }, loaded))
    }

    /// The view: the document as of the last record, undo, redo or import.
    pub fn document(&self) -> &Arc<Document> {
        self.model.document()
    }

    pub fn model(&self) -> &LayoutModel {
        &self.model
    }

    /// Writes what differs between the view and `after` as one transaction of `origin`, then makes
    /// `after` the view. Answers how many undo steps that made: 1, or 0 when nothing replicated
    /// changed, `origin` is not an undoable one, or the transaction joined an open undo group that
    /// already is a step. On failure nothing is written and the view is unchanged.
    pub fn record(&mut self, origin: &Origin, after: &Arc<Document>) -> Result<u32> {
        if self.group.as_ref().is_some_and(|group| undoable(origin) && group.principal != origin.principal) {
            self.end_undo_group();
        }
        let mut ids = self.model.ids().clone();
        let before = self.replica.version();
        let had: HashSet<String> = self.blobs.0.keys().cloned().collect();
        let (model, blobs) = (&self.model, &mut self.blobs);
        let result = self.replica.transact(origin, |tx| write_changes(tx, model.document(), after, &mut ids, blobs).map(|_| ()));
        self.replica.take_changed();
        result.map_err(kit_err)?;
        self.model.adopt(after.clone(), ids);
        let wrote = self.replica.version() != before;
        let added: Vec<String> = self.blobs.0.keys().filter(|hash| !had.contains(*hash)).cloned().collect();
        self.log_added(added);
        if !wrote || !undoable(origin) {
            return Ok(0);
        }
        Ok(match &mut self.group {
            None => 1,
            Some(group) if !group.counted => {
                group.counted = true;
                1
            }
            Some(_) => 0,
        })
    }

    /// Imports a reread file's document (`after`) into this replica as one `import` transaction,
    /// then makes it the view. Only what differs between the view and `after` is written, so
    /// every surviving object keeps its identity and unchanged values stay as they are; an
    /// `import` transaction is never an undo step, so both scopes keep their undo and redo steps.
    /// On failure nothing is written and the view is unchanged.
    pub fn reread(&mut self, after: &Arc<Document>) -> Result<()> {
        self.end_undo_group();
        let mut ids = self.model.ids().clone();
        let had: HashSet<String> = self.blobs.0.keys().cloned().collect();
        let (model, blobs) = (&self.model, &mut self.blobs);
        let origin = Origin::new(OriginKind::Import, "file reread");
        let result = self.replica.transact(&origin, |tx| write_changes(tx, model.document(), after, &mut ids, blobs).map(|_| ()));
        self.replica.take_changed();
        result.map_err(kit_err)?;
        self.model.adopt(after.clone(), ids);
        let added: Vec<String> = self.blobs.0.keys().filter(|hash| !had.contains(*hash)).cloned().collect();
        self.log_added(added);
        Ok(())
    }

    /// Starts an undo group for `principal` (ending any open one first): its transactions until
    /// [`LayoutSession::end_undo_group`] undo as one step.
    pub fn begin_undo_group(&mut self, principal: &str) -> Result<()> {
        self.end_undo_group();
        self.replica.begin_undo_group().map_err(kit_err)?;
        self.group = Some(Group { principal: principal.to_owned(), counted: false });
        Ok(())
    }

    pub fn end_undo_group(&mut self) {
        self.replica.end_undo_group();
        self.group = None;
    }

    /// Whether an undo group is open.
    pub fn grouping(&self) -> bool {
        self.group.is_some()
    }

    /// Undoes this replica's last undo step in `scope` (never an import or a repair, and never the
    /// other scope's steps, Kition platform KitModel 1.1 M12a), repairs the result and patches the
    /// view. Answers whether there was one, and the damage.
    pub fn undo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        self.step_in(scope, true).map(|(outcome, damage)| (outcome.is_done(), damage))
    }

    pub fn redo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        self.step_in(scope, false).map(|(outcome, damage)| (outcome.is_done(), damage))
    }

    /// [`LayoutSession::undo_in`] (`undo`) or [`LayoutSession::redo_in`], answering what the
    /// replica took off its stack ([`UndoOutcome`]: the steps it passed over, which changed
    /// nothing any more, and the one it took back), so a history kept beside the replica's (the
    /// engine's step labels) stays in step with it.
    pub fn step_in(&mut self, scope: UndoScope, undo: bool) -> Result<(UndoOutcome, Damage)> {
        let possible = if undo { self.can_undo_in(scope) } else { self.can_redo_in(scope) };
        if !possible {
            return Ok((UndoOutcome::default(), Damage::None));
        }
        self.group = None;
        let outcome = if undo { self.replica.undo_in(scope) } else { self.replica.redo_in(scope) }.map_err(kit_err)?;
        let damage = self.settle()?;
        Ok((outcome, damage))
    }

    pub fn can_undo_in(&self, scope: UndoScope) -> bool {
        self.replica.can_undo_in(scope)
    }

    pub fn can_redo_in(&self, scope: UndoScope) -> bool {
        self.replica.can_redo_in(scope)
    }

    /// [`LayoutSession::undo_in`] the person's scope.
    pub fn undo(&mut self) -> Result<(bool, Damage)> {
        self.undo_in(UndoScope::Person)
    }

    /// [`LayoutSession::redo_in`] the person's scope.
    pub fn redo(&mut self) -> Result<(bool, Damage)> {
        self.redo_in(UndoScope::Person)
    }

    pub fn can_undo(&self) -> bool {
        self.can_undo_in(UndoScope::Person)
    }

    pub fn can_redo(&self) -> bool {
        self.can_redo_in(UndoScope::Person)
    }

    /// Imports remote or stored changes (their blobs added first, [`LayoutSession::add_blobs`]),
    /// ending any open undo group first, then repairs the merged state and patches the view. Only
    /// the repair runs: no derived edit (autoflow, Smart Text Reflow) follows a merge (design note
    /// 4.4).
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage> {
        self.end_undo_group();
        self.replica.import_changes(bytes).map_err(kit_err)?;
        self.settle()
    }

    /// Takes a draft's changes (made on a replica of this one's line, an agent's draft) in as one
    /// transaction of `origin` with the repair of the merged state in it
    /// (`Replica::merge_changes_repaired`, [`repair::repair_in`]): one undo step of the **person**
    /// (KitModel M12a, also for an agent's `origin`: the person accepted it) unless `origin` is a
    /// `system` or `import` one, and one undo of it gives back the exact state before the merge.
    /// The edits made here since the draft began stay. Patches the view; add the draft's blobs
    /// first ([`LayoutSession::add_blobs`]). Answers the undo scope the merge's step was recorded
    /// in (`DraftMerge::scope`; `None` for no step: also when the draft, repaired, changes
    /// nothing) and the damage. A refused merge (also one whose repair fails) changes nothing.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<(Option<UndoScope>, Damage)> {
        self.end_undo_group();
        let before = self.replica.version();
        let merged = self.replica.merge_changes_repaired(bytes, origin, |tx, _| repair::repair_in(tx));
        let merged = match merged {
            Ok(merged) => merged,
            Err(error) => {
                self.replica.take_changed();
                return Err(kit_err(error));
            }
        };
        self.took_in(&before, merged.scope)
    }

    /// What merging a draft's changes would change in the document now, the repair included as
    /// [`LayoutSession::merge_changes`] runs it (`Replica::compare_changes_repaired`), so a draft's
    /// preview is what accepting it gives. Nothing changes; refuses what the merge refuses.
    pub fn compare_changes(&self, bytes: &[u8]) -> Result<StateDiff> {
        self.replica.compare_changes_repaired(bytes, |tx, _| repair::repair_in(tx)).map_err(kit_err)
    }

    /// The canonical state merging a draft's changes would give, the repair included
    /// (`Replica::merged_state_repaired`); nothing changes. Refuses what the merge refuses.
    pub fn merged_state(&self, bytes: &[u8]) -> Result<serde_json::Value> {
        self.replica.merged_state_repaired(bytes, |tx, _| repair::repair_in(tx)).map_err(kit_err)
    }

    /// Writes the state `snapshot` holds (a snapshot or content of this document, as a host's
    /// checkpoint keeps it) over the replica as one transaction of `origin`, keyed by object id
    /// (`kition_kit::session::write_state`), so every object the two share keeps its id, with the
    /// repair of the restored state in it ([`repair::repair_in`]): one undo step of its scope unless
    /// `origin` is a `system` or `import` one, which gives back the exact state before. Nothing
    /// changes, and no step is recorded, when the states are equal or the repaired restore leaves
    /// the document as it was. Refuses another document's snapshot. Add the blobs it references
    /// first. Answers the step's scope (`None` for none) and the damage.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<(Option<UndoScope>, Damage)> {
        self.end_undo_group();
        let schema = LayoutModel::schema();
        let loaded = Replica::load(Arc::clone(&schema), snapshot, ActorId::random()).map_err(kit_err)?;
        if loaded.doc_id() != self.replica.doc_id() {
            return Err(kit_err(KdmError::WrongDocument { expected: self.replica.doc_id().to_string(), found: loaded.doc_id().to_string() }));
        }
        let (now, then) = (kition_kit::session::state(&self.replica)?, kition_kit::session::state(&loaded)?);
        if now == then {
            return Ok((None, Damage::None));
        }
        let before = self.replica.version();
        let result = transact_unless_unchanged(&mut self.replica, origin, |tx| {
            kition_kit::session::write_state(tx, &schema, &now, &then)?;
            repair::repair_in(tx).map_err(kit_err)
        });
        if let Err(error) = result {
            self.replica.take_changed();
            return Err(error);
        }
        self.took_in(&before, UndoScope::of(origin.kind))
    }

    /// After a merge or a restore wrote one transaction (with its repair in it) since `before`, an
    /// undo step of `scope` if any: patches the view as an import does (no repair of its own: it
    /// ran inside). Answers the step's scope, `None` when nothing was written.
    fn took_in(&mut self, before: &Version, scope: Option<UndoScope>) -> Result<(Option<UndoScope>, Damage)> {
        let wrote = self.replica.version() != *before;
        let referenced_now = referenced(&self.replica);
        let changed = self.replica.take_changed();
        let damage = self.refresh(&changed)?;
        if wrote {
            let added: Vec<String> = referenced_now.into_iter().filter(|hash| self.blobs.0.contains_key(hash)).collect();
            self.log_added(added);
        }
        Ok((scope.filter(|_| wrote), damage))
    }

    /// The blobs a peer at `since` needs to reach this session's state: the hashes logged for a
    /// version `since` does not include, still in the store. An empty `since` answers every blob
    /// the current state references (a creating transaction or a loaded snapshot is never logged).
    pub fn blobs_since(&self, since: &Version) -> Vec<(String, Arc<[u8]>)> {
        if *since == Version::empty() {
            return referenced(&self.replica).into_iter().filter_map(|hash| self.blobs.0.get(&hash).cloned().map(|bytes| (hash, bytes))).collect();
        }
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for (version, hashes) in &self.blob_log {
            if since.includes(version) {
                continue;
            }
            for hash in hashes {
                if seen.insert(hash.clone())
                    && let Some(bytes) = self.blobs.0.get(hash)
                {
                    out.push((hash.clone(), bytes.clone()));
                }
            }
        }
        out
    }

    /// The blobs the current state references (what a file needs).
    pub fn referenced_blobs(&self) -> Vec<(String, Arc<[u8]>)> {
        self.blobs_since(&Version::empty())
    }

    fn log_added(&mut self, added: Vec<String>) {
        if !added.is_empty() {
            self.blob_log.push((self.replica.version(), added));
        }
    }

    pub fn export_changes(&self, since: &Version) -> Result<Vec<u8>> {
        self.replica.export_changes(since).map_err(kit_err)
    }

    pub fn version(&self) -> Version {
        self.replica.version()
    }

    /// The document's state with its version ([`Replica::snapshot`]): for the host's history
    /// store and new replicas on the same line of changes, never for a person's file.
    pub fn snapshot(&self) -> Result<Vec<u8>> {
        self.replica.snapshot().map_err(kit_err)
    }

    /// Takes `file` (a session opened from a `.klayout` file of the same document) into this one,
    /// the replica the host stored with the document's history (kition-sdk data model 7.3,
    /// `kition_kit::session::take_file`): nothing when this replica holds the file's state;
    /// otherwise the file's state, written by object id as one `system` transaction labelled
    /// `import`. The file's blobs come
    /// first. Then the result is repaired and the view patched, as after an import. Refuses a file
    /// of another document.
    pub fn take_file(&mut self, file: &LayoutSession) -> Result<Damage> {
        self.end_undo_group();
        self.add_blobs(file.blobs())?;
        kition_kit::session::take_file(&mut self.replica, &file.replica)?;
        self.settle()
    }

    /// SHA-256 of the replicated state: equal on replicas that hold the same state.
    pub fn state_hash(&self) -> Result<String> {
        self.replica.state_hash().map_err(kit_err)
    }

    pub fn doc_id(&self) -> DocId {
        self.replica.doc_id()
    }

    pub fn replica(&self) -> &Replica {
        &self.replica
    }

    /// Every blob the store holds (what the replica references, and what undo or a pending import
    /// can still bring back).
    pub fn blobs(&self) -> &MemoryBlobs {
        &self.blobs
    }

    /// Adds blobs (a peer's, before importing its changes). Answers whether the view changed:
    /// blobs the state already references that were missing (they came after their changes)
    /// reload the images that show them.
    pub fn add_blobs(&mut self, blobs: &MemoryBlobs) -> Result<bool> {
        let needed = referenced(&self.replica);
        let mut late = false;
        let mut added = Vec::new();
        for (hash, bytes) in &blobs.0 {
            if !self.blobs.0.contains_key(hash) {
                late |= needed.contains(hash);
                self.blobs.0.insert(hash.clone(), bytes.clone());
                added.push(hash.clone());
            }
        }
        self.log_added(added);
        if !late {
            return Ok(false);
        }
        let assets: BTreeSet<ObjectId> = self.replica.objects_of_type("Asset").into_iter().collect();
        self.refresh(&assets)?;
        Ok(true)
    }

    /// Repairs what an import, undo or redo left, then patches the view with both. The view
    /// follows the replica even if the repair fails.
    fn settle(&mut self) -> Result<Damage> {
        let mut changed = self.replica.take_changed();
        let repaired = if changed.is_empty() { Ok(()) } else { repair_all(&mut self.replica) };
        changed.extend(self.replica.take_changed());
        let damage = self.refresh(&changed)?;
        repaired?;
        Ok(damage)
    }

    fn refresh(&mut self, changed: &BTreeSet<ObjectId>) -> Result<Damage> {
        if changed.is_empty() {
            return Ok(Damage::None);
        }
        let blobs: &dyn BlobSource = &self.blobs;
        self.model.apply(&self.replica, changed, blobs)
    }
}

/// Every repair ([`repair::repair_in`]) in one `system` transaction labelled `repair` (written only
/// when there is something to repair): after an import, an undo or a redo, and when a replica
/// opens.
fn repair_all(replica: &mut Replica) -> Result<()> {
    replica.transact(&Origin::system("repair"), repair::repair_in).map_err(kit_err)
}
