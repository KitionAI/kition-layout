//! # kition-layout-kit
//!
//! The Layout engine for Kition's in-page editor, on the Kition SDK. [`LayoutDocument`]
//! implements [`KitDocument`] for a Kition Layout layout (`.klayout`; IDML opens too), and
//! [`KitTools`] for Kition Layout's own tools, so the host runs its selection, frame, pen, and type
//! tools unchanged.
//!
//! Document pixels are points on Kition Layout's canvas, where the spreads are stacked top to
//! bottom as in InDesign, measured from the top-left of the spreads' union with a margin of
//! pasteboard around it.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::cell::RefCell;
use std::sync::Mutex;

mod addresses;
pub mod check;
mod chrome;
mod collab;
mod describe;
pub use describe::NO_OUTPUT;

#[cfg(test)]
mod conformance;
pub mod files;
mod params;
mod place;
pub mod render;

use kition_kit::raster::{self, Frame};
use kition_kit::tools::{KitTools, PointerAction, ToolInfo, ToolMods, ToolOverlay, ToolPointer, ToolRequest, ToolResponse};
use kition_kit::{KitCommand, KitDocument, MAX_RENDER_SIDE};
use kition_layout_engine::{DocState, Session};
use kition_layout_engine::{UiRequest, ViewInfo};
use kition_layout_geom::{Affine, Point, Rect};
use kition_layout_kdm::FileBase;
use kition_layout_render::{Placed, RenderOptions, Renderer};
use kition_layout_tools::{Cursor, Mods, PointerEvent, PointerKind, ToolKey};
use serde_json::{Value, json};

/// Pasteboard shown around the spreads, in points.
const MARGIN: f64 = 72.0;
/// The pasteboard's colour, as Kition's canvas backdrop.
const PASTEBOARD: [u8; 4] = [243, 243, 246, 255];

/// All editor-only keys exposed by `inspect`, shared with the headless backend.
pub const EDITOR_STATE: &[&str] = &["/selection", "/activeLayer", "/tool"];

/// The key of [`LayoutDocument`]'s mark in the editor states it answers.
const OWNER: &str = "owner";

/// One open Kition Layout layout.
pub struct LayoutDocument {
    session: Session,
    renderer: RefCell<Renderer>,
    /// Where the pointer hovers (no button held), for hover feedback.
    hover: Option<Point>,
    /// The canvas around the spreads (an editor's canvas color).
    pasteboard: [u8; 4],
    /// What the file holds after the last `save`, taken as the document's file by `mark_saved`
    /// once the host has stored those bytes (edits made in between stay unsaved).
    saving: Mutex<Option<FileBase>>,
    /// This document's own mark in the editor states it answers: a state it answered itself is
    /// put back as it is, while one of another document (the session a file reread replaces, the
    /// one a premise plan forks) must name only what this document has.
    editor_owner: u64,
}

/// The next [`LayoutDocument::editor_owner`].
static NEXT_EDITOR_OWNER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn engine_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl LayoutDocument {
    /// Refuses a parameter `command` does not take (a typo or a guessed name) before it runs,
    /// naming the one it resembles, instead of running without it (engine rule E7a). The schemas
    /// are the ones the catalogue offers agents, read once.
    pub(crate) fn refuse_unknown(&self, command: &str, params: &Value) -> Result<(), String> {
        static SCHEMAS: std::sync::OnceLock<std::collections::HashMap<String, Value>> = std::sync::OnceLock::new();
        let schemas = SCHEMAS.get_or_init(|| self.commands().into_iter().filter_map(|command| Some((command.id, command.schema?))).collect());
        schemas.get(command).map_or(Ok(()), |schema| kition_kit::schema::refuse_unknown(command, schema, params))
    }

    fn from_session(session: Session) -> Self {
        let editor_owner = NEXT_EDITOR_OWNER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let document =
            Self { session, renderer: RefCell::new(Renderer::new()), hover: None, pasteboard: PASTEBOARD, saving: Mutex::new(None), editor_owner };
        document.commands();
        document
    }

    fn active(&self) -> Result<&DocState, String> {
        self.session.active().ok_or_else(|| "no document is open".to_owned())
    }

    /// The canvas area document pixels cover: the spreads' union and a margin of pasteboard.
    fn bounds(&self) -> Rect {
        let layout = self.session.layout();
        let Ok(state) = self.active() else { return Rect::new(0.0, 0.0, 612.0, 792.0) };
        let mut union: Option<Rect> = None;
        for slot in &layout.slots {
            let Some(spread) = state.doc.spread(slot.spread) else { continue };
            let placed = slot.xf.transform_rect_bbox(spread.bounds());
            union = Some(union.map_or(placed, |all| all.union(placed)));
        }
        union.unwrap_or(Rect::new(0.0, 0.0, 612.0, 792.0)).inflate(MARGIN, MARGIN)
    }

    fn origin(&self) -> Point {
        let bounds = self.bounds();
        Point::new(bounds.x0, bounds.y0)
    }

    fn to_canvas(&self, x: f64, y: f64) -> Point {
        let origin = self.origin();
        Point::new(origin.x + x, origin.y + y)
    }

    /// `width × height` straight-alpha pixels of the area at document pixel (`x`, `y`), `scale`
    /// screen pixels per point: paper-white pages on the pasteboard, as on screen.
    fn draw(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let state = self.active()?;
        let origin = self.origin();
        let layout = self.session.layout();
        let placed: Vec<Placed> = layout.slots.iter().map(|slot| Placed { spread: slot.spread, xf: slot.xf }).collect();
        let view = Affine::scale(scale) * Affine::translate((-(origin.x + x), -(origin.y + y)));
        let options = RenderOptions { background: Some(self.pasteboard), paper: true, page_shadow: true, greek_below_px: 3.0, ..Default::default() };
        let mut renderer = self.renderer.try_borrow_mut().map_err(|_| "the renderer is busy".to_owned())?;
        Ok(renderer.render(&state.doc, &self.session.cache, &placed, width, height, view, &options).to_straight())
    }

    fn view(zoom: f64) -> ViewInfo {
        ViewInfo::at_zoom(if zoom.is_finite() && zoom > 0.0 { zoom } else { 1.0 })
    }

    /// The engine's answer after an event: its requests, overlays with the selection and text
    /// display (in document pixels), its cursor, and whether it is busy or takes text.
    fn respond(&mut self, before_tool: &str, at: Option<Point>, zoom: f64) -> ToolResponse {
        let view = Self::view(zoom);
        let origin = self.origin();
        let mut requests: Vec<ToolRequest> = self.session.ui_requests.drain(..).filter_map(request_out).collect();
        if self.session.tool_id() != before_tool {
            requests.push(ToolRequest::SwitchTool { id: self.session.tool_id().to_owned() });
        }
        let mut engine = chrome::overlays(&self.session, zoom);
        engine.extend(self.session.overlays(view).into_iter().filter_map(chrome::from_tool));
        let overlays = engine.into_iter().filter_map(|overlay| shift(overlay, origin)).collect();
        let cursor = at.map_or_else(|| "default".to_owned(), |point| cursor_css(self.session.cursor(point, Mods::default(), view)));
        ToolResponse { requests, overlays, cursor, busy: self.session.tool_busy(), wants_text: self.session.wants_text() }
    }
}

