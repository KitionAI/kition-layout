//! Type tool (T): click in a frame to place the caret, drag in a frame to select text, drag on
//! empty canvas to draw a new text frame. Keys move the caret / delete; typing goes through
//! `text.insert` (sent by the UI as text input).
//!
//! Two, three, four and five presses in a row select the word, the line, the paragraph and the
//! story under the pointer (`text.selectWord` with its `unit`), and a drag after them grows the
//! selection by that unit. A click in an empty frame turns it into a text frame; a drag that
//! starts there draws a new text frame over it. Dragging a table's row or column border resizes
//! it (`table.resizeBorder`, Shift keeping the table's size). In a table cell, Shift+Tab goes back
//! a cell and Esc switches between the cell's text and the cell. Alt+Left/Right kerns the pair at
//! the caret or tracks the selection (with Cmd, five times as much).
//!
//! Input methods: the marked text of a composition is a preview of `text.insert` at the caret
//! (so it lays out in place like typed text), underlined, until the composition ends. The
//! committed text then arrives as typed text (one undo step); a cancelled composition leaves
//! nothing, and no undo step.

use std::ops::Range;

use kition_layout_compose as compose;
use kition_layout_doc::story::floor_char_boundary;
use kition_layout_doc::{ItemId, Kerning, TextSel};
use kition_layout_geom::{Affine, Point, Vec2};
use serde_json::json;

use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext, ToolKey, frame::drag_rect, rect_json, spread_json};

/// Kerning and tracking change by this much per Alt+arrow, in 1/1000 em (InDesign's default
/// keyboard increment for kerning).
pub const KERN_STEP: f64 = 20.0;

/// Input-method marked text shown at the caret through a preview of `text.insert`, not yet typed.
#[derive(Clone, Debug, PartialEq)]
struct Preedit {
    text: String,
    /// The clause being converted (empty: the input method's cursor), in bytes of `text`.
    active: Option<Range<usize>>,
}

#[derive(Default)]
pub struct TypeTool {
    /// Vertical Type Tool: frames it draws set text vertically.
    pub vertical: bool,
    start: Option<Point>,
    selecting: Option<u64>,
    drawing: bool,
    /// A press on an empty frame: (frame, press point in spread space). A click turns it into a
    /// text frame with the caret in it; a drag draws a new text frame over it, as in InDesign.
    convert: Option<(u64, Point)>,
    /// A press on a table whose whole rows or columns are selected: (frame, press point in
    /// spread space) — a drag moves them, a click places the caret.
    cell_drag: Option<(u64, Point)>,
    /// The press selected a word, line, paragraph or story (several clicks): a drag extends the
    /// selection by that unit.
    unit: Option<&'static str>,
    preedit: Option<Preedit>,
    /// A press on a table row or column border: a drag resizes it.
    border: Option<BorderDrag>,
    /// How many presses in a row the last press completed. A double click the editor also
    /// reports on its own (after the second press's release, even after a drag, and again after
    /// the fourth) was handled by that press: it changes nothing.
    pressed: u8,
}

/// A table row or column border under the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BorderHit {
    table: u64,
    /// A column border (else a row's).
    column: bool,
    /// The column or row the border ends (the last one is the table's edge).
    index: usize,
    /// A right-to-left table: its columns run leftwards, so a column border moves the other way.
    mirrored: bool,
}

/// A table border pressed, and the frame's canvas → inner-space mapping.
#[derive(Clone, Copy)]
struct BorderDrag {
    frame: u64,
    story: u64,
    hit: BorderHit,
    /// Canvas → frame inner space (through rotations, scaling and turned spreads).
    inv: Affine,
    /// Press point in frame inner space.
    start: Point,
    begun: bool,
    /// The border runs vertically on the canvas (column borders on an unrotated frame).
    horizontal_drag: bool,
}

