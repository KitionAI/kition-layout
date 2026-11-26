//! The command bridge to each document's KDM replica (engine contract 2; design note section 4,
//! task W3-22 DE3).
//!
//! A document's replica ([`LayoutSession`]) is its shared source of truth and its undo history;
//! [`DocState::doc`] is the engine's own view of it. Commands, tools and keys run as before on the
//! in-memory [`Document`]; when a top-level call ends ([`Session::execute`] at depth 0, or a tool
//! interaction's commit) the bridge writes what changed since the replica last saw it
//! ([`Link::synced`]) as one transaction: a person's (or the agent's, through
//! [`Session::execute_as`]) when the command is undoable, else a `system` one. Nested commands, a
//! script's steps and the derived edits a command makes after itself (Smart Text Reflow, live
//! captions, placeholders) are all in that one transaction, so they undo together and never run
//! after a merge (design note 4.4: derived edits only inside the local command that caused them).
//!
//! **Undo is the replica's** (M12): the engine keeps no history of its own. Undo and redo step the
//! replica and take its view (with every collaborator's edit since) as the document; the selection
//! keeps what still exists. A data-merge preview and a live interaction are never written: the
//! preview is session state, and an interaction is written once, when it commits (one step per
//! gesture). An import during an interaction rebases it on the merged document and replays its
//! preview.
//!
//! The replica is made lazily (the first edit, or when asked for), unless the document came from a
//! `.klayout` file: then the replica the file's content loads as (or the host's stored replica the
//! file was taken into) is the document's. A `.klayout` file holds the document's content, not its
//! history (kition-sdk data model 7.3, rule M16b): [`DocState::klayout_file`] writes the replica's
//! content ([`kition_layout_kdm::file`]); the change log stays with the replica (and the host's
//! history store).
//! A replica that refuses a write or an undo is left ([`DocState::kdm_resets`]); the next edit
//! makes a fresh one, which collaborators must join from its snapshot.

use std::sync::Arc;

use kition_layout_doc::{Document, Selection};
use kition_layout_kdm::kition_kit::kdm::{Origin, UndoScope, Version};
use kition_layout_kdm::kition_kit::session::{Damage, MemoryBlobs};
use kition_layout_kdm::{FileBase, LayoutSession};

use crate::{DocState, EngineError, Result, Session};

/// The origin of the local person's commands (when nothing else was set with
/// [`Session::execute_as`]).
pub fn local_person() -> Origin {
    Origin::person("person:local")
}

fn kdm_error(error: impl std::fmt::Display) -> EngineError {
    EngineError::Other(format!("document replica: {error}"))
}

/// One document's bridge state.
#[derive(Debug)]
pub struct Link {
    /// The replica; made lazily, `None` again after a reset.
    pub(crate) kdm: Option<LayoutSession>,
    /// The document as last written to (or read from) the replica.
    pub(crate) synced: Arc<Document>,
    /// What the document's `.klayout` file holds besides its content (its docId and the entries
    /// this build does not know).
    pub(crate) file: Option<FileBase>,
    /// The labels of the undo and redo steps of the person's scope and of the agents'
    /// ([`slot`]), newest last (for Edit › Undo's name and `file.history`); the replica
    /// decides what undo does (Kition platform KitModel 1.1 M12a: each scope undoes its own).
    pub(crate) labels: [Labels; 2],
    /// The replica's state hash when the document was last saved, so an undo back to it is clean.
    pub(crate) saved_hash: Option<String>,
    pub(crate) resets: u64,
}

/// One undo scope's step labels.
#[derive(Clone, Debug, Default)]
pub(crate) struct Labels {
    pub(crate) undo: Vec<String>,
    pub(crate) redo: Vec<String>,
}

fn slot(scope: UndoScope) -> usize {
    match scope {
        UndoScope::Person => 0,
        UndoScope::Agent => 1,
    }
}

/// The undo scope `origin`'s steps join (KitModel 1.1 M12a): the agents' for an agent, else the
/// person's.
pub(crate) fn scope_of(origin: &Origin) -> UndoScope {
    UndoScope::of(origin.kind).unwrap_or(UndoScope::Person)
}

impl Link {
    fn labels_mut(&mut self, scope: UndoScope) -> &mut Labels {
        let [person, agent] = &mut self.labels;
        if slot(scope) == 0 { person } else { agent }
    }

    fn labels(&self, scope: UndoScope) -> &Labels {
        let [person, agent] = &self.labels;
        if slot(scope) == 0 { person } else { agent }
    }

    pub(crate) fn new(doc: &Arc<Document>) -> Self {
        Self { kdm: None, synced: doc.clone(), file: None, labels: Default::default(), saved_hash: None, resets: 0 }
    }
}

/// A clone holds no replica (a replica has one owner) and no file: it makes a fresh replica on its
/// next edit.
impl Clone for Link {
    fn clone(&self) -> Self {
        Self { resets: self.resets, ..Self::new(&self.synced) }
    }
}