fn request_out(request: UiRequest) -> Option<ToolRequest> {
    match request {
        UiRequest::Dialog { id, params } => Some(ToolRequest::Dialog { name: id, params }),
        UiRequest::Pick { purpose, params } => Some(ToolRequest::Dialog { name: format!("pick.{purpose}"), params }),
        // View changes (zoom to a page) are the host's to make; Kition keeps its own view.
        UiRequest::View { .. } => None,
    }
}

/// An overlay in canvas points moved into document pixels.
fn shift(overlay: ToolOverlay, origin: Point) -> Option<ToolOverlay> {
    let (dx, dy) = (-origin.x, -origin.y);
    Some(match overlay {
        ToolOverlay::Marquee { x0, y0, x1, y1 } => ToolOverlay::Marquee { x0: x0 + dx, y0: y0 + dy, x1: x1 + dx, y1: y1 + dy },
        ToolOverlay::Path { d, color, width, dashed } => {
            let path = kition_layout_geom::kurbo::BezPath::from_svg(&d).ok()?;
            ToolOverlay::Path { d: (Affine::translate((dx, dy)) * path).to_svg(), color, width, dashed }
        }
        ToolOverlay::Line { ax, ay, bx, by, color, dashed } => {
            ToolOverlay::Line { ax: ax + dx, ay: ay + dy, bx: bx + dx, by: by + dy, color, dashed }
        }
        ToolOverlay::Anchor { x, y, color, filled, size } => ToolOverlay::Anchor { x: x + dx, y: y + dy, color, filled, size },
        ToolOverlay::Handle { x, y, color } => ToolOverlay::Handle { x: x + dx, y: y + dy, color },
        ToolOverlay::Label { x, y, text, color } => ToolOverlay::Label { x: x + dx, y: y + dy, text, color },
        ToolOverlay::Measure { x, y, text } => ToolOverlay::Measure { x: x + dx, y: y + dy, text },
        ToolOverlay::Highlight { quad, color } => ToolOverlay::Highlight { quad: quad.map(|[x, y]| [x + dx, y + dy]), color },
        ToolOverlay::Swatch { x, y, color, selected } => ToolOverlay::Swatch { x: x + dx, y: y + dy, color, selected },
    })
}

fn mods_in(mods: ToolMods) -> Mods {
    Mods { shift: mods.shift, alt: mods.alt, cmd: mods.command, ctrl: mods.control, space: mods.space }
}

/// The engine's cursor as a CSS cursor name.
fn cursor_css(cursor: Cursor) -> String {
    match cursor {
        Cursor::Move => "move",
        Cursor::Crosshair | Cursor::Pen => "crosshair",
        Cursor::ResizeH => "ew-resize",
        Cursor::ResizeV => "ns-resize",
        Cursor::ResizeNwSe => "nwse-resize",
        Cursor::ResizeNeSw => "nesw-resize",
        Cursor::Text => "text",
        Cursor::Hand | Cursor::Rotate => "grab",
        Cursor::HandGrab => "grabbing",
        Cursor::ZoomIn => "zoom-in",
        Cursor::ZoomOut => "zoom-out",
        Cursor::Eyedropper | Cursor::LoadedText | Cursor::LoadedGraphic => "copy",
        Cursor::NotAllowed => "not-allowed",
        _ => "default",
    }
    .to_owned()
}

