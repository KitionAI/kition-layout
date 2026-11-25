//! The Kition Layout engine façade.
//!
//! Every user-visible action is a command with a stable id (`frame.add`, `text.insert`,
//! `layout.pages.insert`…) and JSON parameters. The kit's own editor and the headless kit app
//! both go through [`Session::execute`]. Tools (pointer gestures) are hosted here and reduce to
//! commands, so every gesture is journaled and replayable.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod cmd;
pub mod disk;
pub mod dtd;
mod editor;
pub mod guard;
pub mod math;
pub mod recovery;
pub mod replica;
pub mod sample;
pub mod script;
mod tooling;

use std::sync::Arc;

use kition_layout_compose::Cache;
use kition_layout_doc::{Document, LayerId, Selection};
use serde_json::Value;

pub use cmd::{CommandInfo, CommandSpec, command_specs, find_command};
pub use editor::EditorSnapshot;
pub use kition_layout_compose as compose;
pub use kition_layout_doc as doc;
pub use kition_layout_tools as tools;
pub use tooling::{SnapView, UiRequest, ViewInfo};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("command `{0}` is not available right now: {1}")]
    Disabled(String, String),
    #[error("invalid parameters for `{cmd}`: {msg}")]
    BadParams { cmd: String, msg: String },
    #[error("no active document")]
    NoDocument,
    #[error("{0}")]
    Other(String),
    /// A command panicked; the guard kept the document (a bug: please report it).
    #[error("internal error in `{0}` (the document was kept as it was): {1}")]
    Internal(String, String),
}