/// The row or column border within `tol` (inner space) of inner point `p` in frame `fi`'s tables.
fn hit_border(cs: &compose::ComposedStory, fi: usize, story: &kition_layout_doc::Story, p: Point, tol: f64) -> Option<BorderHit> {
    let ft = cs.frames.get(fi)?;
    let mut best: Option<(f64, BorderHit)> = None;
    for frag in &ft.tables {
        let Some(table) = story.tables.get(&frag.table) else { continue };
        let grown = frag.rect.inflate(tol, tol);
        if !grown.contains(p) {
            continue;
        }
        let mirrored = table.options.direction == kition_layout_doc::TextDirection::RightToLeft;
        for c in &frag.cells {
            let (col_span, row_span) = table.cell(c.row, c.col).map_or((1, 1), |cell| (cell.col_span.max(1) as usize, cell.row_span.max(1) as usize));
            // The column border after this cell: its right edge (its left in a right-to-left table).
            let x = if mirrored { c.rect.x0 } else { c.rect.x1 };
            let dx = (p.x - x).abs();
            if dx <= tol && p.y >= c.rect.y0 - tol && p.y <= c.rect.y1 + tol && best.is_none_or(|(d, _)| dx < d) {
                let index = c.col.saturating_add(col_span - 1);
                best = Some((dx, BorderHit { table: frag.table, column: true, index, mirrored }));
            }
            let dy = (p.y - c.rect.y1).abs();
            if dy <= tol && p.x >= c.rect.x0 - tol && p.x <= c.rect.x1 + tol && best.is_none_or(|(d, _)| dy < d) {
                let index = c.row.saturating_add(row_span - 1);
                best = Some((dy, BorderHit { table: frag.table, column: false, index, mirrored }));
            }
        }
    }
    best.map(|(_, hit)| hit)
}

/// The row or column border of a table near canvas point `pos`, in an unlocked text frame.
fn border_at(cx: &ToolContext, pos: Point) -> Option<BorderDrag> {
    let (_, id) = cx.hit(pos)?;
    let it = cx.doc.item(id)?;
    if it.locked {
        return None;
    }
    let sid = it.text_frame()?.story;
    let story = cx.doc.story(sid)?;
    if story.tables.is_empty() {
        return None;
    }
    let xf = cx.item_canvas_xf(id)? * cx.doc.text_xf(it);
    let det = xf.determinant().abs();
    if !(det.is_finite() && det > 1e-12) {
        return None;
    }
    let inv = xf.inverse();
    let start = inv * pos;
    let cs = cx.cache.get(cx.doc, sid, None);
    let fi = cs.frames.iter().position(|f| f.frame == id)?;
    let hit = hit_border(&cs, fi, story, start, cx.tol(4.0) / det.sqrt())?;
    let axis = if hit.column { Point::new(1.0, 0.0) } else { Point::new(0.0, 1.0) };
    let v = (xf * axis) - (xf * Point::ORIGIN);
    Some(BorderDrag { frame: id.0, story: sid.0, hit, inv, start, begun: false, horizontal_drag: v.x.abs() >= v.y.abs() })
}

impl BorderDrag {
    fn cursor(&self) -> Cursor {
        if self.horizontal_drag { Cursor::ResizeH } else { Cursor::ResizeV }
    }

    /// Begin (once) and preview the resize for the pointer at canvas `pos`; `keep` keeps the
    /// table's size (Shift).
    fn drag(&mut self, pos: Point, keep: bool) -> Vec<Action> {
        let d = (self.inv * pos) - self.start;
        let mut delta = if self.hit.column { d.x } else { d.y };
        if self.hit.mirrored && self.hit.column {
            delta = -delta;
        }
        if !delta.is_finite() {
            return vec![];
        }
        let mut out = vec![];
        if !self.begun {
            self.begun = true;
            out.push(Action::Begin(if self.hit.column { "Resize Column" } else { "Resize Row" }.into()));
        }
        out.push(Action::Preview(
            "table.resizeBorder".into(),
            json!({
                "frame": self.frame,
                "story": self.story,
                "table": self.hit.table,
                "axis": if self.hit.column { "column" } else { "row" },
                "index": self.hit.index,
                "delta": delta,
                "keepTableSize": keep,
            }),
        ));
        out
    }
}

