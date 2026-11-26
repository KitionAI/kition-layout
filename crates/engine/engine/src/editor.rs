//! Session-only state a host saves around a call. Restoring it is no document edit.
use crate::{Prefs, Session, UiRequest};
use kition_layout_doc::{AssetId, Document, ItemId, LayerId, Selection, Story};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorSnapshot {
    selection: Selection,
    active_layer: LayerId,
    editing_parents: bool,
    tool: String,
    prefs: Prefs,
    clipboard: Option<Arc<Document>>,
    text_clipboard: Option<(Arc<Story>, String)>,
    loaded: Option<(AssetId, (f64, f64))>,
    #[serde(default)]
    place_queue: Vec<(AssetId, (f64, f64))>,
    transforms: (Vec<(String, Value)>, Vec<ItemId>, bool),
    text_drag: bool,
    #[serde(default)]
    text_drag_from: usize,
    untitled: u32,
    library: Option<crate::cmd::library::Library>,
    book: Option<crate::cmd::book::Book>,
    conveyor: Vec<(String, Vec<u8>)>,
    ui_requests: Vec<UiRequest>,
}

impl EditorSnapshot {
    /// The first identity this snapshot's selection names that `doc` lacks (a selected item, an
    /// item with selected anchors, the key object, the story or frame of the text selection, a
    /// selected table), as a reason; `None` when everything resolves. The active layer is no
    /// target: it is where new objects go, and it follows the document to its default layer as it
    /// does after an undo or a collaborator's change.
    pub fn unresolved_in(&self, doc: &Document) -> Option<String> {
        let selection = &self.selection;
        if let Some(id) = selection.items.iter().find(|id| doc.item(**id).is_none()) {
            return Some(format!("deleted selected item {}", id.0));
        }
        if let Some((id, _, _)) = selection.anchors.iter().find(|(id, _, _)| doc.item(*id).is_none()) {
            return Some(format!("deleted item {} with selected anchors", id.0));
        }
        if let Some(id) = selection.key.filter(|id| doc.item(*id).is_none()) {
            return Some(format!("deleted key object {}", id.0));
        }
        if let Some(text) = &selection.text {
            let Some(story) = doc.story(text.story) else {
                return Some(format!("deleted story {} holding the text selection", text.story.0));
            };
            if let Some(frame) = text.frame.filter(|frame| doc.item(*frame).is_none()) {
                return Some(format!("deleted text frame {} showing the text selection", frame.0));
            }
            if let Some(cell) = text.cell.filter(|cell| cell.footnote_id().is_none() && !story.tables.contains_key(&cell.table)) {
                return Some(format!("deleted table {} holding the text selection", cell.table));
            }
        }
        if let Some(cells) = &selection.cells
            && doc.story(cells.story).is_none_or(|story| !story.tables.contains_key(&cells.table))
        {
            return Some(format!("deleted table {} holding the selected cells", cells.table));
        }
        None
    }
}

impl Session {
    /// The first identity the current editor state names that `doc` lacks
    /// ([`EditorSnapshot::unresolved_in`]).
    pub fn editor_unresolved_in(&self, doc: &Document) -> Option<String> {
        self.editor_snapshot()?.unresolved_in(doc)
    }

    pub fn editor_snapshot(&self) -> Option<EditorSnapshot> {
        let state = self.active()?;
        Some(EditorSnapshot {
            selection: state.selection.clone(),
            active_layer: state.active_layer,
            editing_parents: state.editing_parents,
            tool: self.tool_id().to_owned(),
            prefs: self.prefs.clone(),
            clipboard: self.clipboard.clone(),
            text_clipboard: self.text_clipboard.clone(),
            loaded: self.loaded,
            place_queue: self.place_queue.clone(),
            transforms: self.transforms.clone(),
            text_drag: self.text_drag,
            text_drag_from: self.text_drag_from,
            untitled: self.untitled,
            library: self.library.clone(),
            book: self.book.clone(),
            conveyor: self.conveyor.clone(),
            ui_requests: self.ui_requests.clone(),
        })
    }

    pub fn restore_editor(&mut self, saved: EditorSnapshot) {
        self.set_tool(&saved.tool);
        if let Some(state) = self.active_mut() {
            state.selection = saved.selection;
            state.selection.items.retain(|id| state.doc.item(*id).is_some());
            if state.selection.text.is_some_and(|text| state.doc.story(text.story).is_none()) {
                state.selection.text = None;
            }
            state.selection.anchors.retain(|(id, _, _)| state.doc.item(*id).is_some());
            if state.selection.cells.is_some_and(|cells| state.doc.story(cells.story).is_none()) {
                state.selection.cells = None;
            }
            if state.selection.key.is_some_and(|id| state.doc.item(id).is_none()) {
                state.selection.key = None;
            }
            if state.doc.layers.iter().any(|layer| layer.id == saved.active_layer) {
                state.active_layer = saved.active_layer;
            }
            state.editing_parents = saved.editing_parents;
        }
        self.prefs = saved.prefs;
        self.clipboard = saved.clipboard;
        self.text_clipboard = saved.text_clipboard;
        self.loaded = saved.loaded;
        self.place_queue = saved.place_queue;
        self.transforms = saved.transforms;
        self.text_drag = saved.text_drag;
        self.text_drag_from = saved.text_drag_from;
        self.untitled = saved.untitled;
        self.library = saved.library;
        self.book = saved.book;
        self.conveyor = saved.conveyor;
        self.ui_requests = saved.ui_requests;
    }

    pub fn context_available(&self) -> bool {
        !self.tool_busy() && self.active().is_some_and(|state| state.interaction.is_none() && state.preview_stash.is_none())
    }
}