/// The replica's view as the engine's document: with an id counter never below `local`'s and the
/// font scope `local` has (neither is replicated).
fn view_of(kdm: &LayoutSession, local: &Document) -> Arc<Document> {
    let view = kdm.document();
    if view.font_scope == local.font_scope && view.next_id >= local.next_id {
        return view.clone();
    }
    let mut d = (**view).clone();
    d.font_scope = local.font_scope;
    d.next_id = d.next_id.max(local.next_id);
    Arc::new(d)
}

/// Keeps of `selection` what `doc` still has: items, a text caret or range (clamped to the story,
/// on a character boundary), table cells.
pub(crate) fn prune_selection(selection: &mut Selection, doc: &Document) {
    selection.items.retain(|id| doc.item(*id).is_some());
    selection.anchors.retain(|(id, _, _)| doc.item(*id).is_some());
    if selection.key.is_some_and(|id| doc.item(id).is_none()) {
        selection.key = None;
    }
    if let Some(text) = &mut selection.text {
        match doc.story(text.story) {
            Some(story) => {
                let clamp = |at: usize| {
                    let mut at = at.min(story.text.len());
                    while at > 0 && !story.text.is_char_boundary(at) {
                        at -= 1;
                    }
                    at
                };
                text.anchor = clamp(text.anchor);
                text.focus = clamp(text.focus);
                if text.frame.is_some_and(|f| doc.item(f).is_none()) {
                    text.frame = None;
                }
                if let Some(cell) = &text.cell
                    && !story.tables.contains_key(&cell.table)
                {
                    selection.text = None;
                }
            }
            None => selection.text = None,
        }
    }
    if let Some(cells) = &selection.cells
        && doc.story(cells.story).is_none_or(|story| !story.tables.contains_key(&cells.table))
    {
        selection.cells = None;
    }
}

impl DocState {
    /// The document from a `.klayout` file, with the replica its content loads as as its own.
    pub fn open_klayout(bytes: &[u8], path: Option<String>) -> Result<Self> {
        let (kdm, base) = kition_layout_kdm::open_file(bytes).map_err(|error| EngineError::Other(error.to_string()))?;
        Ok(Self::with_kdm(kdm, Some(base), path))
    }

    /// The document `kdm` holds, clean, with `kdm` as its replica and `file` as what its file
    /// holds (a replica opened from a file, or the host's stored replica a file was taken into).
    pub fn with_kdm(kdm: LayoutSession, file: Option<FileBase>, path: Option<String>) -> Self {
        let doc = (**kdm.document()).clone();
        let mut st = DocState::new(doc, path);
        st.link.synced = kdm.document().clone();
        st.doc = st.link.synced.clone();
        st.saved_doc = st.doc.clone();
        st.link.saved_hash = kdm.state_hash().ok();
        st.link.kdm = Some(kdm);
        st.link.file = file;
        st
    }

    /// The document's replica, if it has made one.
    pub fn kdm(&self) -> Option<&LayoutSession> {
        self.link.kdm.as_ref()
    }

    /// How many times the document left its replica (a write or an undo the replica refused).
    pub fn kdm_resets(&self) -> u64 {
        self.link.resets
    }

    /// What the document's `.klayout` file holds, when it came from one or was saved as one.
    pub fn file_base(&self) -> Option<&FileBase> {
        self.link.file.as_ref()
    }

    /// Whether the person can undo (their own steps, KitModel 1.1 M12a).
    pub fn can_undo(&self) -> bool {
        self.can_undo_in(UndoScope::Person)
    }

    pub fn can_redo(&self) -> bool {
        self.can_redo_in(UndoScope::Person)
    }

    pub fn can_undo_in(&self, scope: UndoScope) -> bool {
        self.interaction.is_none() && self.link.kdm.as_ref().is_some_and(|kdm| kdm.can_undo_in(scope))
    }

    pub fn can_redo_in(&self, scope: UndoScope) -> bool {
        self.interaction.is_none() && self.link.kdm.as_ref().is_some_and(|kdm| kdm.can_redo_in(scope))
    }

    /// The labels of the person's undo and redo steps, oldest first.
    pub fn history_labels(&self) -> (&[String], &[String]) {
        let labels = self.link.labels(UndoScope::Person);
        (&labels.undo, &labels.redo)
    }

    /// How many undo steps the person has.
    pub fn undo_steps(&self) -> usize {
        self.link.labels(UndoScope::Person).undo.len()
    }

    /// Whether the document changed since the replica last saw it (an edit not written yet).
    pub(crate) fn unsynced(&self) -> bool {
        !Arc::ptr_eq(&self.link.synced, &self.doc)
    }