fn key_in(key: &str) -> Option<ToolKey> {
    Some(match key {
        "Enter" => ToolKey::Enter,
        "Escape" => ToolKey::Escape,
        "Backspace" => ToolKey::Backspace,
        "Delete" => ToolKey::Delete,
        "ArrowUp" | "Up" => ToolKey::Up,
        "ArrowDown" | "Down" => ToolKey::Down,
        "ArrowLeft" | "Left" => ToolKey::Left,
        "ArrowRight" | "Right" => ToolKey::Right,
        "Home" => ToolKey::Home,
        "End" => ToolKey::End,
        "Tab" => ToolKey::Tab,
        "PageUp" => ToolKey::PageUp,
        "PageDown" => ToolKey::PageDown,
        _ => return None,
    })
}

impl KitDocument for LayoutDocument {
    fn batch_settler(&self) -> Option<kition_kit::BatchSettler> {
        Some(Box::new(|tx| kition_layout_kdm::check_batch(tx).map_err(|error| error.to_string())))
    }

    fn batch_applied(&mut self) {
        if let Ok(state) = self.active() {
            place::history::remember(&state.doc);
        }
    }

    fn under(&self, command: &str, params: &Value) -> Result<Option<kition_kit::Under>, String> {
        self.under_call(command, params)
    }

    fn covered(&self, command: &str, params: &Value, objects: &[kition_kit::CoveredAsk]) -> Result<Option<kition_kit::CoveredAt>, String> {
        self.covered_call(command, params, objects)
    }

    fn derived(&self, ids: &[Value], names: &[String]) -> Result<Vec<kition_kit::DerivedValues>, String> {
        self.derived_values(ids, names)
    }

    fn carried(&self, ids: &[Value], changes: &[Value]) -> Result<Vec<kition_kit::Carried>, String> {
        self.carried_values(ids, changes)
    }
    fn editor_state(&self) -> Value {
        let mut state = serde_json::to_value(self.session.editor_snapshot()).unwrap_or_else(|error| json!({"snapshotError":error.to_string()}));
        if let Some(state) = state.as_object_mut() {
            state.insert(OWNER.into(), json!(self.editor_owner));
        }
        state
    }

    /// Puts back a state [`KitDocument::editor_state`] answered. This document's own state is put
    /// back as it is (what a call deleted is left out). A state of another document, such as the
    /// session a file reread replaces, is refused, naming what it is, when its selection names
    /// an item, story or table this document lacks: no default is chosen in its place.
    fn restore_editor_state(&mut self, saved: &Value) -> Result<(), String> {
        let mut saved = saved.clone();
        let owner = saved.as_object_mut().and_then(|state| state.remove(OWNER)).and_then(|owner| owner.as_u64());
        let snapshot: kition_layout_engine::EditorSnapshot =
            serde_json::from_value(saved).map_err(|error| format!("invalid editor snapshot: {error}"))?;
        if owner != Some(self.editor_owner)
            && let Some(reason) = snapshot.unresolved_in(&self.active()?.doc)
        {
            return Err(reason);
        }
        self.session.restore_editor(snapshot);
        Ok(())
    }

    /// A clean outside reread: imports only the file difference into this session's replica as
    /// one `import` transaction outside both undo histories, keeping object identities, the editor
    /// state and the blobs. Refused, with this session left as it was, when the editor state names
    /// something the file no longer has.
    fn reconcile_reread(&mut self, fresh: &Self) -> Result<bool, String> {
        let after = fresh.active()?.doc.clone();
        if let Some(reason) = self.session.editor_unresolved_in(&after) {
            return Err(reason);
        }
        self.session.active_mut().ok_or_else(|| "no document is open".to_owned())?.reconcile_file(&after).map_err(engine_error)?;
        // What an earlier save left for `mark_saved` is not what the file holds now.
        if let Ok(mut saving) = self.saving.lock() {
            *saving = None;
        }
        if let Ok(state) = self.active() {
            place::history::remember(&state.doc);
        }
        Ok(true)
    }

    fn set_context(&mut self, context: &kition_kit::CallContext) -> Result<(), String> {
        if context.part.is_some() || context.time.is_some() {
            return Err("invalid-params: Layout has no current page or time; give a command's explicit spread/page parameters".to_owned());
        }
        if !self.session.context_available() {
            return Err("the editor is in a live gesture or data preview; finish it before an agent call".to_owned());
        }
        let params = addresses::resolve("select.set", &json!({"ids":context.on}), self.session.kdm().map(|kdm| kdm.replica()))?;
        let ids: Vec<_> = params["ids"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|value| {
                let id = value.as_u64().map(kition_layout_engine::doc::ItemId).ok_or_else(|| format!("invalid-params: {value} is not an item id"))?;
                self.active()?.doc.item(id).ok_or_else(|| format!("invalid-params: no item {value}"))?;
                Ok(id)
            })
            .collect::<Result<_, String>>()?;
        if !context.on.is_empty() {
            let state = self.session.active_mut().ok_or_else(|| "no document is open".to_owned())?;
            state.selection = kition_layout_engine::doc::Selection::items(ids);
        }
        Ok(())
    }
    /// What is wrong with the document (KHP A21): its live preflight, see [`check`].
    fn check(&self) -> Vec<kition_kit::diagnostics::Diagnostic> {
        check::check(&self.session)
    }

