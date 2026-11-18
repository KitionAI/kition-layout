//! File › Export › PNG / JPEG: pages as images, for screen designs (posters, social sizes).

use kition_layout_doc::{PageSide, SpreadRef};
use kition_layout_geom::{Affine, Rect, Vec2};
use kition_layout_render::{Placed, RenderOptions, Renderer};
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc, str_param};
use crate::{Result, Session};

/// The largest image side and area exported (a poster at 300 ppi fits).
const MAX_SIDE: f64 = 16_384.0;
const MAX_PIXELS: f64 = 100_000_000.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(noundo "file.exportImage", "Export Image…", [], None,
        "{format?: png|jpeg (default png), pages?: \"1-3,5\" | [1,3] (1-based; default all), ppi?: 72 (pixels per inch: 72 = one pixel per point), background?: paper|transparent (PNG only; default paper), quality?: 1..100 (JPEG, default 90), bleed?: bool, sizes?: bool (every size: the first page of the original and of each alternate layout, instead of `pages`)} → {format, files: [{page, layout (its alternate layout's name, null in the original), base64, width, height}]}",
        has_doc, export_image)]
}

pub(super) fn export_image(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "file.exportImage";
    let jpeg = match str_param(p, "format").unwrap_or("png") {
        "png" => false,
        "jpeg" | "jpg" => true,
        f => return Err(bad(ID, format!("unknown format `{f}` (png, jpeg)"))),
    };
    let transparent = match str_param(p, "background").unwrap_or("paper") {
        "paper" => false,
        "transparent" if !jpeg => true,
        "transparent" => return Err(bad(ID, "JPEG has no transparency; use background: paper")),
        b => return Err(bad(ID, format!("unknown background `{b}` (paper, transparent)"))),
    };
    let ppi = p.get("ppi").and_then(Value::as_f64).unwrap_or(72.0);
    if !(ppi.is_finite() && (1.0..=2400.0).contains(&ppi)) {
        return Err(bad(ID, "`ppi` must be 1 to 2400"));
    }
    let quality = p.get("quality").and_then(Value::as_u64).unwrap_or(90).clamp(1, 100) as u8;
    let bleed = p.get("bleed").and_then(Value::as_bool).unwrap_or(false);
    let st = s.doc()?;
    let d = &st.doc;
    let layouts = super::liquid::layouts(d);
    let pages = if p.get("sizes").and_then(Value::as_bool).unwrap_or(false) {
        layouts.iter().map(|(_, range)| range.start).collect()
    } else {
        match super::export::options(p, d.page_count())?.pages {
            Some(pages) => pages,
            None => (0..d.page_count()).collect(),
        }
    };
    let scale = ppi / 72.0;
    let mut renderer = Renderer::new();
    let mut files = Vec::new();
    for abs in pages {
        let (si, pi) = d.page_loc(abs).ok_or_else(|| bad(ID, format!("no page {}", abs + 1)))?;
        let page = d.spreads.get(si).and_then(|sp| sp.pages.get(pi)).ok_or_else(|| bad(ID, format!("no page {}", abs + 1)))?;
        let mut r = page.bounds();
        if bleed {
            let b = d.settings.bleed;
            let (l, rr) = if page.side == PageSide::Left { (b[3], b[2]) } else { (b[2], b[3]) };
            r = Rect::new(r.x0 - l, r.y0 - b[0], r.x1 + rr, r.y1 + b[1]);
        }
        let (w, h) = ((r.width() * scale).round().max(1.0), (r.height() * scale).round().max(1.0));
        if w > MAX_SIDE || h > MAX_SIDE || w * h > MAX_PIXELS {
            return Err(bad(ID, format!("page {} at {ppi} ppi would be {w} × {h} pixels; use a lower ppi", abs + 1)));
        }
        let view = Affine::scale(scale) * Affine::translate(-r.origin().to_vec2());
        let options = RenderOptions {
            background: if transparent { None } else { Some([255, 255, 255, 255]) },
            paper: !transparent,
            printing_only: true,
            ..Default::default()
        };
        let placed = [Placed { spread: SpreadRef::Doc(si), xf: Affine::translate(Vec2::ZERO) }];
        let img = renderer.render(d, &s.cache, &placed, w as u32, h as u32, view, &options);
        let bytes = if jpeg { img.to_jpeg(quality) } else { img.to_png() };
        let layout = layouts.iter().find(|(_, range)| range.contains(&abs)).and_then(|(name, _)| name.clone());
        files.push(json!({ "page": abs + 1, "layout": layout, "base64": super::base64_encode(&bytes), "width": img.width, "height": img.height }));
    }
    Ok(json!({ "format": if jpeg { "jpeg" } else { "png" }, "files": files }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    fn decode(v: &serde_json::Value) -> Vec<u8> {
        crate::cmd::base64_decode(v.as_str().unwrap())
    }

    #[test]
    fn exports_pages_as_png_and_jpeg() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"pages": 2, "facingPages": false, "width": 200, "height": 100})).unwrap();
        let id = s.execute("frame.create", &json!({"rect": [0, 0, 100, 100], "content": "graphic"})).unwrap()["id"].as_u64().unwrap();
        s.execute("object.fill", &json!({"ids": [id], "swatch": "[Black]"})).unwrap();
        let r = s.execute("file.exportImage", &json!({"pages": [1], "ppi": 144})).unwrap();
        let files = r["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!((files[0]["width"].as_u64(), files[0]["height"].as_u64()), (Some(400), Some(200)));
        let png = kition_layout_images::decode_rgba(&decode(&files[0]["base64"])).unwrap();
        assert!(png.get_pixel(50, 50)[0] < 60, "the black frame");
        assert_eq!(png.get_pixel(350, 150).0, [255, 255, 255, 255], "paper");
        // Transparent: no paper behind the page.
        let r = s.execute("file.exportImage", &json!({"pages": "1", "background": "transparent"})).unwrap();
        let png = kition_layout_images::decode_rgba(&decode(&r["files"][0]["base64"])).unwrap();
        assert_eq!(png.get_pixel(150, 50)[3], 0, "transparent");
        assert_eq!(png.get_pixel(50, 50)[3], 255);
        // JPEG, every page.
        let r = s.execute("file.exportImage", &json!({"format": "jpeg", "quality": 80})).unwrap();
        assert_eq!(r["files"].as_array().unwrap().len(), 2);
        let jpeg = decode(&r["files"][1]["base64"]);
        assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
    }

    #[test]
    fn bad_parameters_are_errors() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"pages": 1})).unwrap();
        for p in [
            json!({"format": "gif"}),
            json!({"format": "jpeg", "background": "transparent"}),
            json!({"ppi": 0}),
            json!({"ppi": 1e9}),
            json!({"pages": [5]}),
            json!({"pages": "x"}),
            json!({"background": "blue"}),
        ] {
            assert!(s.execute("file.exportImage", &p).is_err(), "{p}");
        }
        assert!(s.execute("file.exportImage", &json!({"ppi": 2400})).is_err(), "too large");
        assert!(s.execute("file.exportImage", &json!(null)).is_ok(), "defaults");
    }
}
