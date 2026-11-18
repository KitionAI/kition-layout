//! What Kition Layout shows over its pages, as engine overlays in canvas points: frame edges,
//! margins and columns, the selection (outlines, bounding box, handles, text ports), anchors for
//! the direct tools, and the text selection and caret. Upstream draws these in its egui canvas
//! (`crates/ui-egui/src/canvas.rs`: `draw_frames`, `draw_guides`, `draw_selection`,
//! `draw_ports`, `draw_text_selection`); this is the same logic with no UI toolkit, so a host
//! that draws engine overlays shows a layout as Kition Layout does.

use kition_kit::tools::ToolOverlay;
use kition_layout_doc::{Content, Document, Item, ItemId};
use kition_layout_engine::Session;
use kition_layout_geom::{Affine, BezPath, Point, Rect};
use kition_layout_tools::{CanvasLayout, Overlay};

const SELECTION_BLUE: [u8; 3] = [79, 153, 255];
const TEXT_HIGHLIGHT: [u8; 4] = [80, 140, 255, 90];
const OVERSET_RED: [u8; 4] = [230, 20, 20, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const SMART_GUIDE: [u8; 4] = [255, 0, 170, 255];

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

/// Everything Kition Layout draws over the layout at `zoom` (screen pixels per point).
pub fn overlays(session: &Session, zoom: f64) -> Vec<ToolOverlay> {
    let mut out = Vec::new();
    let Some(st) = session.active() else { return out };
    let doc = &st.doc;
    let layout = session.layout();
    let px = 1.0 / zoom.max(1e-6);
    guides(doc, &layout, &mut out);
    frame_edges(doc, &layout, &mut out);
    let sel = &st.selection;
    if let Some(ts) = sel.text {
        text_selection(session, doc, &layout, ts, &mut out);
        return out;
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
            for sub in &it.path.subpaths {
                for anchor in &sub.anchors {
                    let p = m * anchor.p;
                    for (h, has) in [(anchor.h_in, anchor.has_in()), (anchor.h_out, anchor.has_out())] {
                        if has {
                            let hp = m * h;
                            out.push(line(p, hp, color));
                            out.push(ToolOverlay::Handle { x: hp.x, y: hp.y, color });
                        }
                    }
                    out.push(ToolOverlay::Anchor { x: p.x, y: p.y, color, filled: false, size: 5.0 });
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

/// Margins (in the document's margin colour) and column edges, per page.
fn guides(doc: &Document, layout: &CanvasLayout, out: &mut Vec<ToolOverlay>) {
    let s = &doc.settings;
    for slot in &layout.slots {
        let Some(sp) = doc.spread(slot.spread) else { continue };
        let m = slot.xf;
        for page in &sp.pages {
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

/// The story being edited: its frames' edges, the selected text's highlight, and the caret.
fn text_selection(session: &Session, doc: &Document, layout: &CanvasLayout, ts: kition_layout_doc::TextSel, out: &mut Vec<ToolOverlay>) {
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