    /// Writes what the document changed since the replica last saw it, as one transaction of
    /// `origin` when `undoable` (labelled `label` in the history), else of the system. Nothing is
    /// written during a live interaction or a data-merge preview (neither is the document's
    /// content yet).
    pub(crate) fn sync(&mut self, origin: &Origin, undoable: bool, label: &str) {
        if self.interaction.is_some() || self.preview_stash.is_some() || Arc::ptr_eq(&self.link.synced, &self.doc) {
            return;
        }
        let after = self.doc.clone();
        if self.link.kdm.is_none() {
            match LayoutSession::create(&self.link.synced, &Origin::system("create")) {
                Ok(kdm) => self.link.kdm = Some(kdm),
                Err(error) => {
                    log::warn!("the document can't be held in a replica: {error}");
                    self.link.synced = after;
                    return;
                }
            }
        }
        let origin = if undoable { origin.clone() } else { Origin::system("edit") };
        let Some(kdm) = self.link.kdm.as_mut() else { return };
        match kdm.record(&origin, &after) {
            Ok(steps) => {
                self.link.synced = after;
                if steps > 0 {
                    let labels = self.link.labels_mut(scope_of(&origin));
                    labels.undo.push(label.to_owned());
                    labels.redo.clear();
                }
            }
            Err(error) => {
                log::warn!("the document replica refused an edit: {error}");
                self.detach_kdm();
            }
        }
    }

    /// The replica, made now from the document if there is none yet (holding it in its creating
    /// transaction); a direct write not written yet is written first, as a `system` change.
    pub(crate) fn ensure_kdm(&mut self, origin: &Origin) -> Result<&mut LayoutSession> {
        if self.link.kdm.is_none() && self.interaction.is_none() && self.preview_stash.is_none() {
            let kdm = LayoutSession::create(&self.doc, &Origin::system("create")).map_err(kdm_error)?;
            self.link.synced = self.doc.clone();
            self.link.kdm = Some(kdm);
        }
        self.sync(origin, false, "");
        if self.link.kdm.is_none() {
            let kdm = LayoutSession::create(&self.link.synced, &Origin::system("create")).map_err(kdm_error)?;
            self.link.kdm = Some(kdm);
        }
        self.link.kdm.as_mut().ok_or_else(|| kdm_error("no replica"))
    }

    /// Leaves the replica: the undo history was the replica's, and a fresh one starts empty.
    pub(crate) fn detach_kdm(&mut self) {
        self.link.kdm = None;
        self.link.file = None;
        self.link.resets = self.link.resets.saturating_add(1);
        self.link.synced = self.interaction.as_ref().map_or_else(|| self.doc.clone(), |it| it.doc.clone());
        self.link.labels = Default::default();
    }

    /// The document follows its replica (after an undo, a redo or an import): the view becomes the
    /// document, the selection keeps what still exists, and the document is clean again when the
    /// replica is back at the saved state (`check_saved`).
    fn follow_kdm(&mut self, check_saved: bool) {
        let Some(kdm) = self.link.kdm.as_ref() else { return };
        let view = view_of(kdm, &self.doc);
        let back_at_saved = check_saved && self.link.saved_hash.is_some() && kdm.state_hash().ok() == self.link.saved_hash;
        self.link.synced = view.clone();
        self.doc = view;
        prune_selection(&mut self.selection, &self.doc);
        if self.doc.layer(self.active_layer).is_none() {
            self.active_layer = self.doc.default_layer();
        }
        self.revision = self.revision.saturating_add(1);
        if back_at_saved {
            self.saved_doc = self.doc.clone();
        }
    }

    /// Steps the replica back (`undo`) or forward and takes its view. Answers the step's label
    /// (`None` when there was nothing to step).
    pub(crate) fn step(&mut self, undo: bool, scope: UndoScope) -> Result<Option<String>> {
        let Some(kdm) = self.link.kdm.as_mut() else { return Ok(None) };
        match kdm.step_in(scope, undo) {
            Ok((outcome, _)) => {
                // The steps the replica passed over (they changed nothing any more) left its stack
                // with the one it took back: their labels go too, so the two stay in step.
                let labels = self.link.labels_mut(scope);
                let (from, to) = if undo { (&mut labels.undo, &mut labels.redo) } else { (&mut labels.redo, &mut labels.undo) };
                for _ in 0..outcome.passed.len() {
                    from.pop();
                }
                if !outcome.is_done() {
                    if !outcome.passed.is_empty() {
                        self.follow_kdm(true);
                    }
                    return Ok(None);
                }
                let label = from.pop().unwrap_or_else(|| "Edit".to_owned());
                to.push(label.clone());
                self.follow_kdm(true);
                Ok(Some(label))
            }
            Err(error) => {
                // The replica stepped but its view or repair failed: follow what it holds if it
                // can be read, else leave it.
                log::warn!("the document replica refused an undo or redo: {error}");
                self.detach_kdm();
                Err(kdm_error(error))
            }
        }
    }

    /// A clean reread of the document's file: `fresh` (the document read from it again) is
    /// written into this document's replica as one `import` transaction holding only the file
    /// difference ([`LayoutSession::reread`]), so object identities, the selection and both undo
    /// histories are kept; the document is then clean. Refused, with nothing changed, during a
    /// live interaction or a data-merge preview, and when the document has unsaved edits.
    pub fn reconcile_file(&mut self, fresh: &Arc<Document>) -> Result<()> {
        if self.interaction.is_some() || self.preview_stash.is_some() {
            return Err(EngineError::Other("the document is in a live gesture or data preview; finish it before a reread".to_owned()));
        }
        if self.is_dirty() {
            return Err(EngineError::Other("the document has unsaved edits; a reread would discard them".to_owned()));
        }
        let kdm = self.ensure_kdm(&local_person())?;
        kdm.reread(fresh).map_err(kdm_error)?;
        self.follow_kdm(false);
        self.mark_saved();
        Ok(())
    }

