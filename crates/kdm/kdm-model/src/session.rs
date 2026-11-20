//! [`LayoutSession`]: one document's replica, its [`LayoutModel`] view and its blobs, for the
//! engine's command bridge (design note section 4; task W3-22 DE3).
//!
//! It is `kition_kit::session::KitSession<LayoutModel>`'s flow on concrete, `Send` parts, as Kition
//! Vector's and Photo's sessions are: transactions with an origin, the replica's undo, the change
//! exchange, the repair after a merge, an undo or a redo, and the view patched from the replica.
//! The engine enumerates and ships the image blobs, which `KitSession`'s boxed blob store does not
//! allow. Edits are diff-bridged: the engine changes its own [`Document`] and
//! [`LayoutSession::record`] writes what changed as one transaction ([`write_changes`]), then
//! adopts it as the view, so no command needs rewriting (E12).
//!
//! **Repair.** After every import, undo and redo the session runs [`KitModel::validate`] (rules
//! V1–V10, V12), the read repair ([`repair::read_repairs`], V6) and the engine ids
//! ([`repair::store_numbers`]) in `system` transactions labelled `repair`, never undo steps; every
//! replica writes the same repair for the same state.
//! Opening a file repairs once too, so a file a merge left inconsistent opens consistent.
//!
//! **Undo groups per author.** [`LayoutSession::begin_undo_group`] names the principal it is for:
//! a transaction of another principal ends the group first, so a person's gesture never takes in
//! an agent's edit (or the other way round).

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, OriginKind, Replica, Tx, UndoScope, Version};
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

/// [`Replica::transact`] with a kit error kept aside: the KDM's closure must fail with a
/// [`KdmError`].
fn transact(replica: &mut Replica, origin: &Origin, f: impl FnOnce(&mut Tx<'_>) -> Result<bool>) -> Result<bool> {
    let mut kit_error = None;
    let result = replica.transact(origin, |tx| {
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
        (result, None) => result.map_err(kit_err),
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

    /// Opens a replica from a snapshot (or a full change export) and the changes logged after it,
    /// in order, as a new replica of a fresh actor; `blobs` holds what they reference. The state
    /// is repaired once (a `system` transaction, only when there is something to repair), so a
    /// file a merge left inconsistent opens consistent.
    pub fn open(snapshot: &[u8], changes: &[Vec<u8>], blobs: MemoryBlobs) -> Result<Self> {
        Self::open_with_version(snapshot, changes, blobs).map(|(session, _)| session)
    }

    /// [`LayoutSession::open`], also answering the version the snapshot and changes add up to
    /// (before the repair), which is what the file holds.
    pub fn open_with_version(snapshot: &[u8], changes: &[Vec<u8>], blobs: MemoryBlobs) -> Result<(Self, Version)> {
        let mut replica = Replica::load(LayoutModel::schema(), snapshot, ActorId::random()).map_err(kit_err)?;
        for change in changes {
            replica.import_changes(change).map_err(kit_err)?;
        }
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
        if !self.can_undo_in(scope) {
            return Ok((false, Damage::None));
        }
        self.group = None;
        let done = self.replica.undo_in(scope).map_err(kit_err)?;
        let damage = self.settle()?;
        Ok((done, damage))
    }

    pub fn redo_in(&mut self, scope: UndoScope) -> Result<(bool, Damage)> {
        if !self.can_redo_in(scope) {
            return Ok((false, Damage::None));
        }
        self.group = None;
        let done = self.replica.redo_in(scope).map_err(kit_err)?;
        let damage = self.settle()?;
        Ok((done, damage))
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

    pub fn snapshot(&self) -> Result<Vec<u8>> {
        self.replica.snapshot().map_err(kit_err)
    }

    /// SHA-256 of the replicated state: equal on replicas that converged.
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

/// The validation rules, the read repair, then the engine ids, each in a `system` transaction
/// labelled `repair` (written only when there is something to repair).
fn repair_all(replica: &mut Replica) -> Result<()> {
    let everything = BTreeSet::new();
    transact(replica, &Origin::system("repair"), |tx| LayoutModel::validate(tx, &everything))?;
    let repairs = repair::read_repairs(replica).map_err(kit_err)?;
    if !repairs.is_empty() {
        transact(replica, &Origin::system("repair"), |tx| repair::apply(tx, &repairs).map_err(kit_err))?;
    }
    let numbers = crate::read::number_repairs(replica).map_err(kit_err)?;
    if !numbers.is_empty() {
        transact(replica, &Origin::system("repair"), |tx| repair::store_numbers(tx, &numbers).map_err(kit_err))?;
    }
    Ok(())
}