/// Are whole rows or whole columns of a table selected?
fn whole_rows_or_cols(cx: &ToolContext) -> bool {
    let Some(ts) = cx.selection.cells else { return false };
    let Some(t) = cx.doc.story(ts.story).and_then(|s| s.tables.get(&ts.table)) else { return false };
    let r = ts.range;
    (r.c0 == 0 && r.c1 + 1 == t.ncols()) || (r.r0 == 0 && r.r1 + 1 == t.nrows())
}

impl TypeTool {
    pub fn vertical() -> Self {
        TypeTool { vertical: true, ..Default::default() }
    }
}

/// Byte range in `s` of the characters `r` (input-method ranges count characters; carets count
/// bytes). `None` when `r` runs past the end of `s` or backwards.
pub(crate) fn char_range_to_bytes(s: &str, r: Range<usize>) -> Option<Range<usize>> {
    let n = s.chars().count();
    let byte = |c: usize| if c == n { Some(s.len()) } else { s.char_indices().nth(c).map(|(i, _)| i) };
    let (a, b) = (byte(r.start)?, byte(r.end)?);
    (a <= b).then_some(a..b)
}

/// Text space → canvas for frame `fid` (with the quarter turn of a vertical frame).
fn text_canvas_xf(cx: &ToolContext, fid: ItemId) -> Option<Affine> {
    Some(cx.item_canvas_xf(fid)? * cx.doc.text_xf(cx.doc.item(fid)?))
}

/// Text space → spread for frame `fid` (the space `text.placeCaret`'s point is in).
fn text_spread_xf(cx: &ToolContext, fid: ItemId) -> Option<Affine> {
    let loc = cx.doc.find(fid)?;
    Some(cx.doc.parent_xf(&loc) * cx.doc.text_xf(cx.doc.item(fid)?))
}

/// Where the marked text starts: the preview typed it just before the caret.
fn marked_start(cx: &ToolContext, ts: TextSel, p: &Preedit) -> usize {
    let text = cx.doc.text_story(ts.story, ts.cell).map_or("", |s| s.text.as_str());
    floor_char_boundary(text, ts.focus.saturating_sub(p.text.len()))
}

/// The caret line (top, bottom) at byte `pos` of the selection's text, in canvas space; `None`
/// when `pos` isn't composed (overset).
fn caret_line(cx: &ToolContext, ts: TextSel, pos: usize) -> Option<(Point, Point)> {
    let cs = cx.cache.get(cx.doc, ts.story, None);
    let (fi, x, bl, asc, desc) = match ts.cell {
        Some(c) => match c.footnote_id() {
            Some(id) => compose::note_caret(&cs, id, pos)?,
            None => compose::cell_caret(&cs, c.table, c.row, c.col, pos)?,
        },
        None => compose::caret(&cs, pos)?,
    };
    let ft = cs.frames.get(fi)?;
    let m = text_canvas_xf(cx, ft.frame)?;
    Some((m * Point::new(x, bl - asc), m * Point::new(x, bl + desc)))
}

/// Underlines (start, end in canvas space) under bytes `r` of the selection's text.
fn underlines(cx: &ToolContext, ts: TextSel, r: Range<usize>) -> Vec<(Point, Point)> {
    let mut out = Vec::new();
    let mut add = |lines: &[compose::Line], m: Affine| {
        for l in lines {
            let (s, e) = (r.start.max(l.range.start), r.end.min(l.range.end));
            if s < e {
                out.extend(compose::highlight_quads(l, s, e, false).into_iter().map(|q| (m * q[3], m * q[2])));
            }
        }
    };
    let cs = cx.cache.get(cx.doc, ts.story, None);
    match ts.cell {
        // A cell's or footnote's text is composed on its own and placed in the frame.
        Some(c) => {
            let found = match c.footnote_id() {
                Some(id) => compose::find_note(&cs, id).map(|(fi, n)| (fi, n.origin, n.text.clone())),
                None => compose::find_cell(&cs, c.table, c.row, c.col).map(|(fi, _, pc)| (fi, pc.origin, pc.text.clone())),
            };
            if let Some((fi, origin, text)) = found
                && let Some(m) = cs.frames.get(fi).and_then(|ft| text_canvas_xf(cx, ft.frame))
                && let Some(ft) = text.frames.first()
            {
                add(&ft.lines, m * Affine::translate(origin.to_vec2()));
            }
        }
        None => {
            for ft in &cs.frames {
                if let Some(m) = text_canvas_xf(cx, ft.frame) {
                    add(&ft.lines, m);
                }
            }
        }
    }
    out
}