    /// The document as a `.klayout` file and what the file then holds: the replica's content
    /// (made now if there is none yet). The caller records the base with [`DocState::saved_as`]
    /// once the bytes are stored.
    pub fn klayout_file(&mut self) -> Result<(Vec<u8>, FileBase)> {
        let file = self.link.file.clone();
        let kdm = self.ensure_kdm(&local_person())?;
        kition_layout_kdm::save_file(kdm, file.as_ref()).map(|saved| (saved.bytes, saved.base)).map_err(kdm_error)
    }

    /// The bytes of the document's content as a `.klayout` file without saving it (Save a Copy,
    /// `file.serialize`): the replica's content when it holds the document (no change, so the
    /// document stays as dirty or clean as it was); a fresh replica of `doc` otherwise. A data-merge preview's filled document is session state: the unfilled
    /// template is saved.
    pub fn klayout_bytes(&mut self) -> Result<Vec<u8>> {
        let content = self.preview_stash.clone().unwrap_or_else(|| self.doc.clone());
        match self.link.kdm.as_ref() {
            Some(kdm) if Arc::ptr_eq(&self.link.synced, &content) => {
                kition_layout_kdm::save_file(kdm, self.link.file.as_ref()).map(|saved| saved.bytes).map_err(kdm_error)
            }
            _ => klayout_of(&content),
        }
    }

    /// The document was saved as the file [`DocState::klayout_file`] answered `base` for.
    pub fn saved_as(&mut self, base: FileBase) {
        self.link.file = Some(base);
        self.mark_saved();
    }

    /// The current document is the saved one (dirty no more).
    pub fn mark_saved(&mut self) {
        self.saved_doc = self.doc.clone();
        self.saved_revision = self.revision;
        self.link.saved_hash = self.link.kdm.as_ref().and_then(|kdm| kdm.state_hash().ok());
    }
}

/// `doc` as a new `.klayout` file (a fresh replica: a copy, with its own document id).
pub fn klayout_of(doc: &Document) -> Result<Vec<u8>> {
    let kdm = LayoutSession::create(&Arc::new(doc.clone()), &Origin::system("create")).map_err(kdm_error)?;
    kition_layout_kdm::save_file(&kdm, None).map(|saved| saved.bytes).map_err(kdm_error)
}

/// `doc` written over the `.klayout` file `existing` (the same document: its docId and object ids
/// kept, its content now `doc`'s); a new file when `existing` is not a `.klayout` file.
pub fn klayout_over(existing: Option<&[u8]>, doc: &Document) -> Result<Vec<u8>> {
    let Some(Ok((mut kdm, base))) = existing.filter(|bytes| kition_layout_kdm::is_klayout(bytes)).map(kition_layout_kdm::open_file) else {
        return klayout_of(doc);
    };
    kdm.record(&local_person(), &Arc::new(doc.clone())).map_err(kdm_error)?;
    kition_layout_kdm::save_file(&kdm, Some(&base)).map(|saved| saved.bytes).map_err(kdm_error)
}

/// Typing in progress ([`Session::type_text`]): the document whose replica holds the open undo
/// group, and whether the text typed last ended in white space (the next word starts a new step).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Typing {
    uid: u64,
    after_space: bool,
}

impl Session {
    /// Types `text` at the caret (`text.insert`, as the Type tool does) as part of one undo step
    /// with the text typed just before: a word and the white space after it. The next word, a
    /// pause ([`Session::end_typing`]), any other edit, a click, a key, a tool change or an undo
    /// ends the step. Without a replica (or during a live gesture) each insert is a step of its own.
    pub fn type_text(&mut self, text: &str) -> Result<serde_json::Value> {
        // An input method's committed text takes the place of its marked text.
        self.ime_cancel();
        let starts_word = text.chars().next().is_some_and(|c| !c.is_whitespace());
        let uid = self.active().map(|st| st.uid);
        let joins = matches!(self.typing, Some(typing) if Some(typing.uid) == uid && !(typing.after_space && starts_word));
        if !joins {
            self.end_typing();
            let origin = self.origin();
            if let (Some(uid), Some(st)) = (uid, self.active_mut())
                && st.interaction.is_none()
                && let Ok(kdm) = st.ensure_kdm(&origin)
                && kdm.begin_undo_group(&origin.principal).is_ok()
            {
                self.typing = Some(Typing { uid, after_space: false });
            }
        }
        self.typing_now = true;
        let result = self.execute("text.insert", &serde_json::json!({ "text": text }));
        self.typing_now = false;
        match (&result, self.typing.as_mut()) {
            (Ok(_), Some(typing)) => typing.after_space = text.chars().last().is_some_and(char::is_whitespace),
            _ => self.end_typing(),
        }
        result
    }