impl From<kition_layout_doc::DocError> for EngineError {
    fn from(e: kition_layout_doc::DocError) -> Self {
        EngineError::Other(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(Clone, Debug)]
pub struct Interaction {
    pub label: String,
    pub doc: Arc<Document>,
    pub selection: Selection,
    pub preview: Option<(String, Value)>,
}

#[derive(Clone, Debug)]
pub struct DocState {
    pub doc: Arc<Document>,
    pub selection: Selection,
    /// The document's KDM replica, which holds its undo history ([`replica`]).
    pub link: replica::Link,
    pub path: Option<String>,
    pub revision: u64,
    pub saved_revision: u64,
    /// The document as last saved (dirty = the current document is a different allocation).
    pub saved_doc: Arc<Document>,
    pub active_layer: LayerId,
    pub interaction: Option<Interaction>,
    /// Editing parent spreads (Pages panel double-click on a parent).
    pub editing_parents: bool,
    pub uid: u64,
    /// Unfilled document while data-merge preview is showing a record. Not saved.
    pub preview_stash: Option<Arc<Document>>,
    /// 1-based record currently previewed.
    pub preview_record: Option<u32>,
    /// The fonts the document brought (`doc.font_scope`), found while it is open.
    pub fonts: Option<Arc<FontScope>>,
}

/// A document's font scope ([`kition_layout_fonts::FontDb::load_document_fonts`]): its fonts are
/// found until the document's state (with every copy of it) is gone — closed, reverted, or its
/// session ended.
#[derive(Debug)]
pub struct FontScope(pub u32);

impl Drop for FontScope {
    fn drop(&mut self) {
        kition_layout_fonts::FontDb::global().close_scope(self.0);
    }
}

static NEXT_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl DocState {
    pub fn new(doc: Document, path: Option<String>) -> Self {
        let active_layer = doc.default_layer();
        let doc = Arc::new(doc);
        let link = replica::Link::new(&doc);
        DocState {
            saved_doc: doc.clone(),
            doc,
            selection: Selection::default(),
            link,
            path,
            revision: 1,
            saved_revision: 1,
            active_layer,
            interaction: None,
            editing_parents: false,
            uid: NEXT_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            preview_stash: None,
            preview_record: None,
            fonts: None,
        }
    }
    pub fn is_dirty(&self) -> bool {
        !Arc::ptr_eq(&self.doc, &self.saved_doc)
    }
    pub fn title(&self) -> String {
        self.path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.doc.title.clone())
    }
}

/// Preferences that the engine needs (UI keeps its own).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub show_hidden_characters: bool,
    pub typographers_quotes: bool,
    /// Polygon Settings (double-click the Polygon tool): number of sides, star inset (0–1).
    pub polygon_sides: u32,
    pub star_inset: f64,
    /// Scaling objects scales their stroke weights (Preferences › General › When Scaling).
    pub scale_strokes: bool,
    /// X/Y/W/H in the Control and Properties panels measure the stroke's outer edge.
    pub dimensions_include_stroke: bool,
    /// Preferences › General › Transformations are Totals: rotation, shear and scale of nested
    /// objects (a graphic in a frame, objects in groups) are measured on the pasteboard rather
    /// than relative to their container.
    pub transformations_are_totals: bool,
    /// Preferences › General › Page Numbering View: Absolute (1, 2, 3… from the first page) instead
    /// of section numbering (the page names, e.g. "iv", "A-3").
    pub absolute_page_numbers: bool,
    /// Preferences › Composition › Highlight: H&J violations, custom tracking/kerning, substituted
    /// fonts (screen only).
    pub highlight_hj: bool,
    pub highlight_keeps: bool,
    pub highlight_custom_tracking: bool,
    pub highlight_substituted_fonts: bool,
    /// Preferences › Appearance of Black › Printing / Exporting: 100% K as rich (pure) black in
    /// RGB output (PNG) instead of the accurate dark grey.
    pub rich_black_output: bool,
    /// Font menu favourites (family names).
    pub favorite_fonts: Vec<String>,
    /// Preferences › Type › Show Font Names in English: the font menus show CJK families by their
    /// English names instead of their native ones. Documents store the English name either way.
    pub show_font_names_in_english: bool,
    /// Preferences › Type › Smart Text Reflow: pages follow the primary text frame's story
    /// (added while it oversets, empty ones at the end removed).
    pub smart_text_reflow: bool,
    /// Preferences › Autocorrect: typing a space or punctuation after a listed word replaces it.
    pub autocorrect: bool,
    /// Misspelled word → correction (lowercase; the correction follows the typed capitalisation).
    pub autocorrect_list: Vec<(String, String)>,
    /// Preferences › Track Changes › Show: added text highlighted on screen.
    pub show_added_text: bool,
    /// Preferences › Notes: note anchors shown in layout view.
    pub show_note_anchors: bool,
    /// View › Extras › Show Text Threads: lines between the threaded frames of the selection.
    pub show_text_threads: bool,
    /// Preferences › File Handling: minutes between document recovery saves.
    pub recovery_minutes: f64,
}

/// A starter autocorrect list (common English typing slips).
pub fn default_autocorrect() -> Vec<(String, String)> {
    [
        ("teh", "the"),
        ("adn", "and"),
        ("taht", "that"),
        ("recieve", "receive"),
        ("seperate", "separate"),
        ("occured", "occurred"),
        ("untill", "until"),
        ("wich", "which"),
        ("becuase", "because"),
        ("definately", "definitely"),
        ("thier", "their"),
        ("alot", "a lot"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect()
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            show_hidden_characters: false,
            typographers_quotes: true,
            polygon_sides: 6,
            star_inset: 0.0,
            scale_strokes: true,
            dimensions_include_stroke: true,
            transformations_are_totals: true,
            absolute_page_numbers: false,
            highlight_hj: false,
            highlight_keeps: false,
            highlight_custom_tracking: false,
            highlight_substituted_fonts: true,
            rich_black_output: false,
            favorite_fonts: Vec::new(),
            show_font_names_in_english: false,
            smart_text_reflow: true,
            autocorrect: false,
            autocorrect_list: default_autocorrect(),
            show_added_text: true,
            show_note_anchors: true,
            show_text_threads: false,
            recovery_minutes: 0.5,
        }
    }
}