    /// Opens a `.klayout` file (its content: the document's identity and object ids travel with
    /// the file, its history stays with the host, rule M16b), or an IDML package (imported into a
    /// new replica, never written in place).
    fn open(name: &str, bytes: &[u8]) -> Result<Self, String> {
        let mut session = files::session();
        if kition_layout_kdm::is_klayout(bytes) {
            session.add_document(DocState::open_klayout(bytes, None).map_err(engine_error)?);
        } else if kition_layout_idml::is_idml(bytes) {
            let base64 = kition_layout_engine::cmd::base64_encode(bytes);
            session.execute("file.openBytes", &json!({ "name": name, "base64": base64 })).map_err(engine_error)?;
            session.version().map_err(engine_error)?;
            if let Some(state) = session.active_mut() {
                state.mark_saved();
            }
        } else {
            return Err("not-a-layout: neither a .klayout file nor an IDML package".to_owned());
        }
        let document = Self::from_session(session);
        if let Ok(state) = document.active() {
            place::history::remember(&state.doc);
        }
        Ok(document)
    }

    /// A new one-page layout of `width × height` points with half-inch margins.
    fn create(name: &str, width: u32, height: u32) -> Result<Self, String> {
        let mut session = files::session();
        let title = std::path::Path::new(name).file_stem().map_or_else(|| "Untitled".to_owned(), |stem| stem.to_string_lossy().into_owned());
        session
            .execute("file.new", &json!({ "width": width, "height": height, "pages": 1, "facingPages": false, "margins": 36, "title": title }))
            .map_err(engine_error)?;
        if let Some(state) = session.active_mut() {
            // A new document draws characters its fonts lack from fallback fonts, so text typed in
            // any script shows (upstream's `glyphFallback` preference; files that were saved with
            // it off keep it off).
            std::sync::Arc::make_mut(&mut state.doc).settings.glyph_fallback = true;
        }
        // The replica is made now, holding the new layout in its creating transaction.
        session.version().map_err(engine_error)?;
        if let Some(state) = session.active_mut() {
            state.mark_saved();
        }
        Ok(Self::from_session(session))
    }

    fn width(&self) -> u32 {
        self.bounds().width().round().max(1.0) as u32
    }

    fn height(&self) -> u32 {
        self.bounds().height().round().max(1.0) as u32
    }

    fn revision(&self) -> u64 {
        self.active().map_or(0, |state| state.revision)
    }

    fn dirty(&self) -> bool {
        self.active().is_ok_and(DocState::is_dirty)
    }

    /// Above 1:1 the layout is drawn at the zoom, so type stays sharp; below, at 1:1 and reduced
    /// exactly, as the contract requires.
    fn render(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let (width, height) = (width.clamp(1, MAX_RENDER_SIDE), height.clamp(1, MAX_RENDER_SIDE));
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        if scale >= 1.0 {
            return self.draw(x, y, scale, width, height);
        }
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let span_x = ((f64::from(width) / scale).ceil() as i32).max(1);
        let span_y = ((f64::from(height) / scale).ceil() as i32).max(1);
        self.render_area(x0, y0, x0.saturating_add(span_x), y0.saturating_add(span_y), width, height)
    }

    fn render_area(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let w = u32::try_from(x1.saturating_sub(x0).max(1)).unwrap_or(1).min(MAX_RENDER_SIDE);
        let h = u32::try_from(y1.saturating_sub(y0).max(1)).unwrap_or(1).min(MAX_RENDER_SIDE);
        let full = self.draw(f64::from(x0), f64::from(y0), 1.0, w, h)?;
        Ok(raster::render_area(Frame { rgba: &full, width: w, height: h }, 0, 0, w as i32, h as i32, width, height))
    }

    fn execute(&mut self, command: &str, params: Value) -> Result<Value, String> {
        self.refuse_unknown(command, &params)?;
        if command == render::ID {
            return render::run(self, &params);
        }
        let params = addresses::resolve(command, &params, self.session.kdm().map(|kdm| kdm.replica()))?;
        let answer = self.session.execute(command, &params).map_err(engine_error);
        if answer.is_ok()
            && let Ok(state) = self.active()
        {
            place::history::remember(&state.doc);
        }
        answer
    }

    /// The layout as JSON: pages, spreads, items, stories, styles, selection, and whether undo and
    /// redo have a step.
    fn inspect(&self) -> Result<Value, String> {
        let state = self.active()?;
        place::history::remember(&state.doc);
        let mut scratch = files::session();
        let mut copy = DocState::new((*state.doc).clone(), None);
        copy.selection = state.selection.clone();
        scratch.add_document(copy);
        let mut inspected = scratch.execute("file.inspect", &json!({})).map_err(engine_error)?;
        let origin = self.origin();
        let layout = self.session.layout();
        let spreads: Vec<Value> = layout
            .slots
            .iter()
            .filter_map(|slot| {
                let spread = state.doc.spread(slot.spread)?;
                let placed = slot.xf.transform_rect_bbox(spread.bounds());
                Some(json!({ "x": placed.x0 - origin.x, "y": placed.y0 - origin.y, "width": placed.width(), "height": placed.height(), "pages": spread.pages.len() }))
            })
            .collect();
        if let Some(object) = inspected.as_object_mut() {
            object.insert("canUndo".into(), Value::Bool(state.can_undo()));
            object.insert("canRedo".into(), Value::Bool(state.can_redo()));
            object.insert("canvas".into(), json!({ "spreads": spreads }));
            object.insert("tool".into(), Value::String(self.session.tool_id().to_owned()));
        }
        Ok(inspected)
    }