    /// Runs `steps` (command, params) as one command: one undo step named `label` (Swap Fill and
    /// Stroke is a fill and a stroke). When a step fails the document goes back to how it was
    /// and the error is answered. Inside another command, the steps are just part of it.
    pub fn execute_steps(&mut self, label: &str, steps: &[(String, serde_json::Value)]) -> Result<Vec<serde_json::Value>> {
        if self.depth > 0 {
            return steps.iter().map(|(id, params)| self.execute(id, params)).collect();
        }
        if self.dialog_preview {
            self.cancel_preview();
        }
        self.end_typing();
        self.sync_all(None, "");
        let acting = self.active().map(|d| d.uid);
        let before = self.active().map(|d| (d.doc.clone(), d.selection.clone()));
        self.depth += 1;
        let mut answers = Vec::with_capacity(steps.len());
        let mut failed = None;
        for (id, params) in steps {
            match self.execute(id, params) {
                Ok(answer) => answers.push(answer),
                Err(error) => {
                    failed = Some(error);
                    break;
                }
            }
        }
        self.depth = self.depth.saturating_sub(1);
        if let Some(error) = failed {
            if let (Some((doc, selection)), Some(st)) = (before, self.active_mut())
                && Some(st.uid) == acting
            {
                st.doc = doc;
                st.selection = selection;
                st.revision = st.revision.saturating_add(1);
            }
            return Err(error);
        }
        let still = acting.filter(|uid| self.active().is_some_and(|d| d.uid == *uid));
        self.sync_all(still, label);
        Ok(answers)
    }

    /// Ends the typing in progress: what is typed next is a new undo step.
    pub fn end_typing(&mut self) {
        let Some(typing) = self.typing.take() else { return };
        if let Some(kdm) = self.docs.iter_mut().find(|st| st.uid == typing.uid).and_then(|st| st.link.kdm.as_mut()) {
            kdm.end_undo_group();
        }
    }

    /// Who the running (or next) command acts for: the origin of [`Session::execute_as`], or the
    /// local person.
    pub fn origin(&self) -> Origin {
        self.origin.clone().unwrap_or_else(local_person)
    }

    /// [`Session::execute`] as `origin` (a person, an agent): the replica records the edit as
    /// theirs.
    pub fn execute_as(&mut self, id: &str, params: &serde_json::Value, origin: &Origin) -> Result<serde_json::Value> {
        let previous = self.origin.replace(origin.clone());
        let result = self.execute(id, params);
        self.origin = previous;
        result
    }

    /// After a top-level call: writes every document's changes, as an undo step of the document
    /// `undoable` names (the one the command acted on), as `system` changes for the others (a
    /// merge writing its source's status while it makes a new document).
    pub(crate) fn sync_all(&mut self, undoable: Option<u64>, label: &str) {
        let origin = self.origin();
        for st in &mut self.docs {
            let step = undoable == Some(st.uid);
            st.sync(&origin, step, label);
        }
    }

    /// Edit › Undo (`undo`) or Redo on the active document's replica. Answers the step's label.
    /// In `scope` (engine rule E14a), else the caller's (KitModel 1.1 M12a).
    pub fn undo_redo(&mut self, undo: bool, scope: Option<UndoScope>) -> Result<Option<String>> {
        self.end_typing();
        self.cancel_interaction();
        self.stop_data_preview();
        let origin = self.origin();
        let scope = scope.unwrap_or_else(|| scope_of(&origin));
        let st = self.doc_mut()?;
        st.sync(&origin, true, "Edit");
        st.step(undo, scope)
    }

    /// The active document's replica, if it has made one (never makes one).
    pub fn kdm(&self) -> Option<&LayoutSession> {
        self.active()?.kdm()
    }

    /// The active document's KDM identity, `None` before it has made a replica.
    pub fn doc_id(&self) -> Option<String> {
        Some(self.kdm()?.doc_id().to_string())
    }

    fn active_kdm(&mut self) -> Result<&mut LayoutSession> {
        let origin = self.origin();
        self.doc_mut()?.ensure_kdm(&origin)
    }

    /// The active document's replica version.
    pub fn version(&mut self) -> Result<Version> {
        Ok(self.active_kdm()?.version())
    }

    /// The active document's replica as a snapshot, for a peer.
    pub fn snapshot(&mut self) -> Result<Vec<u8>> {
        self.active_kdm()?.snapshot().map_err(kdm_error)
    }

    /// SHA-256 of the active document's replicated state: equal on replicas that converged.
    pub fn state_hash(&mut self) -> Result<String> {
        self.active_kdm()?.state_hash().map_err(kdm_error)
    }

    /// The active document's changes since `since` (`Version::empty()` for all of them).
    pub fn export_changes(&mut self, since: &Version) -> Result<Vec<u8>> {
        self.active_kdm()?.export_changes(since).map_err(kdm_error)
    }

    /// Every blob the active document's replica holds.
    pub fn blobs(&mut self) -> Result<MemoryBlobs> {
        Ok(self.active_kdm()?.blobs().clone())
    }

