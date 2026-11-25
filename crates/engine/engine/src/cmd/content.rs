//! A frame's placed graphic as its own object: moved, sized and measured inside its frame
//! (`transform.move`, `transform.set` and `transform.info` with `content: true`, what the
//! Selection tool's content grabber and the graphic's own handles do), the frame fitted to it
//! along one side (`object.fit` with `axis`), and the Eyedropper's colour under a point of it.

use kition_layout_doc::{Content, Document, Graphic, ItemId};
use kition_layout_geom::{Affine, Point, Rect, Vec2};
use serde_json::{Value, json};

use super::{bad, f64_or, rect_param};
use crate::{Result, Session};

/// Spread space to the frame's graphic space parent: the frame's placement on its spread.
fn frame_xf(d: &Document, id: ItemId) -> Option<Affine> {
    let loc = d.find(id)?;
    let it = d.item_at(&loc)?;
    Some(d.parent_xf(&loc) * it.xf)
}

/// The graphic's natural box in its own space.
fn natural(g: &Graphic) -> Rect {
    Rect::new(0.0, 0.0, g.size.0, g.size.1)
}

/// The placed graphic of frame `id` on its spread: its box (the transformed natural box's bounds)
/// and the frame's spread transform.
pub(crate) fn graphic_box(d: &Document, id: ItemId) -> Option<(Rect, Affine)> {
    let m = frame_xf(d, id)?;
    let g = d.item(id)?.graphic()?;
    let b = (m * g.xf).transform_rect_bbox(natural(g));
    (b.x0.is_finite() && b.y0.is_finite() && b.x1.is_finite() && b.y1.is_finite()).then_some((b, m))
}

/// Moves the graphic of each frame by (`dx`, `dy`) on the spread; the frames stay.
pub(crate) fn move_content(d: &mut Document, ids: &[ItemId], dx: f64, dy: f64) -> usize {
    let mut moved = 0;
    for id in ids {
        let Some(m) = frame_xf(d, *id) else { continue };
        let inv = m.inverse();
        let step = (inv * Point::new(dx, dy)) - (inv * Point::ORIGIN);
        if !(step.x.is_finite() && step.y.is_finite()) {
            continue;
        }
        let Some(it) = d.item_mut(*id) else { continue };
        if it.locked {
            continue;
        }
        let Content::Graphic(g) = &mut it.content else { continue };
        g.xf = Affine::translate(Vec2::new(step.x, step.y)) * g.xf;
        moved += 1;
    }
    moved
}

/// Maps the graphic of `id` from its current box onto `to` (both on the spread).
fn place_graphic(d: &mut Document, id: ItemId, to: Rect) -> bool {
    let Some((from, m)) = graphic_box(d, id) else { return false };
    if from.width() < 1e-9 || from.height() < 1e-9 || to.width().abs() < 1e-6 || to.height().abs() < 1e-6 {
        return false;
    }
    let on_spread = Affine::translate((to.x0, to.y0))
        * Affine::scale_non_uniform(to.width() / from.width(), to.height() / from.height())
        * Affine::translate((-from.x0, -from.y0));
    let inner = m.inverse() * on_spread * m;
    let Some(it) = d.item_mut(id) else { return false };
    if it.locked {
        return false;
    }
    let Content::Graphic(g) = &mut it.content else { return false };
    let xf = inner * g.xf;
    if !xf.as_coeffs().iter().all(|c| c.is_finite()) {
        return false;
    }
    g.xf = xf;
    true
}

/// The page under `r`'s centre: its left edge (x is measured from it, as for frames).
fn page_x(d: &Document, id: ItemId, r: Rect) -> f64 {
    d.find(id).and_then(|l| d.spread(l.spread)).and_then(|sp| sp.pages.get(sp.page_at_x(r.center().x).unwrap_or(0)).map(|pg| pg.x)).unwrap_or(0.0)
}

/// The graphic's scale (%) as the Transform panel shows it: the frame's and the graphic's together.
fn scale_of(m: Affine, g: &Graphic) -> (f64, f64) {
    let v = kition_layout_geom::decompose::decompose(m * g.xf);
    (v.scale_x * 100.0, v.scale_y * 100.0)
}

/// `transform.info` with `content: true`: the first target's graphic, measured as the panel
/// shows it (top left page-relative, size on the spread, scale).
pub(crate) fn info(s: &Session, ids: &[ItemId]) -> Result<Value> {
    let d = &s.doc()?.doc;
    let id = *ids.first().ok_or_else(|| bad("transform.info", "nothing selected"))?;
    let g = d.item(id).and_then(|it| it.graphic()).ok_or_else(|| bad("transform.info", "the frame holds no placed graphic"))?;
    let (b, m) = graphic_box(d, id).ok_or_else(|| bad("transform.info", "no such object"))?;
    let (sx, sy) = scale_of(m, g);
    let v = kition_layout_geom::decompose::decompose(m * g.xf);
    let r = |x: f64| (x * 1e4).round() / 1e4;
    Ok(json!({
        "content": true,
        "x": r(b.x0 - page_x(d, id, b)),
        "y": r(b.y0),
        "width": r(b.width()),
        "height": r(b.height()),
        "scaleX": r(sx),
        "scaleY": r(sy),
        "rotation": r(v.rotation),
        "shear": r(v.shear),
    }))
}