    fn commands(&self) -> Vec<KitCommand> {
        let mut commands: Vec<kition_kit::KitCommand> = {
            kition_layout_engine::cmd::command_specs()
                .iter()
                .map(|spec| {
                    KitCommand {
                        id: spec.id.to_owned(),
                        title: spec.label.trim_end_matches('…').to_owned(),
                        menu: (!spec.menu.is_empty()).then(|| spec.menu.iter().map(|part| (*part).to_owned()).collect()),
                        shortcut: spec.shortcut.map(str::to_owned),
                        params: Some(spec.params.to_owned()),
                        mutates: spec.journal,
                        enabled: spec.info(&self.session).enabled,
                        ..Default::default()
                    }
                    .with_kit_schema()
                })
                .collect()
        };
        commands.push(render::command());
        // Served for a host, the kit starts no program: printing is not there (KHP A45).
        if files::served() {
            commands.retain(|command| !files::AGENT_EXCLUDED.contains(&command.id.as_str()));
        }
        kition_kit::files::declare(&mut commands, files::FILES);
        describe::describe(&mut commands);
        for command in &mut commands {
            if let Some(schema) = command.schema.as_mut() {
                params::complete(&command.id, schema);
            }
        }
        addresses::declare(&mut commands);
        commands
    }

    /// The commands that cannot run now, without building the full list and its schemas: what
    /// Kition asks after every change for menu and shortcut states.
    fn disabled_commands(&self) -> Vec<String> {
        kition_layout_engine::cmd::command_specs()
            .iter()
            .filter(|spec| (spec.enabled)(&self.session).is_err())
            .map(|spec| spec.id.to_owned())
            .collect()
    }

    /// The layout as a `.klayout` file: the document's content (no history, rule M16b) and the
    /// images' blobs its state references, the same bytes while the document stays at the version
    /// saved; or as an IDML package when `name` says so.
    ///
    /// The content is the replica's (`content.json`, kition-sdk data model 7.4): it depends on the
    /// state alone, so an unchanged layout saves to the same bytes
    /// (`kition_layout_kdm::file::save_file`).
    fn save(&self, name: Option<&str>) -> Result<Vec<u8>, String> {
        let state = self.active()?;
        if name.is_some_and(|name| name.to_ascii_lowercase().ends_with(".idml")) {
            return Ok(kition_layout_idml::export_idml(&state.doc));
        }
        let kdm = state.kdm().ok_or_else(|| "the layout has no replica".to_owned())?;
        let saved = kition_layout_kdm::save_file(kdm, state.file_base()).map_err(engine_error)?;
        if let Ok(mut saving) = self.saving.lock() {
            *saving = Some(saved.base);
        }
        Ok(saved.bytes)
    }

    /// Records the last save as what the file holds (no edit: the state, the revision and the undo
    /// histories stay as they are).
    fn mark_saved(&mut self) {
        let saving = self.saving.get_mut().ok().and_then(Option::take);
        if let Some(state) = self.session.active_mut() {
            match saving {
                Some(base) => state.saved_as(base),
                None => state.mark_saved(),
            }
        }
    }
}

impl KitTools for LayoutDocument {
    fn tools(&self) -> Vec<ToolInfo> {
        kition_layout_tools::catalog::TOOL_GROUPS
            .iter()
            .filter(|tools| !tools.is_empty())
            .enumerate()
            .flat_map(|(group, tools)| {
                tools.iter().map(move |tool| ToolInfo {
                    id: tool.id.to_owned(),
                    label: tool.label.to_owned(),
                    shortcut: tool.shortcut.map(str::to_owned),
                    icon: tool.icon.to_owned(),
                    group,
                })
            })
            .collect()
    }

    fn tool(&self) -> String {
        self.session.tool_id().to_owned()
    }

    fn select_tool(&mut self, id: &str, zoom: f64) -> Result<ToolResponse, String> {
        // A tool a command started (the loaded text or place cursor, off the rail) stays as it is
        // when the editor confirms the engine's switch to it.
        if id == self.session.tool_id() {
            return Ok(self.respond(self.session.tool_id(), None, zoom));
        }
        if kition_layout_tools::catalog::tool_info(id).is_none() {
            return Err(format!("no tool {id}"));
        }
        self.session.set_tool(id);
        let tool = self.session.tool_id();
        Ok(self.respond(tool, None, zoom))
    }

    fn pointer(&mut self, event: &ToolPointer) -> Result<ToolResponse, String> {
        let at = self.to_canvas(event.x, event.y);
        let kind = match event.action {
            PointerAction::Down => PointerKind::Down,
            PointerAction::Drag => PointerKind::Drag,
            PointerAction::Up => PointerKind::Up,
            PointerAction::Move => PointerKind::Move,
            PointerAction::DoubleClick => PointerKind::DoubleClick,
        };
        self.hover = (event.action == PointerAction::Move).then_some(at);
        let before = self.session.tool_id();
        self.session.pointer(&PointerEvent::new(kind, at.x, at.y).with_mods(mods_in(event.mods)), Self::view(event.zoom)).map_err(engine_error)?;
        Ok(self.respond(before, Some(at), event.zoom))
    }

    fn key(&mut self, key: &str, mods: ToolMods, zoom: f64) -> Result<ToolResponse, String> {
        let before = self.session.tool_id();
        if let Some(engine_key) = key_in(key) {
            self.session.tool_key(engine_key, mods_in(mods), Self::view(zoom)).map_err(engine_error)?;
        }
        Ok(self.respond(before, None, zoom))
    }

    /// Typed text goes into the story at the caret, as the Type tool does.
    fn text(&mut self, text: &str, zoom: f64) -> Result<ToolResponse, String> {
        let before = self.session.tool_id();
        if self.session.wants_text() {
            // Typing joins one undo step per word (the editor ends it at a pause: `endTyping`).
            self.session.type_text(text).map_err(engine_error)?;
        }
        Ok(self.respond(before, None, zoom))
    }