    /// The blobs a peer at `since` needs (`Version::empty()`: every blob the state references).
    pub fn blobs_since(&mut self, since: &Version) -> Result<MemoryBlobs> {
        Ok(MemoryBlobs(self.active_kdm()?.blobs_since(since).into_iter().collect()))
    }

    /// Adds a peer's blobs (before importing its changes).
    pub fn add_blobs(&mut self, blobs: &MemoryBlobs) -> Result<()> {
        if self.active_kdm()?.add_blobs(blobs).map_err(kdm_error)? {
            self.doc_mut()?.follow_kdm(false);
        }
        Ok(())
    }

    /// Imports a collaborator's (or stored) changes into the active document's replica. Local
    /// edits not written yet are written first; a data-merge preview ends; a live interaction goes
    /// on from the merged document (its preview replayed). Only the replica's repair runs: no
    /// derived edit follows a merge (design note 4.4).
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage> {
        self.stop_data_preview();
        let origin = self.origin();
        let st = self.doc_mut()?;
        let kdm = st.ensure_kdm(&origin)?;
        let before = kdm.version();
        let result = kdm.import_changes(bytes);
        let changed = kdm.version() != before;
        if result.is_err() && changed {
            // Some of it went in but the view could not follow: leave the replica rather than
            // diff the next edit against a view it no longer holds.
            st.detach_kdm();
            return result.map_err(kdm_error);
        }
        let damage = result.map_err(kdm_error)?;
        if !changed {
            return Ok(damage);
        }
        let preview = st.interaction.as_ref().and_then(|it| it.preview.clone());
        let steps = st.interaction.as_ref().map(|it| it.steps.clone()).unwrap_or_default();
        match st.interaction.as_mut() {
            Some(_) => {
                let Some(kdm) = st.link.kdm.as_ref() else { return Ok(damage) };
                let view = view_of(kdm, &st.doc);
                st.link.synced = view.clone();
                if let Some(it) = st.interaction.as_mut() {
                    it.doc = view.clone();
                    prune_selection(&mut it.selection, &view);
                }
                st.doc = view;
                prune_selection(&mut st.selection, &st.doc);
                st.revision = st.revision.saturating_add(1);
            }
            None => st.follow_kdm(false),
        }
        if let Some((cmd, params)) = preview
            && self.preview(&cmd, params).is_err()
        {
            self.cancel_interaction();
        }
        if !steps.is_empty() && self.preview_many(&steps).is_err() {
            self.cancel_interaction();
        }
        Ok(damage)
    }

    /// Takes a draft's changes (an agent's draft, on this document's line of changes) into the
    /// active document as one undo step of `origin`'s scope, labelled `Merge Draft`: the
    /// document follows, as after an import. A live interaction and a data-merge preview end
    /// first; local edits not written yet are written first. A refused merge changes nothing.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<Damage> {
        self.take_in(origin, "Merge Draft", |kdm, origin| kdm.merge_changes(bytes, origin))
    }

    /// Writes the state `snapshot` holds (a checkpoint of this document) over the active
    /// document by object id as one undo step of `origin`'s scope, labelled `Restore Version`;
    /// the document follows. Nothing changes when the states are equal; another document's
    /// snapshot is refused.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<Damage> {
        self.take_in(origin, "Restore Version", |kdm, origin| kdm.restore_state(snapshot, origin))
    }

    fn take_in(
        &mut self,
        origin: &Origin,
        label: &str,
        write: impl FnOnce(&mut LayoutSession, &Origin) -> kition_layout_kdm::kition_kit::session::Result<(u32, Damage)>,
    ) -> Result<Damage> {
        self.cancel_interaction();
        self.stop_data_preview();
        let st = self.doc_mut()?;
        let kdm = st.ensure_kdm(origin)?;
        let before = kdm.version();
        let result = write(kdm, origin);
        let changed = kdm.version() != before;
        let (steps, damage) = match result {
            Ok(answer) => answer,
            Err(error) if changed => {
                // It went in but the view could not follow: leave the replica rather than diff
                // the next edit against a view it no longer holds.
                st.detach_kdm();
                return Err(kdm_error(error));
            }
            Err(error) => return Err(kdm_error(error)),
        };
        if steps > 0 {
            let labels = st.link.labels_mut(scope_of(origin));
            labels.undo.push(label.to_owned());
            labels.redo.clear();
        }
        if changed {
            st.follow_kdm(false);
        }
        Ok(damage)
    }

    /// Opens a replica (a snapshot or full change export, and the blobs it references) as a new
    /// document of a fresh actor, and makes it active: clean, with an empty history.
    pub fn open_snapshot(&mut self, snapshot: &[u8], blobs: MemoryBlobs) -> Result<usize> {
        let kdm = LayoutSession::open(snapshot, blobs).map_err(kdm_error)?;
        Ok(self.add_document(DocState::with_kdm(kdm, None, None)))
    }
}

#[cfg(test)]
mod tests {
    use kition_layout_doc::{ItemId, StoryId};
    use kition_layout_tools::{PointerEvent, PointerKind};
    use serde_json::json;