/// Alt+Left/Right: the pair at the caret kerned, or the selection tracked, by `steps` ×
/// [`KERN_STEP`] (from the pair's manual kerning, else 0; from the first selected character's
/// tracking). One `text.char`, one undo step.
fn kern_step(cx: &ToolContext, steps: f64) -> Vec<Action> {
    let Some(t) = cx.selection.text else { return vec![] };
    let Some(story) = cx.doc.text_story(t.story, t.cell) else { return vec![] };
    let text = story.text.as_str();
    let amount = steps * KERN_STEP;
    // Kerning and tracking range, in 1/1000 em.
    let clamp = |v: f64| v.clamp(-1000.0, 10000.0);
    let r = t.range();
    let resolved = |at: usize| {
        let para = story.paras.get(story.para_at(at)).cloned().unwrap_or_default();
        cx.doc.styles.resolve_char(&cx.doc.styles.resolve_para(&para).1, story.format_after(at))
    };
    if r.is_empty() {
        // Manual kerning sits on the character before the caret (it applies after it).
        let pos = floor_char_boundary(text, r.start);
        let Some(c) = text.get(..pos).and_then(|before| before.chars().next_back()) else { return vec![] };
        if c == '\n' || kition_layout_doc::is_break_char(c) || text.get(pos..).is_none_or(|after| after.is_empty() || after.starts_with('\n')) {
            return vec![];
        }
        let current = match resolved(pos - c.len_utf8()).kerning {
            Kerning::Manual(v) if v.is_finite() => v,
            _ => 0.0,
        };
        let v = clamp(current + amount);
        return vec![Action::Exec("text.char".into(), json!({"attrs": {"kerning": {"kind": "manual", "value": v}}}))];
    }
    let current = resolved(floor_char_boundary(text, r.start)).tracking;
    let current = if current.is_finite() { current } else { 0.0 };
    vec![Action::Exec("text.char".into(), json!({"attrs": {"tracking": clamp(current + amount)}}))]
}

/// Esc in a table cell: the caret or text in a cell selects that cell (`table.select`); selected
/// cells select the text of the first one, as InDesign does.
fn toggle_cell_selection(cx: &ToolContext) -> Vec<Action> {
    let Some(ts) = cx.selection.cells else { return vec![Action::Exec("table.select".into(), json!({}))] };
    let cs = cx.cache.get(cx.doc, ts.story, None);
    let Some((fi, _, cell)) = compose::find_cell(&cs, ts.table, ts.range.r0, ts.range.c0) else { return vec![] };
    let Some(frame) = cs.frames.get(fi).map(|f| f.frame) else { return vec![] };
    let Some(m) = text_spread_xf(cx, frame) else { return vec![] };
    let at = m * cell.rect.center();
    vec![Action::Exec("text.selectWord".into(), json!({"frame": frame.0, "point": [at.x, at.y], "unit": "story"}))]
}