    fn state(&mut self, x: f64, y: f64, _mods: ToolMods, zoom: f64) -> ToolResponse {
        let at = self.to_canvas(x, y);
        let before = self.session.tool_id();
        self.respond(before, Some(at), zoom)
    }
}

kition_kit::export_kit_document!(LayoutDocument as "LayoutDocument");

/// Adds a font file's faces (TTF, OTF, TTC) for text the embedded faces do not cover, such as
/// Chinese in a web build, which has no system fonts to fall back on (standard E9). Returns the
/// faces added.
pub fn add_font(bytes: &[u8]) -> usize {
    kition_layout_fonts::FontDb::global().add_font(bytes.to_vec())
}

impl LayoutDocument {
    /// Ends the typing in progress: the next text typed (`toolText`) is a new undo step.
    pub fn end_typing(&mut self) {
        self.session.end_typing();
    }

    /// The canvas around the spreads in `#rrggbb` (the editor's canvas color).
    pub fn set_backdrop(&mut self, color: &str) -> Result<(), String> {
        let hex = color.trim().trim_start_matches('#');
        let channel = |at: usize| hex.get(at..at + 2).and_then(|part| u8::from_str_radix(part, 16).ok());
        match (hex.len(), channel(0), channel(2), channel(4)) {
            (6, Some(r), Some(g), Some(b)) => {
                self.pasteboard = [r, g, b, 255];
                Ok(())
            }
            _ => Err(format!("{color} is not a #rrggbb color")),
        }
    }
}

/// Layout's own additions to the exported class, for its embedded editor.
#[cfg(target_arch = "wasm32")]
pub mod web {
    use kition_kit::wasm_bindgen::prelude::*;

    use crate::__kition_kit_export::WebDocument;

    /// Adds a font file's faces for text the embedded faces do not cover; returns how many.
    #[wasm_bindgen(js_name = addFont, wasm_bindgen = kition_kit::wasm_bindgen)]
    pub fn add_font(bytes: &[u8]) -> u32 {
        crate::add_font(bytes) as u32
    }

    fn error(message: String) -> JsError {
        JsError::new(&message)
    }

    /// An origin in the KDM's commit form, the JSON array `[kind, principal, label]`.
    fn origin_of(origin: &str) -> Result<kition_kit::kdm::Origin, JsError> {
        kition_kit::kdm::Origin::decode(origin).ok_or_else(|| error(format!("origin is not [kind, principal, label]: {origin}")))
    }

    #[wasm_bindgen(js_class = "LayoutDocument", wasm_bindgen = kition_kit::wasm_bindgen)]
    impl WebDocument {
        /// The canvas around the spreads in `#rrggbb` (the editor's canvas color).
        #[wasm_bindgen(js_name = setBackdrop)]
        pub fn set_backdrop(&mut self, color: &str) -> Result<(), JsError> {
            self.0.set_backdrop(color).map_err(error)
        }

        // Engine contract 2 (`crate::collab`): shapes mirror `kition-sdk`'s `session::web`.

        pub fn version(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.version().map_err(error)
        }

        #[wasm_bindgen(js_name = exportChanges)]
        pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.export_changes(since).map_err(error)
        }

