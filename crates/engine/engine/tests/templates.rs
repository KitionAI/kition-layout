//! The package's starter layouts (`web/public/templates/`): each `.klayout` is the document
//! `file.applyTemplate` builds, with a preview PNG beside it for the New from Template dialog.
//! `KITION_LAYOUT_WRITE_TEMPLATES=1 cargo test -p kition-layout-engine --test templates` writes
//! them again (after a template or the file format changes); otherwise the test checks them.

use std::path::PathBuf;

use kition_layout_engine::cmd::templates::{TEMPLATES, build};
use kition_layout_engine::replica::klayout_of;
use kition_layout_engine::{DocState, Session};
use serde_json::json;

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../web/public/templates")
}

/// The template's preview: its page at a quarter of a pixel per point, in the package's fonts
/// (the kition-fonts faces the editor gives its engine; native builds only register the Japanese
/// ones by themselves).
fn preview(doc: kition_layout_doc::Document) -> Vec<u8> {
    let families = ["Inter", "Lora"];
    let fonts: Vec<_> = kition_layout_fonts::KITION_FONTS.iter().filter(|f| families.contains(&f.family)).collect();
    assert_eq!(fonts.len(), families.len(), "previews need kition-fonts (KITION_FONTS_DIR) with Inter and Lora");
    for font in fonts {
        kition_layout_fonts::FontDb::global().add_font(font.bytes.to_vec());
    }
    let mut s = Session::new();
    s.add_document(DocState::new(doc, None));
    let out = s.execute("file.exportImage", &json!({"format": "png", "ppi": 18})).unwrap();
    let text = out["files"][0]["base64"].as_str().unwrap();
    decode(text)
}

fn decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes().filter(|c| *c != b'=') {
        let v = ALPHABET.iter().position(|a| *a == c).unwrap() as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// What a template is, independent of the ids a file's replica mints when it is read: page size,
/// swatches, paragraph styles, and each spread's and parent's objects with their text.
fn comparable(doc: &kition_layout_doc::Document) -> serde_json::Value {
    let round = |v: f64| (v * 100.0).round() / 100.0;
    let item = |it: &kition_layout_doc::Item| {
        let b = it.bounds();
        let text = it.text_frame().and_then(|t| doc.story(t.story)).map(|s| s.text.clone());
        json!([
            it.name,
            it.label,
            [round(b.x0), round(b.y0), round(b.x1), round(b.y1)],
            it.fill.swatch,
            it.stroke.swatch,
            it.stroke.weight,
            it.locked,
            text,
            it.liquid.is_some()
        ])
    };
    json!({
        "size": [doc.settings.page_width, doc.settings.page_height],
        "swatches": doc.swatches.iter().map(|w| serde_json::to_value(w).unwrap()).collect::<Vec<_>>(),
        "styles": doc.styles.paragraph.iter().map(|p| json!([p.name, serde_json::to_value(&p.chars).unwrap(), serde_json::to_value(&p.para).unwrap()])).collect::<Vec<_>>(),
        "spreads": doc.spreads.iter().map(|sp| sp.items.iter().map(|it| item(it)).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "parents": doc.parents.iter().map(|sp| sp.items.iter().map(|it| item(it)).collect::<Vec<_>>()).collect::<Vec<_>>(),
    })
}

#[test]
fn the_packaged_templates_are_the_built_ones() {
    let dir = folder();
    let write = std::env::var_os("KITION_LAYOUT_WRITE_TEMPLATES").is_some();
    for (id, _) in TEMPLATES {
        let doc = build(id, "").unwrap();
        let file = dir.join(format!("{id}.klayout"));
        let png = dir.join(format!("{id}.png"));
        if write {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&file, klayout_of(&doc).unwrap()).unwrap();
            std::fs::write(&png, preview(doc.clone())).unwrap();
        }
        let bytes = std::fs::read(&file).unwrap_or_else(|_| panic!("{} is missing: run with KITION_LAYOUT_WRITE_TEMPLATES=1", file.display()));
        let packaged = DocState::open_klayout(&bytes, None).unwrap().doc.as_ref().clone();
        assert_eq!(comparable(&packaged), comparable(&doc), "{id}.klayout is out of date: run with KITION_LAYOUT_WRITE_TEMPLATES=1");
        assert!(std::fs::read(&png).is_ok_and(|b| b.starts_with(&[137, 80, 78, 71])), "{id}.png is missing");
    }
}
