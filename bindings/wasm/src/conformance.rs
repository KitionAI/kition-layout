//! The KHP conformance fixtures (`conformance/`) and the check catalogue (`agent/checks.json`,
//! from [`crate::check::CATALOGUE`]): `basic.klayout` (no diagnostics), `edit.json` (steps that
//! change it), one `check-<code>.klayout` per catalogue code, `overflow.klayout` (a layout kit's
//! overflowing text, KHP A40), and the `expected/` previews of `basic`. Each fixture starts from a new
//! letter-size print document made with the engine's own commands; what a document can hold but the
//! commands never make (a graphic whose file is gone, a font that is not installed here, a
//! typography setting the composer does not apply) is made by editing the document, as a document
//! made elsewhere would hold it.
//!
//! `KITION_WRITE_CONFORMANCE=1 cargo test -p kition-layout-kit conformance` writes them; without the
//! variable the test only checks that every fixture it would write reports its code.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kition_kit::KitDocument;
use kition_layout_doc::Document;
use serde_json::{Value, json};

use crate::LayoutDocument;
use crate::check::{CATALOGUE, catalogue_json};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(document: &mut LayoutDocument, command: &str, params: Value) -> Value {
    document.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn doc_mut(document: &mut LayoutDocument) -> &mut Document {
    Arc::make_mut(&mut document.session.active_mut().expect("open").doc)
}

/// Writes a direct edit of the document into its replica (what a file holds): the engine writes the
/// document's changes after each top-level command.
fn commit(document: &mut LayoutDocument) {
    run(document, "document.inspect", json!({}));
}

/// A small PNG of `side` × `side` pixels, as base64.
fn picture(side: u32) -> String {
    let picture = image::RgbaImage::from_fn(side, side, |x, y| image::Rgba([(x * 255 / side.max(1)) as u8, (y * 255 / side.max(1)) as u8, 160, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, image::ImageFormat::Png).expect("png");
    kition_layout_engine::cmd::base64_encode(&out.into_inner())
}

fn frame(document: &mut LayoutDocument, rect: [f64; 4], text: &str) -> u64 {
    let made = run(document, "frame.create", json!({ "rect": rect, "content": "text", "text": text }));
    made["id"].as_u64().or_else(|| made["ids"][0].as_u64()).unwrap_or_else(|| panic!("frame id in {made}"))
}

/// A letter page with a headline frame and a coloured band.
fn basic() -> LayoutDocument {
    let mut document = LayoutDocument::create("Flyer.klayout", 612, 792).expect("document");
    frame(&mut document, [72.0, 72.0, 540.0, 160.0], "Spring concert");
    run(&mut document, "frame.create", json!({ "rect": [72.0, 200.0, 540.0, 260.0], "content": "unassigned" }));
    document
}

fn fixture(code: &str) -> LayoutDocument {
    let mut document = basic();
    match code {
        "layout.overset" => {
            frame(&mut document, [72.0, 300.0, 200.0, 320.0], &"A long programme note that does not fit its small frame. ".repeat(20));
        }
        "layout.missing-font" => {
            let doc = doc_mut(&mut document);
            let story = doc.stories.values_mut().next().expect("story");
            for run in &mut Arc::make_mut(story).chars {
                run.format.over = serde_json::from_value(json!({ "fontFamily": "Kition Missing Sans" })).expect("char attrs");
            }
            commit(&mut document);
        }
        "layout.missing-glyphs" => {
            // A document whose glyph fallback is off (an IDML import, a file saved that way): the
            // characters no font of the text has are drawn as missing glyphs.
            doc_mut(&mut document).settings.glyph_fallback = false;
            commit(&mut document);
            frame(&mut document, [72.0, 300.0, 540.0, 360.0], "Private use \u{F0B00}\u{F0B01}");
        }
        "layout.missing-link" => {
            run(&mut document, "file.place", json!({ "base64": picture(400), "name": "photo.png", "x": 72, "y": 300, "width": 200 }));
            let doc = doc_mut(&mut document);
            for asset in doc.assets.values_mut() {
                let asset = Arc::make_mut(asset);
                asset.data = Arc::new(Vec::new());
                asset.link = Some("images/photo.png".into());
            }
            commit(&mut document);
        }
        "layout.unsupported-typography" => {
            let doc = doc_mut(&mut document);
            let story = doc.stories.values_mut().next().expect("story");
            for para in &mut Arc::make_mut(story).paras {
                para.para = serde_json::from_value(json!({ "kinsokuType": "PushInFirst" })).expect("para attrs");
            }
            commit(&mut document);
        }
        "layout.low-resolution" => {
            run(&mut document, "file.place", json!({ "base64": picture(8), "name": "thumbnail.png", "x": 72, "y": 300, "width": 400 }));
        }
        "layout.empty-frame" => {
            frame(&mut document, [72.0, 300.0, 540.0, 360.0], "");
        }
        "layout.rgb-in-print" => {
            run(&mut document, "swatch.create", json!({ "name": "Screen Blue", "color": "#0066ff" }));
        }
        other => panic!("no fixture for {other}"),
    }
    document
}

fn edit() -> Value {
    json!({ "steps": [
        { "command": "frame.create", "params": { "rect": [72, 300, 540, 360], "content": "text", "text": "Saturday 8 pm" } },
    ] })
}

fn png(document: &LayoutDocument, scale: f64) -> Vec<u8> {
    let (width, height) = ((f64::from(document.width()) * scale) as u32, (f64::from(document.height()) * scale) as u32);
    let rgba = document.render(0.0, 0.0, scale, width, height).expect("render");
    let picture = image::RgbaImage::from_raw(width, height, rgba).expect("pixels");
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, image::ImageFormat::Png).expect("png");
    out.into_inner()
}

fn codes(document: &LayoutDocument) -> Vec<String> {
    document.check().into_iter().map(|diagnostic| diagnostic.code).collect()
}

#[test]
fn every_fixture_reports_its_code_and_basic_is_clean() {
    let write = std::env::var_os("KITION_WRITE_CONFORMANCE").is_some();
    let out = repo().join("conformance");
    let basic = basic();
    assert_eq!(codes(&basic), Vec::<String>::new(), "basic.klayout is clean");
    if write {
        std::fs::create_dir_all(out.join("expected")).expect("conformance/expected");
        std::fs::create_dir_all(repo().join("agent")).expect("agent");
        std::fs::write(out.join("basic.klayout"), basic.save(None).expect("save")).expect("basic");
        std::fs::write(out.join("edit.json"), format!("{:#}\n", edit())).expect("edit.json");
        std::fs::write(out.join("expected/basic.png"), png(&basic, 1.0)).expect("expected preview");
        std::fs::write(out.join("expected/basic-scale-2.png"), png(&basic, 2.0)).expect("expected preview at scale 2");
        let tolerance = json!({
            "maxChannelDelta": 2,
            "maxDifferentPixels": 1500,
            "note": "The engine's own renderer draws the preview (the spread on its pasteboard); glyph edges may differ by a step between platforms."
        });
        std::fs::write(out.join("expected/tolerance.json"), format!("{tolerance:#}\n")).expect("tolerance");
        std::fs::write(repo().join("agent/checks.json"), format!("{:#}\n", catalogue_json())).expect("checks.json");
    }
    let mut failures = Vec::new();
    for entry in CATALOGUE {
        let bytes = fixture(entry.code).save(None).expect("save");
        let reopened = LayoutDocument::open("fixture.klayout", &bytes).expect("reopen");
        let found = codes(&reopened);
        if !found.iter().any(|code| code == entry.code) {
            failures.push(format!("{}: {found:?}", entry.code));
        }
        if write {
            std::fs::write(out.join(format!("check-{}.klayout", entry.code.trim_start_matches("layout."))), &bytes).expect("fixture");
            if entry.code == "layout.overset" {
                std::fs::write(out.join("overflow.klayout"), &bytes).expect("overflow");
            }
        }
    }
    assert!(failures.is_empty(), "fixtures that lose their code on save: {failures:#?}");
}