pub struct Session {
    docs: Vec<DocState>,
    active: Option<usize>,
    pub prefs: Prefs,
    pub cache: Arc<Cache>,
    pub(crate) tool: Box<dyn kition_layout_tools::Tool>,
    pub journal: Vec<(String, Value)>,
    pub clipboard: Option<Arc<Document>>,
    /// Crash-recovery folder (the app sets it; saving or closing a document removes its entry).
    pub recovery_dir: Option<std::path::PathBuf>,
    /// Copied text with its formatting (a frameless story slice) and its plain text.
    pub text_clipboard: Option<(Arc<kition_layout_doc::Story>, String)>,
    /// Requests for the UI (dialogs, view changes) produced by tools/commands.
    pub ui_requests: Vec<UiRequest>,
    /// Graphic loaded in the place cursor: (asset, natural size in points).
    pub loaded: Option<(kition_layout_doc::AssetId, (f64, f64))>,
    pub(crate) untitled: u32,
    /// Transforms applied to the current selection, oldest first (Transform Again / Sequence
    /// Again), with the selection they were applied to and whether the last one came from a
    /// tool interaction (its previews replace each other).
    pub(crate) transforms: (Vec<(String, Value)>, Vec<kition_layout_doc::ItemId>, bool),
    /// Drag and drop text editing: the press landed in the selected text.
    pub(crate) text_drag: bool,
    /// The open Object Library (File › New / Open Library).
    pub library: Option<cmd::library::Library>,
    /// The open book (File › Open Book).
    pub book: Option<cmd::book::Book>,
    /// Content Collector conveyor: collected objects as snippets (name, bytes).
    pub conveyor: Vec<(String, Vec<u8>)>,
    /// Who commands act for ([`Session::execute_as`]); `None`: the local person.
    pub(crate) origin: Option<kition_layout_kdm::kition_kit::kdm::Origin>,
    /// How deep [`Session::execute`] is: the replica is written when the top-level call ends.
    pub(crate) depth: u32,
    /// Every file belongs to the host ([`Session::for_host`]: a kit's document; KHP A29): the
    /// engine reads only what the host lends and reaches nothing else by path ([`disk`]).
    host_files: Option<disk::HostFiles>,
    /// What anyone but a person may not run (an agent, a script's `model.batch`, a host's own
    /// change), checked before every command however it arrives: directly, as a batch's step, or
    /// as a step of `script.run` (the kit's own list: commands that name files or start programs).
    pub agent_gate: Option<fn(&str) -> std::result::Result<(), String>>,
    /// Typing in progress: its text joins one undo step ([`Session::type_text`]).
    pub(crate) typing: Option<replica::Typing>,
    /// Whether the running command is [`Session::type_text`]'s insert (which keeps the typing open).
    pub(crate) typing_now: bool,
    /// A dialog's live preview is showing ([`Session::preview_command`]): the interaction is the
    /// dialog's, not a tool's.
    pub(crate) dialog_preview: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        guard::install_panic_hook();
        Session {
            docs: vec![],
            active: None,
            prefs: Prefs::default(),
            cache: Arc::new(Cache::new()),
            tool: kition_layout_tools::create("selection"),
            journal: vec![],
            clipboard: None,
            recovery_dir: None,
            text_clipboard: None,
            ui_requests: vec![],
            loaded: None,
            transforms: Default::default(),
            text_drag: false,
            library: None,
            conveyor: Vec::new(),
            book: None,
            untitled: 0,
            origin: None,
            depth: 0,
            host_files: None,
            agent_gate: None,
            typing: None,
            typing_now: false,
            dialog_preview: false,
        }
    }

    /// A session whose files all belong to the host (a kit's document, served headless or in the
    /// page; KHP A29, A45): it reads a file a document or a command names only through `files`,
    /// by its path in the workspace, and reaches nothing else by path and starts no program
    /// ([`disk`]).
    pub fn for_host(files: disk::HostFiles) -> Self {
        Session { host_files: Some(files), ..Session::new() }
    }

    /// Whether the host owns every file ([`Session::for_host`]).
    pub fn host_files_only(&self) -> bool {
        self.host_files.is_some()
    }

    pub fn documents(&self) -> &[DocState] {
        &self.docs
    }
    pub fn active_index(&self) -> Option<usize> {
        self.active
    }
    /// A page's label as the UI shows it (Page Numbering View).
    pub fn page_label(&self, abs: usize) -> String {
        match self.active() {
            Some(d) if !self.prefs.absolute_page_numbers => d.doc.page_name(abs),
            _ => (abs + 1).to_string(),
        }
    }

    /// The page a typed page reference means: `+n` is always absolute; otherwise a section page
    /// name (section numbering) or a position (absolute numbering, or no page has that name).
    pub fn resolve_page(&self, s: &str) -> Option<usize> {
        let d = self.active()?;
        let n = d.doc.page_count();
        let s = s.trim();
        let pos = |t: &str| t.parse::<usize>().ok().filter(|k| (1..=n).contains(k)).map(|k| k - 1);
        if let Some(t) = s.strip_prefix('+') {
            return pos(t);
        }
        if !self.prefs.absolute_page_numbers
            && let Some(i) = (0..n).find(|i| d.doc.page_name(*i).eq_ignore_ascii_case(s))
        {
            return Some(i);
        }
        pos(s)
    }

    pub fn active(&self) -> Option<&DocState> {
        self.active.and_then(|i| self.docs.get(i))
    }
    pub fn active_mut(&mut self) -> Option<&mut DocState> {
        self.active.and_then(|i| self.docs.get_mut(i))
    }
    pub fn doc(&self) -> Result<&DocState> {
        self.active().ok_or(EngineError::NoDocument)
    }
    pub fn doc_mut(&mut self) -> Result<&mut DocState> {
        self.active_mut().ok_or(EngineError::NoDocument)
    }
    pub fn set_active(&mut self, i: usize) {
        if i < self.docs.len() && self.active != Some(i) {
            self.stop_data_preview();
            self.active = Some(i);
        }
    }
    pub fn add_document(&mut self, d: DocState) -> usize {
        self.stop_data_preview();
        self.docs.push(d);
        self.active = Some(self.docs.len() - 1);
        self.docs.len() - 1
    }
    /// Put the unfilled template back. Preview is session state and must not be saved.
    pub fn stop_data_preview(&mut self) {
        let Some(st) = self.active_mut() else { return };
        st.preview_record = None;
        let Some(stash) = st.preview_stash.take() else { return };
        st.doc = stash;
        st.revision = st.revision.saturating_add(1);
    }
    pub fn close_document(&mut self, i: usize) {
        if self.active == Some(i) {
            self.stop_data_preview();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let (Some(dir), Some(d)) = (&self.recovery_dir, self.docs.get(i)) {
            recovery::discard(dir, d.uid);
        }
        if i < self.docs.len() {
            self.docs.remove(i);
            self.active = if self.docs.is_empty() { None } else { Some(i.min(self.docs.len() - 1)) };
        }
    }

    /// Run command `id`. A panic inside it becomes [`EngineError::Internal`] and leaves the
    /// document as it was (see [`guard`]). When the top-level call ends, what it changed is
    /// written to the document's replica as one transaction: one undo step for an undoable command
    /// (nested commands and derived edits included), a `system` change otherwise, nothing during an
    /// interaction ([`replica`]).
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        let _files = disk::enter(self.host_files);
        let acting = (self.depth == 0).then(|| self.active().map(|d| d.uid)).flatten();
        // A command from anywhere else (an agent, a panel) runs on the document, not on a dialog's
        // preview of it: the preview is taken back first. A query reads the preview as it shows.
        if self.depth == 0 && self.dialog_preview && find_command(id).is_some_and(|spec| spec.undoable || spec.journal) {
            self.cancel_preview();
        }
        if self.depth == 0 {
            // A direct write since the last command (`Session::edit` from outside one) is no part
            // of this command's step: it goes first, as a `system` change.
            self.sync_all(None, "");
        }
        self.depth += 1;
        let r = self.guarded(id, |s| s.execute_unguarded(id, params));
        self.depth = self.depth.saturating_sub(1);
        if self.depth == 0 {
            // Any other edit ends the typing before it is written: it is a step of its own (a
            // query, which changes nothing, leaves the typing going).
            if !self.typing_now && self.typing.is_some() && self.docs.iter().any(DocState::unsynced) {
                self.end_typing();
            }
            // Every edit a person makes is theirs to undo: a command marked not undoable that
            // changed the document anyway (Tab in a table's last cell adds a row) is a step of its
            // own. File commands' bookkeeping (a save refreshing relative paths) is not.
            let (undoable, label) = find_command(id).map_or((false, ""), |spec| (spec.undoable || !spec.id.starts_with("file."), spec.label));
            // The step is the acting document's, while it is still the one shown (a merge that
            // opened its result writes its source's bookkeeping as a `system` change).
            let still = acting.filter(|uid| self.active().is_some_and(|d| d.uid == *uid));
            self.sync_all(still.filter(|_| undoable), label);
        }
        r
    }

    fn execute_unguarded(&mut self, id: &str, params: &Value) -> Result<Value> {
        let spec = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.into()))?;
        if let Some(gate) = self.agent_gate
            && self.origin.as_ref().is_some_and(|origin| origin.kind != kition_layout_kdm::kition_kit::kdm::OriginKind::Person)
        {
            gate(id).map_err(EngineError::Other)?;
        }
        if let Err(e) = (spec.enabled)(self) {
            // A command that takes `ids` acts on the objects the call names, so it needs no
            // selection: with a text caret or nothing selected, `{"ids": [5]}` still runs.
            let named = if e == cmd::NOTHING_SELECTED || e == cmd::NO_TEXT_OR_FRAMES {
                cmd::named_targets(spec, params)
            } else if e == "place the insertion point in a table"
                && params.get("table").is_some()
                && (id.starts_with("table.") || matches!(id, "style.cell.apply" | "style.table.apply"))
            {
                cmd::table::table_address(self, params, id)?;
                Some(Vec::new())
            } else {
                None
            };
            let Some(ids) = named else { return Err(EngineError::Disabled(id.into(), e)) };
            let st = self.doc()?;
            if let Some(gone) = ids.iter().find(|i| st.doc.item(**i).is_none()) {
                return Err(EngineError::Disabled(id.into(), format!("no object with id {}", gone.0)));
            }
        }
        // Preview is temporary. Save, merge, and every other edit restore the template first.
        // Queries stay read-only so the panel can poll fields during a preview. `data.preview`
        // itself is the one command that fills the live document.
        if id != "data.preview" && (spec.undoable || spec.journal) {
            self.stop_data_preview();
        }
        let before = self.active().map(|d| (d.uid, d.doc.clone()));
        let r = (spec.run)(self, params)?;
        // Undo, redo or deleting layers can take the active layer away: new objects would land on
        // a layer that isn't there (invisible, not exported). Fall back to the top layer.
        if let Some(st) = self.active_mut()
            && st.doc.layer(st.active_layer).is_none()
        {
            st.active_layer = st.doc.default_layer();
        }
        self.record_transform(id, params);
        if self.prefs.smart_text_reflow
            && spec.undoable
            && let (Some((uid, old)), Some(st)) = (&before, self.active())
            && st.uid == *uid
            && reflow_wanted(old, &st.doc)
        {
            self.smart_reflow();
        }
        // Live captions follow their sources.
        if spec.undoable
            && let Some(st) = self.active_mut()
            && st.interaction.is_none()
            && st.doc.spreads.iter().any(|sp| sp.items.iter().any(|i| i.live_caption.is_some()))
        {
            let mut d = (*st.doc).clone();
            if cmd::captions::refresh_live(&mut d) {
                st.doc = Arc::new(d);
            }
        }
        if spec.journal {
            self.journal.push((id.to_string(), params.clone()));
        }
        Ok(r)
    }

    /// Smart Text Reflow for the primary story: add threaded pages while it oversets; remove
    /// trailing pages whose only object is an empty frame of it.
    fn smart_reflow(&mut self) {
        let Some(st) = self.active() else { return };
        if st.interaction.is_some() {
            return;
        }
        let Some(sid) = st.doc.settings.primary_story else { return };
        let Some(story) = st.doc.story(sid) else { return };
        let cs = self.cache.get(&st.doc, sid, None);
        let overset = cs.overset_at.is_some();
        let empty_tail = story.frames.len() > 1 && story.frames.last().is_some_and(|f| cs.frame(*f).is_none_or(|ft| ft.range.is_empty()));
        if !overset && !empty_tail {
            return;
        }
        let mut d = (*st.doc).clone();
        if overset {
            let _ = cmd::place_text_autoflow(&mut d, sid, 500);
        } else {
            // Drop empty trailing frames on the last pages (one frame, nothing else there).
            while let Some(story) = d.story(sid) {
                if story.frames.len() < 2 {
                    break;
                }
                let Some(&last) = story.frames.last() else { break };
                let cs = kition_layout_compose::compose_story(&d, sid, &Default::default());
                if cs.frame(last).is_some_and(|ft| !ft.range.is_empty()) {
                    break;
                }
                let Some(page) = d.page_of_item(last) else { break };
                let alone = d.spreads.iter().flat_map(|sp| sp.items.iter()).filter(|i| d.page_of_item(i.id) == Some(page)).count() == 1;
                if page + 1 != d.page_count() || !alone || d.page_count() < 2 {
                    break;
                }
                if d.remove_item(last).is_err() || d.delete_pages(&[page]).is_err() {
                    break;
                }
            }
        }
        if let Some(st) = self.active_mut() {
            st.doc = Arc::new(d);
            st.revision += 1;
        }
    }

    /// Remember a move/rotate/scale/shear/flip for Transform Again.
    fn record_transform(&mut self, id: &str, params: &Value) {
        if !matches!(id, "transform.move" | "transform.rotate" | "transform.scale" | "transform.shear" | "transform.flip") {
            return;
        }
        let Some(st) = self.active() else { return };
        let sel = st.selection.items.clone();
        let live = st.interaction.is_some();
        let mut p = params.clone();
        if let Some(o) = p.as_object_mut() {
            o.remove("ids");
            o.remove("id");
        }
        let (list, on, was_live) = &mut self.transforms;
        if *on != sel {
            list.clear();
            *on = sel;
        } else if *was_live && live && list.last().is_some_and(|l| l.0 == id) {
            list.pop();
        }
        list.push((id.to_string(), p));
        *was_live = live;
    }

    pub fn commands(&self) -> Vec<CommandInfo> {
        command_specs().iter().map(|c| c.info(self)).collect()
    }

    /// Mutate the active document (copy-on-write) and bump the revision.
    pub fn edit<T>(&mut self, f: impl FnOnce(&mut Document, &mut Selection) -> Result<T>) -> Result<T> {
        let st = self.doc_mut()?;
        let mut doc = (*st.doc).clone();
        let mut sel = st.selection.clone();
        let r = f(&mut doc, &mut sel)?;
        crate::cmd::sync_placeholders(&mut doc);
        doc.sync_endnote_story();
        st.doc = Arc::new(doc);
        st.selection = sel;
        st.revision += 1;
        Ok(r)
    }
}

/// Smart Text Reflow follows edits of the primary story's text and formatting (and of the styles
/// it uses), as InDesign's does: deleting pages, deleting or unthreading its frames, or moving
/// them leaves the pages as they are even while the story oversets.
fn reflow_wanted(old: &Document, new: &Document) -> bool {
    let Some(sid) = new.settings.primary_story else { return false };
    if old.settings.primary_story != Some(sid) || !Arc::ptr_eq(&old.styles, &new.styles) {
        return true;
    }
    match (old.stories.get(&sid), new.stories.get(&sid)) {
        (Some(a), Some(b)) => !Arc::ptr_eq(a, b) && (a.text != b.text || a.paras != b.paras || a.chars != b.chars || a.tables != b.tables),
        (None, Some(_)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests_hosted;
#[cfg(test)]
mod tests_idml;
#[cfg(test)]
mod tests_roundtrip;
#[cfg(test)]
mod tests_table;
