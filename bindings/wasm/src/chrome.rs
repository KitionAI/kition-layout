//! What Kition Layout shows over its pages, as engine overlays in canvas points: ruler guides,
//! margins and columns, bleed and slug, the document and baseline grids, frame edges, overset
//! marks, hidden characters, the selection (outlines, bounding box, handles, text ports), anchors
//! for the direct tools, and the text selection and caret (in a table cell too). The editor's
//! view options ([`ViewOptions`]) say which aids show. Upstream draws these in its egui canvas
//! (`crates/ui-egui/src/canvas.rs`: `draw_frames`, `draw_guides`, `draw_selection`,
//! `draw_ports`, `draw_text_selection`); this is the same logic with no UI toolkit, so a host
//! that draws engine overlays shows a layout as Kition Layout does.

use kition_kit::tools::ToolOverlay;
use kition_layout_doc::{Content, Document, GridRelative, Item, ItemId, Orientation, PageSide};
use kition_layout_engine::Session;
use kition_layout_geom::{Affine, BezPath, Point, Rect};
use kition_layout_tools::{CanvasLayout, Overlay};

use crate::view::{GuideRef, ViewOptions};

const SELECTION_BLUE: [u8; 3] = [79, 153, 255];
const TEXT_HIGHLIGHT: [u8; 4] = [80, 140, 255, 90];
const OVERSET_RED: [u8; 4] = [230, 20, 20, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const SMART_GUIDE: [u8; 4] = [255, 0, 170, 255];
/// Ruler guides (InDesign's default cyan) and a selected one.
const RULER_GUIDE: [u8; 4] = [74, 200, 245, 255];
/// The most hidden-character marks drawn at once (a long story's spaces).
const MAX_MARKS: usize = 20_000;
/// The most grid lines drawn per page and direction.
const MAX_GRID_LINES: usize = 2_000;

fn rgba([r, g, b]: [u8; 3], alpha: u8) -> [u8; 4] {
    [r, g, b, alpha]
}

fn path(path: BezPath, color: [u8; 4], width: f32) -> ToolOverlay {
    ToolOverlay::Path { d: path.to_svg(), color, width, dashed: false }
}

fn line(a: Point, b: Point, color: [u8; 4]) -> ToolOverlay {
    ToolOverlay::Line { ax: a.x, ay: a.y, bx: b.x, by: b.y, color, dashed: false }
}

fn rect_path(r: Rect) -> BezPath {
    let mut bp = BezPath::new();
    bp.move_to((r.x0, r.y0));
    bp.line_to((r.x1, r.y0));
    bp.line_to((r.x1, r.y1));
    bp.line_to((r.x0, r.y1));
    bp.close_path();
    bp
}

fn layer_color(doc: &Document, it: &Item) -> [u8; 3] {
    doc.layer(it.layer).map_or(SELECTION_BLUE, |layer| layer.color)
}

/// Spread space to canvas for an item.
fn item_xf(doc: &Document, layout: &CanvasLayout, id: ItemId) -> Option<Affine> {
    let loc = doc.find(id)?;
    Some(layout.xf(loc.spread) * doc.parent_xf(&loc))
}

/// An item's outline in canvas points.
fn outline(it: &Item, a: Affine) -> BezPath {
    (a * it.xf) * it.path.to_bezpath()
}

/// A tool's own overlay (a drag's preview, a marquee, smart guides) in Kition's terms.
pub fn from_tool(overlay: Overlay) -> Option<ToolOverlay> {
    Some(match overlay {
        Overlay::Marquee(r) => ToolOverlay::Marquee { x0: r.x0, y0: r.y0, x1: r.x1, y1: r.y1 },
        Overlay::Path { path: p, color, dashed } => ToolOverlay::Path { d: p.to_svg(), color: rgba(color, 255), width: 1.0, dashed },
        Overlay::Line { a, b, color, dashed } => ToolOverlay::Line { ax: a.x, ay: a.y, bx: b.x, by: b.y, color: rgba(color, 255), dashed },
        Overlay::Measure { p, text } => ToolOverlay::Measure { x: p.x, y: p.y, text },
        Overlay::Guide { a, b } => line(a, b, SMART_GUIDE),
        // A spacing segment shows as its label at its middle; the segment itself is a guide.
        Overlay::Gap { a, b, label } => {
            let mid = a.midpoint(b);
            ToolOverlay::Label { x: mid.x, y: mid.y, text: label, color: SMART_GUIDE }
        }
    })
}

/// Everything Kition Layout draws over the layout at `zoom` (screen pixels per point), as `view`
/// asks.
pub fn overlays(session: &Session, zoom: f64, view: &ViewOptions) -> Vec<ToolOverlay> {
    let mut out = Vec::new();
    let Some(st) = session.active() else { return out };
    let doc = &st.doc;
    let layout = session.layout();
    let px = 1.0 / zoom.max(1e-6);
    if view.aids() {
        page_aids(doc, &layout, zoom, view, &mut out);
        if view.frame_edges {
            frame_edges(doc, &layout, &mut out);
        }
        overset_marks(session, doc, &layout, px, &mut out);
        if view.hidden_characters {
            hidden_characters(session, doc, &layout, &mut out);
        }
    }
    let sel = &st.selection;
    if let Some(ts) = sel.text {
        text_selection(session, doc, &layout, ts, &mut out);
        return out;
    }
    if let Some(cells) = &sel.cells {
        cell_selection(session, doc, &layout, cells, &mut out);
    }
    let mut union: Option<Rect> = None;
    let mut color = SELECTION_BLUE;
    for id in &sel.items {
        let (Some(it), Some(a)) = (doc.item(*id), item_xf(doc, &layout, *id)) else { continue };
        color = layer_color(doc, it);
        out.push(path(outline(it, a), rgba(color, 255), 1.0));
        let b = a.transform_rect_bbox(it.bounds());
        if sel.items.len() == 1 {
            out.push(path(rect_path(b), rgba(color, 255), 1.0));
        }
        union = Some(union.map_or(b, |u| u.union(b)));
        if let Content::Text(tf) = &it.content {
            ports(session, doc, it, tf.story, a, color, px, &mut out);
        }
    }
    let direct = matches!(session.tool_id(), "directSelection" | "pen" | "addAnchor" | "deleteAnchor" | "convertDirection");
    if direct {
        for id in &sel.items {
            let (Some(it), Some(a)) = (doc.item(*id), item_xf(doc, &layout, *id)) else { continue };
            let m = a * it.xf;
            let color = rgba(layer_color(doc, it), 255);
            for (si, sub) in it.path.subpaths.iter().enumerate() {
                let count = sub.anchors.len();
                let chosen = |ai: usize| sel.anchors.contains(&(*id, si, ai));
                for (ai, anchor) in sub.anchors.iter().enumerate() {
                    let p = m * anchor.p;
                    // Direction lines show on a selected anchor and its neighbours (the segments it
                    // shapes), as InDesign's Direct Selection does; the other anchors are hollow.
                    let before = if ai == 0 { sub.closed.then(|| count.saturating_sub(1)) } else { Some(ai - 1) };
                    let after = if ai + 1 == count { sub.closed.then_some(0) } else { Some(ai + 1) };
                    let near = chosen(ai) || before.is_some_and(chosen) || after.is_some_and(chosen);
                    if near {
                        for (h, has) in [(anchor.h_in, anchor.has_in()), (anchor.h_out, anchor.has_out())] {
                            if has {
                                let hp = m * h;
                                out.push(line(p, hp, color));
                                out.push(ToolOverlay::Handle { x: hp.x, y: hp.y, color });
                            }
                        }
                    }
                    out.push(ToolOverlay::Anchor { x: p.x, y: p.y, color, filled: chosen(ai), size: 5.0 });
                }
            }
        }
    } else if let Some(u) = union {
        let color = rgba(color, 255);
        if sel.items.len() > 1 {
            out.push(path(rect_path(u), color, 1.0));
        }
        out.push(ToolOverlay::Anchor { x: u.center().x, y: u.center().y, color, filled: true, size: 3.5 });
        for h in kition_layout_tools::select::handles(u) {
            out.push(ToolOverlay::Anchor { x: h.x, y: h.y, color, filled: false, size: 6.5 });
        }
    }
    out
}

/// The non-printing aids of each spread, back to front: the document grid, bleed and slug, the
/// baseline grid, margins and columns, then the ruler guides.
fn page_aids(doc: &Document, layout: &CanvasLayout, zoom: f64, view: &ViewOptions, out: &mut Vec<ToolOverlay>) {
    let s = &doc.settings;
    for (spread_index, slot) in layout.slots.iter().enumerate() {
        let Some(sp) = doc.spread(slot.spread) else { continue };
        let m = slot.xf;
        if view.document_grid {
            for page in &sp.pages {
                document_grid(doc, page.bounds(), m, zoom, out);
            }
        }
        if view.bleed {
            for (offsets, color) in [(s.bleed, s.bleed_color), (s.slug, s.slug_color)] {
                if let Some(r) = bleed_box(sp, offsets) {
                    out.push(path(m * rect_path(r), rgba(color, 255), 1.0));
                }
            }
        }
        for page in &sp.pages {
            if view.baseline_grid && zoom >= s.baseline_grid.view_threshold {
                let g = &s.baseline_grid;
                let b = page.bounds();
                let start = match g.relative_to {
                    GridRelative::TopOfPage => g.start,
                    GridRelative::TopMargin => g.start + page.margins.top,
                };
                for y in baseline_lines(start, g.increment, b.height()) {
                    out.push(line(m * Point::new(b.x0, b.y0 + y), m * Point::new(b.x1, b.y0 + y), rgba(g.color, 255)));
                }
            }
            if view.margins {
                out.push(path(m * rect_path(page.margin_rect()), rgba(s.margin_color, 255), 1.0));
                let cols = page.column_rects();
                if cols.len() > 1 {
                    for (i, c) in cols.iter().enumerate() {
                        let color = rgba(s.column_color, 255);
                        if i > 0 {
                            out.push(line(m * Point::new(c.x0, c.y0), m * Point::new(c.x0, c.y1), color));
                        }
                        if i + 1 < cols.len() {
                            out.push(line(m * Point::new(c.x1, c.y0), m * Point::new(c.x1, c.y1), color));
                        }
                    }
                }
            }
        }
        if view.guides {
            ruler_guides(doc, layout, slot, spread_index, view, out);
        }
    }
}

/// A spread's bleed or slug box: its pages' bounds pushed out by `[top, bottom, inside, outside]`
/// (inside is the spine side of a facing page; a single page's left). None when all are zero.
pub fn bleed_box(sp: &kition_layout_doc::Spread, [top, bottom, inside, outside]: [f64; 4]) -> Option<Rect> {
    if [top, bottom, inside, outside].iter().all(|v| !v.is_finite() || *v <= 0.0) {
        return None;
    }
    let b = sp.bounds();
    let left = match sp.pages.first().map(|p| p.side) {
        Some(PageSide::Left) => outside,
        _ => inside,
    };
    let right = match sp.pages.last().map(|p| p.side) {
        Some(PageSide::Left) => inside,
        _ => outside,
    };
    let r = Rect::new(b.x0 - left, b.y0 - top, b.x1 + right, b.y1 + bottom);
    (r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite()).then_some(r)
}

/// Baseline grid lines down a page of `height`, from its top: `start + n × increment` for n ≥ 0
/// inside the page (lines above it are skipped without walking them).
fn baseline_lines(start: f64, increment: f64, height: f64) -> impl Iterator<Item = f64> {
    let step = if increment.is_finite() { increment.max(1.0) } else { 12.0 };
    let first = if start.is_finite() && start >= 0.0 { start } else { start.rem_euclid(step) };
    (0..MAX_GRID_LINES * 8).map(move |n| first + n as f64 * step).take_while(move |y| y.is_finite() && *y < height)
}

/// The document grid over a page: major lines in the grid's colour, subdivisions fainter (left
/// out while closer than 4 screen pixels).
fn document_grid(doc: &Document, page: Rect, m: Affine, zoom: f64, out: &mut Vec<ToolOverlay>) {
    let g = &doc.settings.grid;
    for (vertical, major) in [(true, g.vertical), (false, g.horizontal)] {
        if !major.is_finite() || major <= 0.0 {
            continue;
        }
        let sub = f64::from(g.subdivisions.max(1));
        let shown_sub = major / sub * zoom >= 4.0;
        let step = if shown_sub { major / sub } else { major };
        let (lo, hi) = if vertical { (page.x0, page.x1) } else { (page.y0, page.y1) };
        for n in 0..MAX_GRID_LINES {
            let at = lo + n as f64 * step;
            if !at.is_finite() || at > hi {
                break;
            }
            let is_major = !shown_sub || n % (g.subdivisions.max(1) as usize) == 0;
            let color = rgba(g.color, if is_major { 255 } else { 110 });
            let (a, b) =
                if vertical { (Point::new(at, page.y0), Point::new(at, page.y1)) } else { (Point::new(page.x0, at), Point::new(page.x1, at)) };
            out.push(line(m * a, m * b, color));
        }
    }
}

/// A spread's ruler guides on its visible layers: a page guide across its page, a spread guide
/// across the pasteboard; the selected guide thicker, a liquid one dashed, the one being dragged
/// left out (the editor draws it where it goes).
fn ruler_guides(
    doc: &Document,
    layout: &CanvasLayout,
    slot: &kition_layout_tools::layout::SpreadSlot,
    spread_index: usize,
    view: &ViewOptions,
    out: &mut Vec<ToolOverlay>,
) {
    let Some(sp) = doc.spread(slot.spread) else { return };
    let m = slot.xf;
    let pasteboard = m.inverse().transform_rect_bbox(layout.pasteboard);
    for (pi, page) in sp.pages.iter().enumerate() {
        for (gi, g) in page.guides.iter().enumerate() {
            let here = GuideRef { spread: spread_index, page: pi, index: gi };
            if !g.visible_in(doc) || view.moving_guide == Some(here) {
                continue;
            }
            let span = if g.spread { pasteboard } else { page.bounds() };
            let (a, b) = match g.orientation {
                Orientation::Horizontal => (Point::new(span.x0, g.position), Point::new(span.x1, g.position)),
                Orientation::Vertical => (Point::new(g.position, span.y0), Point::new(g.position, span.y1)),
            };
            let color = match g.layer.and_then(|l| doc.layer(l)) {
                Some(layer) if view.selected_guide == Some(here) => rgba(layer.color, 255),
                _ => RULER_GUIDE,
            };
            let mut bp = BezPath::new();
            bp.move_to(m * a);
            bp.line_to(m * b);
            out.push(ToolOverlay::Path { d: bp.to_svg(), color, width: if view.selected_guide == Some(here) { 2.0 } else { 1.0 }, dashed: g.liquid });
        }
    }
}

/// Frame edges: frames without a stroke show their edge; empty graphic frames their X.
fn frame_edges(doc: &Document, layout: &CanvasLayout, out: &mut Vec<ToolOverlay>) {
    fn edges(doc: &Document, it: &Item, a: Affine, out: &mut Vec<ToolOverlay>) {
        if it.hidden || doc.layer(it.layer).is_some_and(|l| !l.visible) {
            return;
        }
        let color = rgba(layer_color(doc, it), 190);
        if let Content::Group { items } = &it.content {
            for child in items {
                edges(doc, child, a * it.xf, out);
            }
            return;
        }
        if it.stroke.is_none() || matches!(it.content, Content::Text(_) | Content::Graphic(_)) {
            out.push(path(outline(it, a), color, 1.0));
        }
        if it.object_style == kition_layout_doc::BASIC_GRAPHICS_FRAME && matches!(it.content, Content::Unassigned) {
            let r = it.inner_bounds();
            let m = a * it.xf;
            out.push(line(m * Point::new(r.x0, r.y0), m * Point::new(r.x1, r.y1), color));
            out.push(line(m * Point::new(r.x1, r.y0), m * Point::new(r.x0, r.y1), color));
        }
    }
    for slot in &layout.slots {
        let Some(sp) = doc.spread(slot.spread) else { continue };
        for it in &sp.items {
            edges(doc, it, slot.xf, out);
        }
    }
}

/// A text frame's in and out ports; the out port of an overset story's last frame is a red plus.
#[allow(clippy::too_many_arguments)]
fn ports(
    session: &Session,
    doc: &Document,
    it: &Item,
    story: kition_layout_doc::StoryId,
    a: Affine,
    color: [u8; 3],
    px: f64,
    out: &mut Vec<ToolOverlay>,
) {
    let Some(st) = doc.story(story) else { return };
    let pos = st.frames.iter().position(|f| *f == it.id).unwrap_or(0);
    let last = pos + 1 == st.frames.len();
    let overset = last && session.cache.get(doc, story, None).is_overset();
    let [inp, outp] = kition_layout_tools::thread::port_points(it, a, 1.0 / px);
    for (c, is_out) in [(inp, false), (outp, true)] {
        out.push(ToolOverlay::Anchor { x: c.x, y: c.y, color: rgba(color, 255), filled: false, size: 8.5 });
        if is_out && overset {
            let d = 2.5 * px;
            out.push(line(c - (d, 0.0), c + (d, 0.0), OVERSET_RED));
            out.push(line(c - (0.0, d), c + (0.0, d), OVERSET_RED));
        }
    }
}

/// The red plus on the out port of every overset story's last frame, selected or not, so the
/// text that does not fit shows anywhere in the layout. A selected frame draws its ports with the
/// selection.
fn overset_marks(session: &Session, doc: &Document, layout: &CanvasLayout, px: f64, out: &mut Vec<ToolOverlay>) {
    let selected = session.active().map(|st| st.selection.items.clone()).unwrap_or_default();
    for story in doc.stories.values() {
        let Some(last) = story.frames.last().copied() else { continue };
        if selected.contains(&last) || !session.cache.get(doc, story.id, None).is_overset() {
            continue;
        }
        let (Some(it), Some(a)) = (doc.item(last), item_xf(doc, layout, last)) else { continue };
        if it.hidden || doc.layer(it.layer).is_some_and(|l| !l.visible) {
            continue;
        }
        let [_, port] = kition_layout_tools::thread::port_points(it, a, 1.0 / px);
        let d = 2.5 * px;
        out.push(ToolOverlay::Anchor { x: port.x, y: port.y, color: OVERSET_RED, filled: false, size: 8.5 });
        out.push(line(port - (d, 0.0), port + (d, 0.0), OVERSET_RED));
        out.push(line(port - (0.0, d), port + (0.0, d), OVERSET_RED));
    }
}

/// Type › Show Hidden Characters: ¶ at a paragraph's end, # at the story's, » for a tab, · for a
/// space, ¬ for a forced line break, ° for a nonbreaking space; each in its frame's layer colour,
/// at the baseline (a table cell's marks in the cell).
fn hidden_characters(session: &Session, doc: &Document, layout: &CanvasLayout, out: &mut Vec<ToolOverlay>) {
    let mut count = 0;
    for story in doc.stories.values() {
        let cs = session.cache.get(doc, story.id, None);
        for ft in &cs.frames {
            let (Some(a), Some(it)) = (item_xf(doc, layout, ft.frame), doc.item(ft.frame)) else { continue };
            if it.hidden || doc.layer(it.layer).is_some_and(|l| !l.visible) {
                continue;
            }
            let m = a * doc.text_xf(it);
            let color = rgba(layer_color(doc, it), 255);
            for (p, mark) in frame_marks(&story.text, ft) {
                if count >= MAX_MARKS {
                    return;
                }
                count += 1;
                let at = m * p;
                out.push(ToolOverlay::Label { x: at.x, y: at.y, text: mark.to_owned(), color });
            }
        }
    }
}

/// A frame's hidden-character marks in frame space: its lines', then its tables' cells'.
fn frame_marks(text: &str, ft: &kition_layout_compose::FrameText) -> Vec<(Point, &'static str)> {
    let table_lines: Vec<usize> = ft.tables.iter().map(|t| t.line).collect();
    let mut marks: Vec<(Point, &'static str)> = ft
        .lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !table_lines.contains(i))
        .flat_map(|(_, l)| line_marks(text, l).into_iter().map(move |(x, mark)| (Point::new(x, l.baseline), mark)))
        .collect();
    for t in &ft.tables {
        for c in t.cells.iter().filter(|c| c.graphic.is_none() && !c.repeated) {
            if let Some(cf) = c.text.frames.first() {
                marks.extend(frame_marks(&c.source, cf).into_iter().map(|(p, mark)| (p + c.origin.to_vec2(), mark)));
            }
        }
        if t.last {
            marks.push((Point::new(t.rect.x1 + 1.0, t.rect.y1), "¶"));
        }
    }
    marks
}

/// The marks on one line: frame-space x and the mark.
fn line_marks(text: &str, l: &kition_layout_compose::Line) -> Vec<(f64, &'static str)> {
    let mut marks = Vec::new();
    for g in l.glyphs.iter().filter(|g| g.len > 0) {
        let Some(ch) = text.get(g.byte..).and_then(|rest| rest.chars().next()) else { continue };
        match ch {
            ' ' => marks.push((g.x + g.adv / 2.0, "·")),
            '\t' => marks.push((g.x, "»")),
            '\u{2028}' => marks.push((g.x, "¬")),
            '\u{A0}' => marks.push((g.x + g.adv / 2.0, "°")),
            _ => {}
        }
    }
    if l.last_in_para {
        // Spaces trimmed off the line's end sit past `end_x`: the paragraph's mark follows them.
        let end = l.glyphs.iter().filter(|g| g.len > 0 && !g.visible && g.x >= l.end_x - 1e-9).map(|g| g.x + g.adv).fold(l.end_x, f64::max);
        marks.push((end + 1.0, if l.range.end >= text.len() { "#" } else { "¶" }));
    }
    marks
}

/// Selected table cells: each one's area highlighted.
fn cell_selection(session: &Session, doc: &Document, layout: &CanvasLayout, cells: &kition_layout_doc::TableSel, out: &mut Vec<ToolOverlay>) {
    let cs = session.cache.get(doc, cells.story, None);
    let r = &cells.range;
    let (r0, r1, c0, c1) = (r.r0.min(r.r1), r.r0.max(r.r1), r.c0.min(r.c1), r.c0.max(r.c1));
    for ft in &cs.frames {
        let (Some(a), Some(it)) = (item_xf(doc, layout, ft.frame), doc.item(ft.frame)) else { continue };
        let m = a * doc.text_xf(it);
        for t in ft.tables.iter().filter(|t| t.table == cells.table) {
            for c in t.cells.iter().filter(|c| (r0..=r1).contains(&c.row) && (c0..=c1).contains(&c.col)) {
                let q = [
                    Point::new(c.rect.x0, c.rect.y0),
                    Point::new(c.rect.x1, c.rect.y0),
                    Point::new(c.rect.x1, c.rect.y1),
                    Point::new(c.rect.x0, c.rect.y1),
                ];
                out.push(ToolOverlay::Highlight {
                    quad: q.map(|p| {
                        let c = m * p;
                        [c.x, c.y]
                    }),
                    color: TEXT_HIGHLIGHT,
                });
            }
        }
    }
}

/// Text being typed in a table cell: the cell's highlighted text and its caret.
fn cell_text_selection(
    session: &Session,
    doc: &Document,
    layout: &CanvasLayout,
    ts: kition_layout_doc::TextSel,
    cell: kition_layout_doc::CellAddr,
    out: &mut Vec<ToolOverlay>,
) {
    let cs = session.cache.get(doc, ts.story, None);
    let Some((fi, _, placed)) = kition_layout_compose::find_cell(&cs, cell.table, cell.row, cell.col) else { return };
    let Some(ft) = cs.frames.get(fi) else { return };
    let (Some(a), Some(it)) = (item_xf(doc, layout, ft.frame), doc.item(ft.frame)) else { return };
    let frame = a * doc.text_xf(it);
    out.push(path(outline(it, a), rgba(layer_color(doc, it), 150), 1.0));
    let m = frame * Affine::translate(placed.origin.to_vec2());
    let range = ts.range();
    if !range.is_empty()
        && let Some(cft) = placed.text.frames.first()
    {
        for l in &cft.lines {
            let s = range.start.max(l.range.start);
            let e = range.end.min(l.range.end);
            if s > e || (s == e && !(l.range.end < range.end && e == l.range.end)) {
                continue;
            }
            let past = e == l.range.end && range.end > l.range.end;
            for q in kition_layout_compose::highlight_quads(l, s, e, past) {
                out.push(ToolOverlay::Highlight {
                    quad: q.map(|p| {
                        let c = m * p;
                        [c.x, c.y]
                    }),
                    color: TEXT_HIGHLIGHT,
                });
            }
        }
    }
    if ts.is_caret()
        && let Some((_, x, baseline, ascent, descent)) = kition_layout_compose::cell_caret(&cs, cell.table, cell.row, cell.col, ts.focus)
    {
        out.push(line(frame * Point::new(x, baseline - ascent), frame * Point::new(x, baseline + descent), BLACK));
    }
}

/// The story being edited: its frames' edges, the selected text's highlight, and the caret.
fn text_selection(session: &Session, doc: &Document, layout: &CanvasLayout, ts: kition_layout_doc::TextSel, out: &mut Vec<ToolOverlay>) {
    if let Some(cell) = ts.cell.filter(|cell| cell.footnote_id().is_none()) {
        cell_text_selection(session, doc, layout, ts, cell, out);
        return;
    }
    let cs = session.cache.get(doc, ts.story, None);
    let range = ts.range();
    for ft in &cs.frames {
        let (Some(a), Some(it)) = (item_xf(doc, layout, ft.frame), doc.item(ft.frame)) else { continue };
        out.push(path(outline(it, a), rgba(layer_color(doc, it), 150), 1.0));
        if range.is_empty() {
            continue;
        }
        let m = a * doc.text_xf(it);
        for l in &ft.lines {
            let s = range.start.max(l.range.start);
            let e = range.end.min(l.range.end);
            if s > e || (s == e && !(l.range.end < range.end && e == l.range.end)) {
                continue;
            }
            let past = e == l.range.end && range.end > l.range.end;
            for q in kition_layout_compose::highlight_quads(l, s, e, past) {
                let quad = q.map(|p| {
                    let c = m * p;
                    [c.x, c.y]
                });
                out.push(ToolOverlay::Highlight { quad, color: TEXT_HIGHLIGHT });
            }
        }
    }
    if ts.is_caret()
        && let Some((fi, x, baseline, ascent, descent)) = kition_layout_compose::caret(&cs, ts.focus)
        && let Some(ft) = cs.frames.get(fi)
        && let (Some(a), Some(it)) = (item_xf(doc, layout, ft.frame), doc.item(ft.frame))
    {
        let m = a * doc.text_xf(it);
        out.push(line(m * Point::new(x, baseline - ascent), m * Point::new(x, baseline + descent), BLACK));
    }
}
