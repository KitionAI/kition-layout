//! Tool hosting: the active tool receives pointer/key events and its actions are executed here.

use kition_layout_geom::Unit;
pub use kition_layout_tools::SnapView;
use kition_layout_tools::{Action, CanvasLayout, Cursor, Mods, Overlay, PointerEvent, ToolContext, ToolKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{EngineError, Interaction, Result, Session};

/// Something the UI should do in response to a tool or command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UiRequest {
    Dialog {
        id: String,
        params: Value,
    },
    View {
        params: Value,
    },
    /// Open a file picker for the given purpose (`place`, `open`, `saveAs`, `export`).
    Pick {
        purpose: String,
        params: Value,
    },
}

/// View information the UI passes with pointer events.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ViewInfo {
    /// Screen pixels per point.
    pub zoom: f64,
    pub snap: SnapView,
    pub unit: Unit,
}

impl ViewInfo {
    pub const fn at_zoom(zoom: f64) -> Self {
        Self { zoom, snap: SnapView::FACTORY, unit: Unit::Points }
    }
}

impl Default for ViewInfo {
    fn default() -> Self {
        Self::at_zoom(1.0)
    }
}

impl Session {
    pub fn tool_id(&self) -> &'static str {
        self.tool.id()
    }

    pub fn set_tool(&mut self, id: &str) {
        if self.tool.id() == id {
            return;
        }
        self.ime_cancel();
        self.end_typing();
        self.tool = kition_layout_tools::create(id);
        // Leaving the Type tool keeps the text frame selected; the Eyedropper keeps the text
        // selected, to give it the formatting or colour it picks up.
        if id != "type"
            && id != "eyedropper"
            && let Some(st) = self.active_mut()
            && let Some(t) = st.selection.text.take()
            && let Some(f) = t.frame.or_else(|| st.doc.story(t.story).and_then(|s| s.frames.first().copied()))
        {
            st.selection.items = vec![f];
            st.selection.cells = None;
        }
    }

    pub fn layout(&self) -> CanvasLayout {
        match self.active() {
            Some(st) => CanvasLayout::new(&st.doc, st.editing_parents),
            None => CanvasLayout::default(),
        }
    }

    fn with_ctx<T>(&mut self, view: ViewInfo, f: impl FnOnce(&mut dyn kition_layout_tools::Tool, &ToolContext) -> T) -> Option<T> {
        let st = self.active()?;
        let doc = st.doc.clone();
        let sel = st.selection.clone();
        let layer = st.active_layer;
        let layout = CanvasLayout::new(&doc, st.editing_parents);
        let cache = self.cache.clone();
        let cx = ToolContext {
            doc: &doc,
            selection: &sel,
            cache: &cache,
            layout: &layout,
            zoom: view.zoom.max(1e-6),
            layer,
            snap: view.snap,
            unit: view.unit,
        };
        Some(f(self.tool.as_mut(), &cx))
    }

    pub fn pointer(&mut self, ev: &PointerEvent, view: ViewInfo) -> Result<()> {
        // A press (moving the caret, choosing something else) ends the typing; hovering does not.
        if matches!(ev.kind, kition_layout_tools::PointerKind::Down | kition_layout_tools::PointerKind::DoubleClick) {
            // A press ends a composition: its marked text goes (the input method commits on its own).
            self.ime_cancel();
            self.end_typing();
            // A gesture works on the document, not on a dialog's preview of it.
            self.cancel_preview();
        }
        let actions = self.with_ctx(view, |t, cx| t.pointer(cx, ev)).unwrap_or_default();
        self.run_actions(actions)
    }

    pub fn tool_key(&mut self, key: ToolKey, mods: Mods, view: ViewInfo) -> Result<bool> {
        self.ime_cancel();
        self.end_typing();
        self.cancel_preview();
        let actions = self.with_ctx(view, |t, cx| t.key(cx, key, mods)).unwrap_or_default();
        let handled = !actions.is_empty();
        self.run_actions(actions)?;
        Ok(handled)
    }

    pub fn overlays(&mut self, view: ViewInfo) -> Vec<Overlay> {
        let threads = self.prefs.show_text_threads;
        self.with_ctx(view, |t, cx| {
            let mut out = t.overlays(cx);
            if threads {
                out.extend(kition_layout_tools::thread::thread_lines(cx));
            }
            out
        })
        .unwrap_or_default()
    }

    pub fn cursor(&mut self, p: kition_layout_geom::Point, mods: Mods, view: ViewInfo) -> Cursor {
        self.with_ctx(view, |t, cx| t.cursor(cx, p, mods)).unwrap_or_default()
    }

    pub fn tool_busy(&self) -> bool {
        self.tool.busy()
    }

    /// The frame whose placed graphic the page shows whole and translucent now: it is being moved
    /// or sized inside its frame.
    pub fn content_ghost(&self) -> Option<kition_layout_doc::ItemId> {
        self.tool.ghost()
    }

    pub fn wants_text(&mut self) -> bool {
        self.with_ctx(ViewInfo::at_zoom(1.0), |t, cx| t.wants_text(cx)).unwrap_or(false)
    }

    /// An input method's marked text at the caret (`text`, replacing the last; empty clears it),
    /// shown in place through the active tool's preview: no undo step until the committed text is
    /// typed ([`Session::type_text`]). `active_chars` is the clause being converted.
    pub fn ime_preedit(&mut self, text: &str, active_chars: Option<std::ops::Range<usize>>) -> Result<()> {
        if !text.is_empty() && !self.tool.composing() {
            self.end_typing();
            // Marked text replaces a dialog's preview as any typing does.
            self.cancel_preview();
            if self.tool_busy() || self.doc()?.interaction.is_some() {
                return Err(EngineError::Other("finish the gesture in progress before typing".into()));
            }
        }
        let actions = self.with_ctx(ViewInfo::default(), |t, cx| t.ime_preedit(cx, text, active_chars)).unwrap_or_default();
        self.run_actions(actions)
    }

    /// Takes an input method's marked text away, leaving the document and the selection as they
    /// were and no undo or redo step. False when none was showing.
    pub fn ime_cancel(&mut self) -> bool {
        let actions = self.tool.ime_cancel();
        let had = !actions.is_empty();
        for a in actions {
            if a == Action::Cancel {
                self.cancel_interaction();
            }
        }
        had
    }

    /// Whether an input method's marked text is showing.
    pub fn composing(&self) -> bool {
        self.tool.composing()
    }

    /// Where an input method's candidate window goes: the caret line (top, bottom) in canvas
    /// points, or None when no text takes typing.
    pub fn ime_caret(&mut self, view: ViewInfo) -> Option<(kition_layout_geom::Point, kition_layout_geom::Point)> {
        self.with_ctx(view, |t, cx| t.ime_caret(cx)).flatten()
    }

    pub fn run_actions(&mut self, actions: Vec<Action>) -> Result<()> {
        for a in actions {
            match a {
                Action::Begin(label) => self.begin_interaction(&label)?,
                Action::Preview(cmd, p) => self.preview(&cmd, p)?,
                Action::Commit => self.commit_interaction()?,
                Action::Cancel => self.cancel_interaction(),
                Action::Exec(cmd, p) => {
                    self.execute(&cmd, &p)?;
                }
                Action::Dialog(id, params) => self.ui_requests.push(UiRequest::Dialog { id, params }),
                Action::SwitchTool(id) => self.set_tool(&id),
                Action::View(params) => self.ui_requests.push(UiRequest::View { params }),
            }
        }
        Ok(())
    }

    pub fn begin_interaction(&mut self, label: &str) -> Result<()> {
        let st = self.doc_mut()?;
        st.interaction = Some(Interaction { label: label.into(), doc: st.doc.clone(), selection: st.selection.clone(), preview: None });
        Ok(())
    }

    /// Apply `cmd` on top of the interaction's snapshot (replacing the previous preview).
    pub fn preview(&mut self, cmd: &str, params: Value) -> Result<()> {
        let st = self.doc_mut()?;
        let Some(it) = st.interaction.clone() else { return Err(EngineError::Other("no interaction".into())) };
        st.doc = it.doc.clone();
        st.selection = it.selection.clone();
        let r = self.execute(cmd, &params);
        if let Some(st) = self.active_mut()
            && let Some(i) = st.interaction.as_mut()
        {
            i.preview = Some((cmd.to_string(), params));
        }
        // Journal only the committed result.
        if r.is_ok() {
            self.journal.pop();
        }
        r.map(|_| ())
    }

    /// A dialog's live preview (an editor's Preview check box): shows `cmd` with `params` on the
    /// document as it was when the preview began, replacing the previous preview. The command runs
    /// through [`Session::execute`] inside an interaction, so nothing reaches the undo history
    /// until [`Session::commit_preview`]; [`Session::cancel_preview`] leaves neither an undo nor a
    /// redo step. Only an undoable command previews, and not during a tool's gesture.
    pub fn preview_command(&mut self, cmd: &str, params: &Value) -> Result<()> {
        let spec = crate::find_command(cmd).ok_or_else(|| EngineError::UnknownCommand(cmd.into()))?;
        if !spec.undoable {
            return Err(EngineError::Other(format!("`{cmd}` changes nothing a preview could show")));
        }
        if !self.dialog_preview {
            if self.tool_busy() || self.doc()?.interaction.is_some() {
                return Err(EngineError::Other("finish the gesture in progress before a preview".into()));
            }
            self.begin_interaction(spec.label.trim_end_matches('…'))?;
            self.dialog_preview = true;
        }
        self.dialog_steps.clear();
        // Run as a nested command: the top-level bookkeeping (writing the replica, ending the
        // typing, taking a preview back) is the preview's own.
        self.depth += 1;
        let result = self.preview(cmd, params.clone());
        self.depth = self.depth.saturating_sub(1);
        result
    }

    /// A dialog's live preview of several commands (an Effects dialog sets opacity and each
    /// effect): shows `steps` in order on the document as it was when the preview began, replacing
    /// the previous preview, as [`Session::preview_command`] shows one. Kept, they are one undo
    /// step named `label`; a step that fails leaves the document as it was before the steps.
    pub fn preview_steps(&mut self, label: &str, steps: &[(String, Value)]) -> Result<()> {
        for (cmd, _) in steps {
            let spec = crate::find_command(cmd).ok_or_else(|| EngineError::UnknownCommand(cmd.clone()))?;
            if !spec.undoable {
                return Err(EngineError::Other(format!("`{cmd}` changes nothing a preview could show")));
            }
        }
        if !self.dialog_preview {
            if self.tool_busy() || self.doc()?.interaction.is_some() {
                return Err(EngineError::Other("finish the gesture in progress before a preview".into()));
            }
            self.begin_interaction(label.trim_end_matches('…'))?;
            self.dialog_preview = true;
        }
        let st = self.doc_mut()?;
        let Some(it) = st.interaction.clone() else { return Err(EngineError::Other("no interaction".into())) };
        st.doc = it.doc.clone();
        st.selection = it.selection.clone();
        if let Some(i) = st.interaction.as_mut() {
            i.preview = None;
        }
        self.dialog_steps.clear();
        let journaled = self.journal.len();
        self.depth += 1;
        let mut failed = None;
        for (cmd, params) in steps {
            if let Err(error) = self.execute(cmd, params) {
                failed = Some(error);
                break;
            }
        }
        self.depth = self.depth.saturating_sub(1);
        // Journal only the committed result.
        self.journal.truncate(journaled);
        if let Some(error) = failed {
            if let Ok(st) = self.doc_mut() {
                st.doc = it.doc;
                st.selection = it.selection;
                st.revision = st.revision.saturating_add(1);
            }
            return Err(error);
        }
        self.dialog_steps = steps.to_vec();
        Ok(())
    }

    /// Keeps a dialog's preview as one undo step, labelled with its command. False when no preview
    /// was showing.
    pub fn commit_preview(&mut self) -> Result<bool> {
        if !self.dialog_preview {
            return Ok(false);
        }
        // Typing in progress is a step of its own, not part of the dialog's.
        self.end_typing();
        let steps = std::mem::take(&mut self.dialog_steps);
        self.commit_interaction()?;
        self.journal.extend(steps);
        Ok(true)
    }

    /// Takes a dialog's preview back: the document and the selection are as they were, with no
    /// undo or redo step. False when no preview was showing.
    pub fn cancel_preview(&mut self) -> bool {
        if !self.dialog_preview {
            return false;
        }
        self.cancel_interaction();
        true
    }

    /// Whether a dialog's preview is showing.
    pub fn previewing(&self) -> bool {
        self.dialog_preview
    }

    pub fn commit_interaction(&mut self) -> Result<()> {
        self.dialog_preview = false;
        let st = self.doc_mut()?;
        let Some(it) = st.interaction.take() else { return Ok(()) };
        if let Some((c, p)) = it.preview {
            self.journal.push((c, p));
        }
        // The interaction's last preview is the transform to repeat; the next one starts afresh.
        self.transforms.2 = false;
        // The gesture is one undo step: written now, unless a command is running it (its own
        // end writes it then).
        if self.depth == 0 {
            let origin = self.origin();
            if let Ok(st) = self.doc_mut() {
                st.sync(&origin, true, &it.label);
            }
        }
        Ok(())
    }

    pub fn cancel_interaction(&mut self) {
        self.dialog_preview = false;
        self.dialog_steps.clear();
        if let Some(st) = self.active_mut()
            && let Some(it) = st.interaction.take()
        {
            st.doc = it.doc;
            st.selection = it.selection;
            st.revision += 1;
            if self.transforms.2 {
                self.transforms.0.pop();
                self.transforms.2 = false;
            }
        }
    }
}
