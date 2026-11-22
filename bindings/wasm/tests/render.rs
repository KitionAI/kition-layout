//! `file.render`, the kit's render command (`kit.json` `agent.render`; Kition platform rule K21,
//! KHP A24, A39): `layout_inspect` with `preview: true` draws through it. It takes `scale`,
//! `maxSize` and `target` (a page, a spread, or an object, in the shape of a diagnostic's target)
//! and answers a top-level base64 `png`; without a target it draws the whole canvas exactly as the
//! conformance expected previews show it. It never changes the document.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::Path;

use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

const MANIFEST: &str = include_str!("../../../kit.json");

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn open(name: &str) -> LayoutDocument {
    LayoutDocument::open(name, &std::fs::read(repo().join("conformance").join(name)).unwrap()).unwrap()
}

fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for byte in text.trim_end_matches('=').bytes() {
        buffer = (buffer << 6) | ALPHABET.iter().position(|known| *known == byte).unwrap() as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    out
}

/// The command's answer, decoded: the PNG it answered at the top of its result.
fn render(doc: &mut LayoutDocument, params: Value) -> image::RgbaImage {
    let answer = doc.execute("file.render", params.clone()).unwrap_or_else(|error| panic!("file.render {params}: {error}"));
    let png = base64_decode(answer["png"].as_str().unwrap_or_else(|| panic!("a top-level png in {answer}")));
    let picture = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap().to_rgba8();
    assert_eq!((answer["width"].as_u64(), answer["height"].as_u64()), (Some(u64::from(picture.width())), Some(u64::from(picture.height()))));
    picture
}

fn expected(name: &str) -> image::RgbaImage {
    image::open(repo().join("conformance/expected").join(name)).unwrap().to_rgba8()
}

/// The expected previews' tolerance (`conformance/expected/tolerance.json`).
fn assert_like(what: &str, got: &image::RgbaImage, want: &image::RgbaImage) {
    assert_eq!(got.dimensions(), want.dimensions(), "{what}: size");
    let different = got.pixels().zip(want.pixels()).filter(|(a, b)| a.0.iter().zip(b.0).any(|(x, y)| x.abs_diff(y) > 2)).count();
    assert!(different <= 1500, "{what}: {different} pixels differ");
}

#[test]
fn the_manifest_declares_the_render_command() {
    let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
    assert_eq!(manifest["agent"]["render"], "file.render");
    let doc = open("basic.klayout");
    let command = doc.commands().into_iter().find(|command| command.id == "file.render").expect("offered");
    assert!(!command.mutates, "a preview is never an edit");
    let schema = command.schema.unwrap();
    for param in ["scale", "maxSize", "target", "slide"] {
        assert!(schema.get(param).is_some() || schema["properties"].get(param).is_some(), "{param} in {schema}");
    }
}

#[test]
fn without_a_target_it_draws_the_canvas_as_the_expected_previews() {
    let mut doc = open("basic.klayout");
    let whole = render(&mut doc, json!({ "scale": 1 }));
    let canvas = doc.render(0.0, 0.0, 1.0, doc.width(), doc.height()).unwrap();
    assert_eq!(whole.as_raw(), &canvas, "the canvas, exactly");
    assert_like("scale 1", &whole, &expected("basic.png"));
    assert_like("scale 2", &render(&mut doc, json!({ "scale": 2 })), &expected("basic-scale-2.png"));
    assert!(!doc.dirty());
}

#[test]
fn a_page_a_spread_and_an_object_are_drawn_alone() {
    let mut doc = open("check-overset.klayout");
    // A page (counted from 1, as diagnostics count) and its spread: a letter page, 612 x 792 points.
    assert_eq!(render(&mut doc, json!({ "scale": 1, "target": { "page": 1 } })).dimensions(), (612, 792));
    assert_eq!(render(&mut doc, json!({ "scale": 2, "target": { "spread": 0 } })).dimensions(), (1224, 1584));
    assert_eq!(render(&mut doc, json!({ "scale": 1, "slide": 0 })).dimensions(), (612, 792), "the host's part number, from 0");
    // A diagnostic's target, passed on unchanged: the overset frame alone.
    let found = doc.check();
    let target = serde_json::to_value(&found[0].target).unwrap();
    assert!(target["path"].as_str().unwrap().starts_with("item:"), "{target}");
    let frame = render(&mut doc, json!({ "scale": 1, "target": target }));
    assert!(frame.width() < 612 && frame.height() < 792, "the frame, not the page: {:?}", frame.dimensions());
    // `maxSize` bounds the longer side.
    let small = render(&mut doc, json!({ "scale": 2, "maxSize": 200 }));
    assert!(small.width().max(small.height()) <= 200 && small.width().max(small.height()) >= 199, "{:?}", small.dimensions());
    assert!(!doc.dirty(), "rendering changes nothing");
}

#[test]
fn what_it_cannot_draw_is_an_error() {
    let mut doc = open("basic.klayout");
    for params in [
        json!({ "scale": 3 }),
        json!({ "scale": 0 }),
        json!({ "scale": 1, "maxSize": 5000 }),
        json!({ "scale": 1, "target": { "page": 9 } }),
        json!({ "scale": 1, "target": { "spread": 4 } }),
        json!({ "scale": 1, "target": { "path": "item:999999" } }),
        json!({ "scale": 1, "target": { "path": "story:1" } }),
    ] {
        assert!(doc.execute("file.render", params.clone()).is_err(), "{params}");
    }
}