        #[wasm_bindgen(js_name = importChanges)]
        pub fn import_changes(&mut self, bytes: &[u8]) -> Result<String, JsError> {
            self.0.import_changes(bytes).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        pub fn snapshot(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.snapshot().map_err(error)
        }

        pub fn undo(&mut self) -> Result<String, JsError> {
            self.0.undo().map(|(done, damage)| serde_json::json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        pub fn redo(&mut self) -> Result<String, JsError> {
            self.0.redo().map(|(done, damage)| serde_json::json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        /// Ends the typing in progress, so the next text typed is a new undo step: the editor
        /// calls it when the person pauses.
        #[wasm_bindgen(js_name = endTyping)]
        pub fn end_typing(&mut self) {
            self.0.end_typing();
        }

        #[wasm_bindgen(js_name = canUndo)]
        pub fn can_undo(&self) -> bool {
            self.0.can_undo()
        }

        #[wasm_bindgen(js_name = canRedo)]
        pub fn can_redo(&self) -> bool {
            self.0.can_redo()
        }

        /// `origin` is the KDM's commit form, the JSON array `[kind, principal, label]`.
        #[wasm_bindgen(js_name = executeAs)]
        pub fn execute_as(&mut self, command: &str, params: &str, origin: &str) -> Result<String, JsError> {
            let params = kition_kit::parse_params(params).map_err(error)?;
            let origin = origin_of(origin)?;
            self.0.execute_as(command, params, &origin).map(|value| value.to_string()).map_err(error)
        }

        /// A draft's changes taken in as one undo step, recorded with `origin` (`[kind, principal,
        /// label]`): `{"damage", "summary"}`. A refusal fails with the KDM's message.
        #[wasm_bindgen(js_name = mergeChanges)]
        pub fn merge_changes(&mut self, changes: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.merge_changes(changes, &origin).map(|answer| answer.to_string()).map_err(error)
        }

        /// What merging a draft's changes would change now, as `StateDiff` JSON; changes nothing.
        #[wasm_bindgen(js_name = compareChanges)]
        pub fn compare_changes(&self, changes: &[u8]) -> Result<String, JsError> {
            self.0.compare_changes(changes).map(|answer| answer.to_string()).map_err(error)
        }

        /// Writes the state a snapshot holds over the layout as one change of `origin`; answers
        /// the damage JSON.
        #[wasm_bindgen(js_name = restoreState)]
        pub fn restore_state(&mut self, snapshot: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.restore_state(snapshot, &origin).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        /// An agent's reply (JSON text) with its object ids named as the tool interface names them.
        #[wasm_bindgen(js_name = modelReply)]
        pub fn model_reply(&self, value: &str) -> Result<String, JsError> {
            self.0.model_reply(value).map_err(error)
        }

        /// What changed since the version `since` (a receipt): `{"changes": StateDiff}`, or
        /// `{"changes": null, "unavailable"}` when the replica's log does not hold it.
        #[wasm_bindgen(js_name = changesSince)]
        pub fn changes_since(&self, since: &[u8]) -> String {
            self.0.changes_since(since).to_string()
        }

        /// `model/describe` as JSON text.
        #[wasm_bindgen(js_name = modelDescribe)]
        pub fn model_describe(&mut self) -> Result<String, JsError> {
            self.0.model_describe().map(|answer| answer.to_string()).map_err(error)
        }

        /// `model/batch`: the request and the answer as JSON text; a failure's message is the
        /// error's JSON (`{"code", "index", "message"}`).
        #[wasm_bindgen(js_name = modelBatch)]
        pub fn model_batch(&mut self, request: &str) -> Result<String, JsError> {
            let request: serde_json::Value = serde_json::from_str(request).map_err(|parse| {
                error(serde_json::json!({ "code": "invalid-op", "index": null, "message": format!("the batch is not JSON: {parse}") }).to_string())
            })?;
            self.0.model_batch(&request).map(|answer| answer.to_string()).map_err(error)
        }

        /// Every blob the replica holds, for a peer's `addBlobs`.
        pub fn blobs(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.blobs().map_err(error)
        }

        #[wasm_bindgen(js_name = addBlobs)]
        pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), JsError> {
            self.0.add_blobs(bytes).map_err(error)
        }

        /// The blobs newly needed since `since` (empty bytes: everything the state references).
        #[wasm_bindgen(js_name = blobsSince)]
        pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.blobs_since(since).map_err(error)
        }

        #[wasm_bindgen(js_name = kdmResets)]
        pub fn kdm_resets(&self) -> u32 {
            self.0.kdm_resets()
        }

        #[wasm_bindgen(js_name = schemaVersion)]
        pub fn schema_version(&self) -> u32 {
            self.0.schema_version()
        }

        #[wasm_bindgen(js_name = authorSchemaVersion)]
        pub fn author_schema_version(&self) -> Option<u32> {
            self.0.author_schema_version()
        }

        /// Opens a file with the replica a host stored for it: a `.klayout` file of the same
        /// document is taken into the stored replica, and empty `bytes` open the replica alone.
        #[wasm_bindgen(js_name = openWithReplica)]
        pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<WebDocument, JsError> {
            crate::LayoutDocument::open_with_replica(name, bytes, snapshot, blobs).map(Self).map_err(error)
        }

        #[wasm_bindgen(getter, js_name = hasReplica)]
        pub fn has_replica(&self) -> bool {
            self.0.has_replica()
        }

        /// The replica's own document id, for the Kit UI protocol's `replica/info`.
        #[wasm_bindgen(getter, js_name = docId)]
        pub fn doc_id(&self) -> Option<String> {
            self.0.doc_id()
        }
    }
}
kition_kit::export_kit_tools!(LayoutDocument as "LayoutDocument");

#[cfg(test)]
mod tests {
    use super::*;
    use kition_kit::testing::check_contract;

    /// A Letter page with a red rectangle near its top-left.
    fn fixture() -> Vec<u8> {
        let mut doc = LayoutDocument::create("Flyer.klayout", 612, 792).unwrap();
        doc.execute("swatch.add", json!({ "name": "Red", "color": "#ff0000" })).unwrap();
        let frame = doc.execute("frame.add", json!({ "rect": [72, 72, 216, 144], "content": "unassigned" })).unwrap();
        doc.execute("object.fill", json!({ "swatch": "Red", "ids": [frame["id"]] })).unwrap();
        doc.save(None).unwrap()
    }

    #[test]
    fn keeps_the_kit_contract() {
        check_contract::<LayoutDocument>(&fixture(), "Flyer.klayout", |doc| {
            let frame = doc.execute("frame.add", json!({ "rect": [300, 300, 400, 400], "content": "unassigned" }))?;
            doc.execute("object.fill", json!({ "swatch": "Black", "ids": [frame["id"]] }))?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn takes_a_font_for_text_its_own_faces_do_not_cover() {
        assert_eq!(add_font(b"not a font"), 0);
        // A synthetic family, never one of the engine's bundled or kition-fonts faces, so this is
        // deterministically new regardless of what KITION_FONTS_DIR points at.
        let bytes = kition_layout_fonts::testing::font_with("Host-Served Test Face", &['A']).unwrap();
        assert!(add_font(&bytes) > 0);
        // A face it already has is not added twice.
        assert_eq!(add_font(&bytes), 0);
        // New documents draw missing glyphs from fallback fonts.
        let doc = LayoutDocument::create("New.klayout", 612, 792).unwrap();
        assert!(doc.session.doc().unwrap().doc.settings.glyph_fallback);
    }

    #[test]
    fn renders_the_page_on_the_pasteboard() {
        let doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        assert_eq!((doc.width(), doc.height()), (612 + 144, 792 + 144));
        let at = |x: u32, y: u32| doc.render(f64::from(x), f64::from(y), 1.0, 1, 1).unwrap();
        assert_eq!(at(10, 10), vec![243, 243, 246, 255], "pasteboard");
        assert_eq!(at(72 + 36, 72 + 300), vec![255, 255, 255, 255], "paper");
        assert_eq!(at(72 + 100, 72 + 100), vec![255, 0, 0, 255], "the rectangle");
        // An editor's canvas color replaces the pasteboard's.
        let mut doc = doc;
        doc.set_backdrop("#f6f5f4").unwrap();
        assert!(doc.set_backdrop("blue").is_err());
        assert_eq!(doc.render(10.0, 10.0, 1.0, 1, 1).unwrap(), vec![0xf6, 0xf5, 0xf4, 255]);
    }

    #[test]
    fn the_type_tool_takes_text_into_a_new_frame() {
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        assert!(doc.tools().iter().any(|tool| tool.id == "type"));
        doc.select_tool("type", 1.0).unwrap();
        let event = |action, x, y| ToolPointer { action, x, y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 };
        doc.pointer(&event(PointerAction::Down, 72.0 + 72.0, 72.0 + 300.0)).unwrap();
        doc.pointer(&event(PointerAction::Drag, 72.0 + 400.0, 72.0 + 400.0)).unwrap();
        let up = doc.pointer(&event(PointerAction::Up, 72.0 + 400.0, 72.0 + 400.0)).unwrap();
        assert!(up.wants_text, "a caret in the new frame: {up:?}");
        let typed = KitTools::text(&mut doc, "Hello", 1.0).unwrap();
        assert!(typed.overlays.iter().any(|overlay| matches!(overlay, ToolOverlay::Line { .. })), "the caret: {:?}", typed.overlays);
        let inspected = doc.inspect().unwrap().to_string();
        assert!(inspected.contains("Hello"), "{inspected}");
    }

    /// The out port's click loads the text cursor; the editor's confirming `selectTool` keeps
    /// it, and a click on an empty frame threads it.
    #[test]
    fn the_out_port_click_threads_through_the_tools() {
        let mut doc = LayoutDocument::create("Story.klayout", 612, 792).unwrap();
        let text = "Words keep coming and the frame fills up quickly with them. ".repeat(12);
        let a = doc.execute("frame.add", json!({ "rect": [100, 100, 300, 200], "content": "text", "text": text, "caret": false })).unwrap()["id"]
            .as_u64()
            .unwrap();
        let b = doc.execute("frame.add", json!({ "rect": [100, 300, 300, 500], "content": "unassigned" })).unwrap()["id"].as_u64().unwrap();
        doc.execute("select.set", json!({ "ids": [a] })).unwrap();
        doc.select_tool("selection", 1.0).unwrap();
        let event = |action, x: f64, y: f64| ToolPointer { action, x: 72.0 + x, y: 72.0 + y, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 };
        let shown = doc.state(72.0 + 300.0, 72.0 + 188.0, ToolMods::default(), 1.0);
        assert!(shown.overlays.iter().any(|o| matches!(o, ToolOverlay::Line { color, .. } if *color == [230, 20, 20, 255])), "the overset plus");
        doc.pointer(&event(PointerAction::Down, 300.0, 188.0)).unwrap();
        let up = doc.pointer(&event(PointerAction::Up, 300.0, 188.0)).unwrap();
        assert!(up.requests.iter().any(|r| matches!(r, ToolRequest::SwitchTool { id } if id == "loadedText")), "{:?}", up.requests);
        assert_eq!(up.cursor, "copy");
        assert!(up.busy, "Escape goes to the tool");
        let kept = doc.select_tool("loadedText", 1.0).unwrap();
        assert_eq!((doc.tool(), kept.cursor.as_str()), ("loadedText".to_owned(), "default"));
        doc.pointer(&event(PointerAction::Down, 200.0, 400.0)).unwrap();
        let done = doc.pointer(&event(PointerAction::Up, 200.0, 400.0)).unwrap();
        assert!(done.requests.iter().any(|r| matches!(r, ToolRequest::SwitchTool { id } if id == "selection")));
        let story = doc.execute("story.get", json!({ "frame": a })).unwrap();
        assert_eq!(story["frames"], json!([a, b]), "{story}");
        assert!(doc.select_tool("loadedText", 1.0).is_err(), "not on the rail once unloaded");
    }

    #[test]
    fn the_selection_shows_as_overlays() {
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        doc.select_tool("selection", 1.0).unwrap();
        let event = |action| ToolPointer { action, x: 72.0 + 100.0, y: 72.0 + 100.0, mods: ToolMods::default(), pressure: 1.0, zoom: 1.0 };
        doc.pointer(&event(PointerAction::Down)).unwrap();
        let up = doc.pointer(&event(PointerAction::Up)).unwrap();
        let handles = up.overlays.iter().filter(|overlay| matches!(overlay, ToolOverlay::Anchor { .. })).count();
        assert!(handles >= 8, "eight handles: {:?}", up.overlays);
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert!(LayoutDocument::open("Broken.klayout", b"not a layout").is_err());
        let mut doc = LayoutDocument::open("Flyer.klayout", &fixture()).unwrap();
        assert!(doc.execute("no.such.command", json!({})).is_err());
        assert!(doc.select_tool("no-such-tool", 1.0).is_err());
    }
}