/// `transform.set` with `content: true`: `bounds` (spread coordinates) places each target's
/// graphic there; otherwise `x`, `y`, `width`, `height`, `scaleX`, `scaleY` about the reference
/// point `ref` (top left by default) size and place the first target's graphic as the panel shows
/// it. Rotation and shear stay the frame's.
pub(crate) fn set(s: &mut Session, p: &Value, ids: &[ItemId]) -> Result<Value> {
    const ID: &str = "transform.set";
    if p.get("rotation").is_some() || p.get("shear").is_some() {
        return Err(bad(ID, "a frame's content turns and slants with its frame: give rotation and shear without `content`"));
    }
    if let Some(to) = rect_param(p, "bounds") {
        if !(to.width().abs() > 1e-6 && to.height().abs() > 1e-6) {
            return Err(bad(ID, "`bounds` is an empty rectangle"));
        }
        return s.edit(|d, _| {
            let n = ids.iter().filter(|id| place_graphic(d, **id, to)).count();
            Ok(json!({"changed": n}))
        });
    }
    let rf = p.get("ref").and_then(Value::as_u64).unwrap_or(0).min(8) as usize;
    s.edit(|d, _| {
        let mut n = 0;
        for id in ids {
            let (Some((from, m)), Some(g)) = (graphic_box(d, *id), d.item(*id).and_then(|it| it.graphic())) else { continue };
            let (sx, sy) = scale_of(m, g);
            let mut w = f64_or(p, "width", from.width());
            let mut h = f64_or(p, "height", from.height());
            if let Some(v) = p.get("scaleX").and_then(Value::as_f64).filter(|v| v.abs() >= 0.01 && sx.abs() > 1e-9) {
                w = from.width() * v / sx;
            }
            if let Some(v) = p.get("scaleY").and_then(Value::as_f64).filter(|v| v.abs() >= 0.01 && sy.abs() > 1e-9) {
                h = from.height() * v / sy;
            }
            let (w, h) = (w.max(0.01), h.max(0.01));
            let anchor = kition_layout_geom::reference_point(from, rf);
            let fx = [0.0, 0.5, 1.0][rf % 3];
            let fy = [0.0, 0.5, 1.0][(rf / 3).min(2)];
            let mut to = Rect::new(anchor.x - fx * w, anchor.y - fy * h, anchor.x - fx * w + w, anchor.y - fy * h + h);
            if let Some(x) = p.get("x").and_then(Value::as_f64) {
                to = to + Vec2::new(x + page_x(d, *id, from) - anchor.x, 0.0);
            }
            if let Some(y) = p.get("y").and_then(Value::as_f64) {
                to = to + Vec2::new(0.0, y - anchor.y);
            }
            if place_graphic(d, *id, to) {
                n += 1;
            }
        }
        Ok(json!({"changed": n}))
    })
}

/// Fit Frame to Content along one side: the frame's path takes the graphic's extent across
/// (`width`) or down (`height`) and keeps the other.
pub(crate) fn fit_frame_axis(r: Rect, cur: Rect, axis: &str) -> Rect {
    match axis {
        "width" => Rect::new(cur.x0, r.y0, cur.x1, r.y1),
        "height" => Rect::new(r.x0, cur.y0, r.x1, cur.y1),
        _ => cur,
    }
}

/// The colour of frame `id`'s placed graphic at spread point `at`, or `None` where the graphic
/// shows no opaque pixel (outside it, or transparent there).
pub(crate) fn sample(d: &Document, id: ItemId, at: Point) -> Option<kition_layout_color::Color> {
    let it = d.item(id)?;
    let g = it.graphic()?;
    let asset = d.assets.get(&g.asset)?;
    let m = frame_xf(d, id)?;
    // The frame clips its graphic: a point outside the frame samples nothing.
    if !m.transform_rect_bbox(it.inner_bounds()).contains(at) {
        return None;
    }
    let local = (m * g.xf).inverse() * at;
    let (nw, nh) = g.size;
    if !(local.x >= 0.0 && local.y >= 0.0 && local.x < nw && local.y < nh) {
        return None;
    }
    let px = kition_layout_render::decode_pixmap_page(&asset.data, asset.page)?;
    let (w, h) = (f64::from(px.width()), f64::from(px.height()));
    if w < 1.0 || h < 1.0 {
        return None;
    }
    let x = ((local.x / nw) * w).floor().clamp(0.0, w - 1.0) as u16;
    let y = ((local.y / nh) * h).floor().clamp(0.0, h - 1.0) as u16;
    let c = px.sample(x, y);
    if c.a < 8 {
        return None;
    }
    // Premultiplied: back to the colour itself.
    let un = |v: u8| ((u32::from(v) * 255 + u32::from(c.a) / 2) / u32::from(c.a)).min(255) as u8;
    kition_layout_color::Color::from_hex(&format!("#{:02x}{:02x}{:02x}", un(c.r), un(c.g), un(c.b)))
}