impl TypeTool {
    fn pointer_down(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        self.start = Some(ev.pos);
        self.drawing = false;
        self.selecting = None;
        self.cell_drag = None;
        self.unit = None;
        self.border = None;
        self.convert = None;
        if ev.kind == PointerKind::Down
            && let Some(b) = border_at(cx, ev.pos)
        {
            self.border = Some(b);
            return vec![];
        }
        // Text frames take the caret (locked ones too, as in InDesign); an empty frame becomes
        // one, unless it's locked: then the press draws a new text frame over it.
        if let Some((_, id)) = cx.hit(ev.pos)
            && let Some(it) = cx.doc.item(id)
            && !matches!(it.content, kition_layout_doc::Content::Graphic(_) | kition_layout_doc::Content::Group { .. })
            && (it.is_text_frame() || !it.locked)
        {
            let sp = cx.layout.spread_at(ev.pos).map(|(_, p)| p).unwrap_or(ev.pos);
            let clicks = ev.click_count();
            // Whole rows/columns selected: wait to see whether this is a drag.
            if clicks == 1 && !ev.mods.shift && whole_rows_or_cols(cx) {
                self.cell_drag = Some((id.0, sp));
                return vec![];
            }
            // An empty frame: wait to see whether this is a click or a drag.
            if clicks == 1 && !ev.mods.shift && !it.is_text_frame() {
                self.convert = Some((id.0, sp));
                return vec![];
            }
            self.selecting = Some(id.0);
            // Two presses select a word (or a run of spaces), three a line, four a paragraph,
            // five the story.
            if clicks >= 2 {
                let unit = match clicks {
                    2 => "word",
                    3 => "line",
                    4 => "paragraph",
                    _ => "story",
                };
                self.unit = Some(unit);
                return vec![Action::Exec("text.selectWord".into(), json!({"frame": id.0, "point": [sp.x, sp.y], "unit": unit}))];
            }
            let cmd = if ev.mods.shift { "text.extendTo" } else { "text.placeCaret" };
            return vec![Action::Exec(cmd.into(), json!({"frame": id.0, "point": [sp.x, sp.y]}))];
        }
        vec![]
    }

    fn pointer_drag(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let Some(a) = self.start else { return vec![] };
        if let Some(b) = self.border.as_mut() {
            // A click with a little wobble must not resize.
            if !b.begun && (ev.pos - a).hypot() < cx.tol(3.0) {
                return vec![];
            }
            return b.drag(ev.pos, ev.mods.shift);
        }
        if self.cell_drag.is_some() {
            return vec![];
        }
        if let Some(fid) = self.selecting {
            let sp = cx.layout.spread_at(ev.pos).map(|(_, p)| p).unwrap_or(ev.pos);
            let mut p = json!({"frame": fid, "point": [sp.x, sp.y]});
            // After several clicks the drag grows the selection by whole units.
            if let Some(unit) = self.unit {
                p["unit"] = json!(unit);
            }
            return vec![Action::Exec("text.extendTo".into(), p)];
        }
        if (ev.pos - a).hypot() < cx.tol(3.0) && !self.drawing {
            return vec![];
        }
        let Some((sr, sa)) = cx.layout.spread_at(a) else { return vec![] };
        let r = drag_rect(sa, cx.layout.to_spread(sr, ev.pos), ev.mods);
        let mut out = vec![];
        if !self.drawing {
            self.drawing = true;
            self.convert = None;
            out.push(Action::Begin("Create Text Frame".into()));
        }
        out.push(Action::Preview(
            "frame.add".into(),
            json!({"spread": spread_json(sr), "shape": "rectangle", "content": "text", "rect": rect_json(r), "caret": true, "vertical": self.vertical}),
        ));
        out
    }

    fn pointer_up(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let start = self.start.take();
        if let Some(b) = self.border.take() {
            return if b.begun { vec![Action::Commit] } else { vec![] };
        }
        if let Some((fid, from)) = self.cell_drag.take() {
            let sp = cx.layout.spread_at(ev.pos).map(|(_, p)| p).unwrap_or(ev.pos);
            let moved = start.is_some_and(|a| (ev.pos - a).hypot() >= cx.tol(3.0));
            return if moved {
                vec![Action::Exec("table.dropCells".into(), json!({"frame": fid, "from": [from.x, from.y], "to": [sp.x, sp.y]}))]
            } else {
                vec![Action::Exec("text.placeCaret".into(), json!({"frame": fid, "point": [from.x, from.y]}))]
            };
        }
        if std::mem::take(&mut self.drawing) {
            self.selecting = None;
            return vec![Action::Commit];
        }
        // A click in an empty frame: it becomes a text frame with the caret in it. A release
        // away from the press (with no drag in between) isn't a click: nothing changes.
        if let Some((fid, at)) = self.convert.take() {
            if start.is_some_and(|a| (ev.pos - a).hypot() >= cx.tol(3.0)) {
                return vec![];
            }
            return vec![Action::Exec("text.placeCaret".into(), json!({"frame": fid, "point": [at.x, at.y]}))];
        }
        // Ends a press in text: drops dragged text, or is ignored by the engine.
        if let Some(fid) = self.selecting.take() {
            let sp = cx.layout.spread_at(ev.pos).map(|(_, p)| p).unwrap_or(ev.pos);
            let moved = start.is_some_and(|a| (ev.pos - a).hypot() >= cx.tol(3.0));
            return vec![Action::Exec("text.release".into(), json!({"frame": fid, "point": [sp.x, sp.y], "moved": moved, "copy": ev.mods.alt}))];
        }
        vec![]
    }
}