    use super::*;
    use crate::ViewInfo;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"pages": 1})).unwrap();
        s
    }

    fn frame(s: &mut Session, rect: [f64; 4]) -> u64 {
        s.execute("frame.add", &json!({"rect": rect, "content": "unassigned"})).unwrap()["id"].as_u64().unwrap()
    }

    fn items(s: &Session) -> usize {
        s.doc().unwrap().doc.spreads.iter().map(|sp| sp.items.len()).sum()
    }

    /// Kition platform KitModel 1.1 M12a: on one replica, the person's undo and an agent's undo
    /// each take back only their own steps and keep the other's.
    #[test]
    fn agent_edits_undo_in_their_own_scope() {
        let mut s = session();
        let agent = Origin::agent("agent:helper");
        frame(&mut s, [10.0, 10.0, 60.0, 60.0]);
        s.execute_as("frame.add", &json!({"rect": [100.0, 10.0, 160.0, 60.0], "content": "unassigned"}), &agent).unwrap();
        assert_eq!(items(&s), 2);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(items(&s), 1, "the person's undo takes the person's frame");
        assert!(s.execute("edit.undo", &json!({})).is_err(), "the person has nothing left: the agent's frame is not theirs");
        assert_eq!(items(&s), 1);
        s.execute_as("edit.undo", &json!({}), &agent).unwrap();
        assert_eq!(items(&s), 0, "the agent's undo takes the agent's frame");
    }

    /// M12a: an agent's undo of a step under the person's later edit keeps the person's edit, and
    /// `edit.undo`'s `scope` (engine rule E14a) names whose step to undo.
    #[test]
    fn an_agent_undo_under_the_persons_later_step_keeps_the_persons_edit() {
        let mut s = session();
        let agent = Origin::agent("agent:helper");
        s.execute_as("frame.add", &json!({"rect": [100.0, 10.0, 160.0, 60.0], "content": "unassigned"}), &agent).unwrap();
        let mine = frame(&mut s, [10.0, 10.0, 60.0, 60.0]);
        s.execute("edit.undo", &json!({"scope": "agent"})).unwrap();
        assert_eq!(items(&s), 1, "the agent's frame is undone");
        assert!(s.doc().unwrap().doc.item(ItemId(mine)).is_some(), "the person's later frame stays");
        s.execute_as("edit.redo", &json!({}), &agent).unwrap();
        assert_eq!(items(&s), 2, "the agent's redo brings its frame back");
        assert!(s.execute("edit.undo", &json!({"scope": "other"})).is_err(), "an unknown scope is refused");
    }

    /// A second session holding a replica of `a`'s active document.
    fn peer(a: &mut Session) -> Session {
        let mut b = Session::new();
        let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        b.open_snapshot(&snapshot, blobs).unwrap();
        b
    }

    fn exchange(from: &mut Session, to: &mut Session) {
        let since = to.version().unwrap();
        let changes = from.export_changes(&since).unwrap();
        to.add_blobs(&from.blobs_since(&since).unwrap()).unwrap();
        to.import_changes(&changes).unwrap();
    }

    #[test]
    fn undo_and_redo_are_the_replicas_and_back_at_the_save_is_clean() {
        let mut s = session();
        assert!(!s.doc().unwrap().can_undo());
        frame(&mut s, [72.0, 72.0, 144.0, 144.0]);
        s.doc_mut().unwrap().mark_saved();
        frame(&mut s, [200.0, 200.0, 300.0, 300.0]);
        assert!(s.doc().unwrap().is_dirty());
        assert_eq!(s.doc().unwrap().history_labels().0.len(), 2);
        let undone = s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(undone["undone"], "Create Frame");
        assert_eq!(items(&s), 1);
        assert!(!s.doc().unwrap().is_dirty(), "undone back to what was saved");
        s.execute("edit.redo", &json!({})).unwrap();
        assert_eq!(items(&s), 2);
        assert!(s.doc().unwrap().is_dirty());
        assert!(s.doc().unwrap().kdm().is_some_and(|kdm| kdm.can_undo()));
        let inspected = s.execute("file.inspect", &json!({})).unwrap();
        assert_eq!((inspected["canUndo"].as_bool(), inspected["canRedo"].as_bool()), (Some(true), Some(false)));
    }

    #[test]
    fn a_gesture_is_one_step_and_a_script_is_one_step() {
        let mut s = session();
        s.set_tool("rectangle");
        let v = ViewInfo::at_zoom(1.0);
        s.pointer(&PointerEvent::new(PointerKind::Down, 10.0, 10.0), v).unwrap();
        for i in 1..6 {
            s.pointer(&PointerEvent::new(PointerKind::Drag, 10.0 + f64::from(i) * 10.0, 20.0), v).unwrap();
        }
        assert!(!s.doc().unwrap().can_undo(), "nothing written while the drag lasts");
        s.pointer(&PointerEvent::new(PointerKind::Up, 60.0, 50.0), v).unwrap();
        assert_eq!(s.doc().unwrap().undo_steps(), 1);
        let script = "frame.add {\"rect\": [300, 300, 350, 350]}\nframe.add {\"rect\": [400, 400, 450, 450]}";
        s.execute("script.run", &json!({"text": script})).unwrap();
        assert_eq!(s.doc().unwrap().undo_steps(), 2);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(items(&s), 1, "the script's two frames undo together");
    }

    /// Design note 4.4: Smart Text Reflow runs only inside the local command that overset the
    /// story. The peer that imports the typing adds no page of its own, so the merged document has
    /// the pages one side's edit made, not twice as many.
    #[test]
    fn derived_edits_never_follow_a_merge() {
        let mut a = Session::new();
        a.execute("file.new", &json!({"pages": 1, "primaryTextFrame": true})).unwrap();
        let mut b = peer(&mut a);
        assert!(b.prefs.smart_text_reflow);
        let sid = a.doc().unwrap().doc.settings.primary_story.unwrap().0;
        a.execute("text.select", &json!({"story": sid, "anchor": 0, "focus": 0})).unwrap();
        a.execute("text.insert", &json!({"text": "Words keep coming and the page fills up quickly with them. ".repeat(120)})).unwrap();
        let pages = a.doc().unwrap().doc.page_count();
        assert!(pages >= 2, "a's typing added pages: {pages}");
        exchange(&mut a, &mut b);
        assert_eq!(b.doc().unwrap().doc.page_count(), pages, "b added none after the merge");
        // A local edit elsewhere on b does not reflow the merged story either.
        frame(&mut b, [400.0, 400.0, 450.0, 450.0]);
        assert_eq!(b.doc().unwrap().doc.page_count(), pages);
        exchange(&mut b, &mut a);
        assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
        assert_eq!(a.doc().unwrap().doc.story(StoryId(sid)).unwrap().frames.len(), pages);
    }

    #[test]
    fn an_import_during_a_drag_rebases_it() {
        let mut a = session();
        let id = frame(&mut a, [72.0, 72.0, 144.0, 144.0]);
        let mut b = peer(&mut a);
        a.set_tool("selection");
        let v = ViewInfo::at_zoom(1.0);
        a.pointer(&PointerEvent::new(PointerKind::Down, 100.0, 100.0), v).unwrap();
        a.pointer(&PointerEvent::new(PointerKind::Drag, 130.0, 100.0), v).unwrap();
        frame(&mut b, [300.0, 300.0, 350.0, 350.0]);
        exchange(&mut b, &mut a);
        assert!(a.doc().unwrap().interaction.is_some(), "the drag goes on");
        assert_eq!(items(&a), 2, "the merged frame shows during the drag");
        a.pointer(&PointerEvent::new(PointerKind::Up, 130.0, 100.0), v).unwrap();
        let moved = a.doc().unwrap().doc.item(kition_layout_doc::ItemId(id)).unwrap().bounds();
        assert!(moved.x0 > 72.0, "{moved:?}");
        exchange(&mut a, &mut b);
        assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
    }

    #[test]
    fn a_klayout_file_keeps_its_identity_and_holds_its_content_only() {
        let mut s = session();
        frame(&mut s, [72.0, 72.0, 144.0, 144.0]);
        let (bytes, base) = s.doc_mut().unwrap().klayout_file().unwrap();
        let version = s.version().unwrap();
        s.doc_mut().unwrap().saved_as(base);
        assert_eq!(s.version().unwrap(), version, "a save writes nothing into the live replica");
        let mut opened = Session::new();
        opened.add_document(DocState::open_klayout(&bytes, None).unwrap());
        assert_eq!(opened.doc_id(), s.doc_id());
        assert_eq!(opened.state_hash().unwrap(), s.state_hash().unwrap(), "the file holds the live replica's state, under its ids");
        assert!(!opened.doc().unwrap().is_dirty());
        frame(&mut opened, [200.0, 200.0, 250.0, 250.0]);
        let (again, base) = opened.doc_mut().unwrap().klayout_file().unwrap();
        assert_eq!(base.version(), &opened.version().unwrap());
        let parts = kition_layout_format::container::read(&again).unwrap();
        let content = String::from_utf8(parts.content).unwrap();
        assert!(content.starts_with("{\"kdm\":1,\"kit\":\"kit.layout\",\"docId\":"), "{content}");
        assert!(
            !content.contains("\"version\"") && !content.contains("lineage") && !content.contains("person:"),
            "the content holds no version, actor or origin"
        );
        // The engine's paths read it too.
        assert_eq!(crate::cmd::from_bytes(&again).unwrap().spreads[0].items.len(), 2);
        // A direct write between commands is no part of the next command's step.
        opened
            .edit(|d, _| {
                d.title = "Renamed".into();
                Ok(())
            })
            .unwrap();
        frame(&mut opened, [300.0, 300.0, 350.0, 350.0]);
        opened.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(opened.doc().unwrap().doc.title, "Renamed");
    }

    #[test]
    fn the_selection_keeps_what_an_undo_left() {
        let mut s = session();
        let id = frame(&mut s, [72.0, 72.0, 144.0, 144.0]);
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.doc().unwrap().selection.items.is_empty());
    }
}