impl Tool for TypeTool {
    fn id(&self) -> &'static str {
        if self.vertical { "verticalType" } else { "type" }
    }

    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        match ev.kind {
            PointerKind::DoubleClick if self.pressed >= 2 => vec![],
            PointerKind::Down => {
                self.pressed = ev.click_count();
                self.pointer_down(cx, ev)
            }
            PointerKind::DoubleClick => self.pointer_down(cx, ev),
            PointerKind::Drag => self.pointer_drag(cx, ev),
            PointerKind::Up => self.pointer_up(cx, ev),
            PointerKind::Move => vec![],
        }
    }

    fn key(&mut self, cx: &ToolContext, key: ToolKey, mods: Mods) -> Vec<Action> {
        if key == ToolKey::Escape
            && let Some(b) = self.border.take()
        {
            self.start = None;
            return if b.begun { vec![Action::Cancel] } else { vec![] };
        }
        // The input method owns the keys while it composes (the editor holds them back; this is
        // a guard).
        if cx.selection.text.is_none() || self.preedit.is_some() {
            return vec![];
        }
        let mv = |dir: &str| vec![Action::Exec("text.move".into(), json!({"dir": dir, "extend": mods.shift, "word": mods.alt || mods.ctrl}))];
        // A table cell's text, not a footnote's (which is addressed as a cell too).
        let in_cell = cx.selection.text.is_some_and(|t| t.cell.is_some_and(|c| c.footnote_id().is_none()));
        // Alt+Left/Right: kerning at the caret, tracking of a selection; with Cmd, five times as much.
        let steps = if mods.cmd { 5.0 } else { 1.0 };
        match key {
            ToolKey::Left if mods.alt && !mods.shift => kern_step(cx, -steps),
            ToolKey::Right if mods.alt && !mods.shift => kern_step(cx, steps),
            // Cmd with an arrow goes to the line's start or end, or the story's.
            ToolKey::Left if mods.cmd => mv("lineStart"),
            ToolKey::Right if mods.cmd => mv("lineEnd"),
            ToolKey::Up if mods.cmd => mv("storyStart"),
            ToolKey::Down if mods.cmd => mv("storyEnd"),
            ToolKey::Left => mv("left"),
            ToolKey::Right => mv("right"),
            ToolKey::Up => mv("up"),
            ToolKey::Down => mv("down"),
            ToolKey::Home => mv("lineStart"),
            ToolKey::End => mv("lineEnd"),
            ToolKey::Backspace => vec![Action::Exec("text.delete".into(), json!({"forward": false, "word": mods.alt}))],
            ToolKey::Delete => vec![Action::Exec("text.delete".into(), json!({"forward": true, "word": mods.alt}))],
            ToolKey::Enter => vec![Action::Exec("text.insert".into(), json!({"text": if mods.shift { "\u{2028}" } else { "\n" }}))],
            // In a table cell Shift+Tab goes back a cell (Tab goes on through `text.insert`).
            ToolKey::Tab if mods.shift && in_cell => vec![Action::Exec("table.prevCell".into(), json!({}))],
            ToolKey::Tab => vec![Action::Exec("text.insert".into(), json!({"text": "\t"}))],
            // In a table Esc switches between the cell's text and the cell, as InDesign's.
            ToolKey::Escape if in_cell => toggle_cell_selection(cx),
            ToolKey::Escape => vec![Action::SwitchTool("selection".into()), Action::Exec("text.exitToFrame".into(), json!({}))],
            _ => vec![],
        }
    }

    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        let (Some(p), Some(ts)) = (&self.preedit, cx.selection.text) else { return vec![] };
        // Marked text: a thin underline, a second line under the clause being converted.
        let start = marked_start(cx, ts, p);
        let line = |(a, b): (Point, Point)| Overlay::Line { a, b, color: [0, 0, 0], dashed: false };
        let mut o: Vec<Overlay> = underlines(cx, ts, start..start + p.text.len()).into_iter().map(line).collect();
        match &p.active {
            Some(r) if !r.is_empty() => {
                for (a, b) in underlines(cx, ts, start + r.start..start + r.end) {
                    // A second line a pixel further from the text.
                    let d = b - a;
                    let n = Vec2::new(-d.y, d.x) * (cx.tol(1.0) / d.hypot().max(1e-9));
                    o.push(line((a + n, b + n)));
                }
            }
            // The input method's cursor inside the marked text (the text caret stays at its end).
            Some(r) if r.start < p.text.len() => o.extend(caret_line(cx, ts, start + r.start).map(line)),
            _ => {}
        }
        o
    }

    fn cursor(&self, cx: &ToolContext, p: Point, _m: Mods) -> Cursor {
        if let Some(b) = self.border.as_ref().copied().or_else(|| if self.start.is_none() { border_at(cx, p) } else { None }) {
            return b.cursor();
        }
        Cursor::Text
    }

    fn busy(&self) -> bool {
        self.drawing || self.border.is_some()
    }

    /// Typing goes into the text at the caret or selection, not into selected table cells (the
    /// first Esc out of a cell's text): then the letter keys choose tools, so a tool key right
    /// after leaving text never types into the cell.
    fn wants_text(&self, cx: &ToolContext) -> bool {
        cx.selection.text.is_some() && cx.selection.cells.is_none()
    }

    fn ime_preedit(&mut self, cx: &ToolContext, text: &str, active_chars: Option<Range<usize>>) -> Vec<Action> {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        // A cleared composition: the preview goes, leaving no undo step. A clear with nothing
        // marked changes nothing, so it never deletes the selected text.
        if text.is_empty() || cx.selection.text.is_none() {
            return self.ime_cancel();
        }
        let mut out = Vec::new();
        if self.preedit.is_none() {
            out.push(Action::Begin("Type".into()));
        }
        // Each update replaces the previous marked text (or the selected text). Marked text shows
        // as it is: typographer's quotes and autocorrect wait for the commit.
        out.push(Action::Preview("text.insert".into(), json!({"text": text, "raw": true})));
        self.preedit = Some(Preedit { active: active_chars.and_then(|r| char_range_to_bytes(&text, r)), text });
        out
    }

    fn ime_cancel(&mut self) -> Vec<Action> {
        if self.preedit.take().is_some() { vec![Action::Cancel] } else { vec![] }
    }

    fn composing(&self) -> bool {
        self.preedit.is_some()
    }

    fn ime_caret(&self, cx: &ToolContext) -> Option<(Point, Point)> {
        let ts = cx.selection.text?;
        // The candidate window follows the clause being converted (or the input method's cursor),
        // else the caret.
        let at = match &self.preedit {
            Some(p) => marked_start(cx, ts, p) + p.active.as_ref().map_or(0, |r| r.start),
            None => ts.focus,
        };
        caret_line(cx, ts, at).or_else(|| {
            // Overset text has no caret on a page: the window goes to the out port of the story's
            // last frame.
            let fid = cx.doc.story(ts.story).and_then(|s| s.frames.last().copied()).or(ts.frame)?;
            let it = cx.doc.item(fid)?;
            let m = cx.item_canvas_xf(fid)? * it.xf;
            let r = it.inner_bounds();
            Some((m * Point::new(r.x1, r.y1 - 12.0), m * Point::new(r.x1, r.y1)))
        })
    }
}
