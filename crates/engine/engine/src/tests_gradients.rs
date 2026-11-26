//! Gradient swatches by hand (plan 2026-11-25, phase 11): a named gradient swatch (linear or
//! radial, stops from colour values or colour swatches, midpoints) is made and changed with the
//! swatch commands, applied to fills and strokes, followed by everything using it, deleted with a
//! replacement, and kept by saving and reopening the `.klayout` file and by IDML export and import.

use serde_json::json;

use super::*;
use kition_layout_color::{Color, GradientKind, SwatchValue};

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 1})).unwrap();
    s.execute("swatch.add", &json!({"name": "Ink", "color": {"c": 100, "m": 0, "y": 0, "k": 0}})).unwrap();
    s
}

fn gradient(s: &mut Session, name: &str) -> kition_layout_color::Gradient {
    match &s.doc().unwrap().doc.swatch(name).unwrap_or_else(|| panic!("no swatch {name}")).value {
        SwatchValue::Gradient { gradient } => gradient.clone(),
        other => panic!("{other:?}"),
    }
}

const SUNSET: &str = r##"{"kind": "linear", "stops": [{"location": 0, "color": "Ink", "midpoint": 30}, {"location": 100, "color": "#ff0000"}]}"##;

#[test]
fn a_gradient_swatch_is_made_changed_and_followed_by_fills_and_strokes() {
    let mut s = session();
    let steps = s.doc().unwrap().undo_steps();
    let r = s.execute("swatch.add", &json!({"name": "Sunset", "gradient": serde_json::from_str::<serde_json::Value>(SUNSET).unwrap()})).unwrap();
    assert_eq!(r["name"], "Sunset");
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);
    let g = gradient(&mut s, "Sunset");
    assert_eq!(g.kind, GradientKind::Linear);
    assert_eq!(g.stops.len(), 2);
    assert_eq!(g.stops[0].color, Color::cmyk(1.0, 0.0, 0.0, 0.0), "a stop takes a colour swatch's colour");
    assert!((g.stops[0].midpoint - 0.3).abs() < 1e-6);
    assert_eq!(g.stops[1].color, Color::from_hex("#ff0000").unwrap());

    // On a fill and a stroke: the screen shows the gradient on both.
    let id = s.execute("frame.add", &json!({"rect": [36, 36, 236, 136], "content": "graphic"})).unwrap()["id"].as_u64().unwrap();
    s.execute("object.fill", &json!({"ids": [id], "swatch": "Sunset"})).unwrap();
    s.execute("object.stroke", &json!({"ids": [id], "swatch": "Sunset", "weight": 12})).unwrap();
    let d = s.doc().unwrap().doc.clone();
    let mut rr = kition_layout_render::Renderer::new();
    rr.threads = 0;
    let img = rr.render_page(&d, &s.cache, 0, 1.0, false, &Default::default()).unwrap();
    let (left, right) = (img.pixel(40, 86), img.pixel(232, 86));
    assert!(left[0] < 80 && left[2] > 150, "the stroke starts cyan: {left:?}");
    assert!(right[0] > 200 && right[2] < 80, "and ends red: {right:?}");

    // Swatch Options: radial, three stops; the object follows (it names the swatch).
    s.execute(
        "swatch.set",
        &json!({"name": "Sunset", "gradient": {"kind": "radial", "stops": [{"location": 0, "color": "#ffffff"}, {"location": 40, "color": "#00ff00", "midpoint": 70}, {"location": 100, "color": "#000000"}]}}),
    )
    .unwrap();
    let g = gradient(&mut s, "Sunset");
    assert_eq!((g.kind, g.stops.len()), (GradientKind::Radial, 3));
    assert_eq!(s.doc().unwrap().doc.item(kition_layout_doc::ItemId(id)).unwrap().fill.swatch, "Sunset");
    // A rename follows every use.
    s.execute("swatch.set", &json!({"name": "Sunset", "rename": "Dawn"})).unwrap();
    let it = s.doc().unwrap().doc.item(kition_layout_doc::ItemId(id)).unwrap().clone();
    assert_eq!((it.fill.swatch.as_str(), it.stroke.swatch.as_str()), ("Dawn", "Dawn"));

    // Refusals: one stop, a midpoint out of range, a colour on a gradient, a gradient on a colour.
    let one = json!({"stops": [{"location": 0, "color": "#000000"}]});
    assert!(s.execute("swatch.add", &json!({"gradient": one})).is_err());
    let bad_mid = json!({"stops": [{"location": 0, "color": "#000000", "midpoint": 95}, {"location": 100, "color": "#ffffff"}]});
    assert!(s.execute("swatch.set", &json!({"name": "Dawn", "gradient": bad_mid})).is_err());
    assert!(s.execute("swatch.set", &json!({"name": "Dawn", "color": "#123456"})).is_err());
    assert!(s.execute("swatch.set", &json!({"name": "Ink", "gradient": serde_json::from_str::<serde_json::Value>(SUNSET).unwrap()})).is_err());

    // Deleted with a replacement: the fill and stroke take it.
    s.execute("swatch.delete", &json!({"name": "Dawn", "replaceWith": "Ink"})).unwrap();
    let it = s.doc().unwrap().doc.item(kition_layout_doc::ItemId(id)).unwrap().clone();
    assert_eq!((it.fill.swatch.as_str(), it.stroke.swatch.as_str()), ("Ink", "Ink"));
    s.doc().unwrap().doc.check().unwrap();
}

#[test]
fn a_gradient_swatch_survives_save_and_reopen_and_idml() {
    let mut s = session();
    s.execute("swatch.add", &json!({"name": "Sunset", "gradient": serde_json::from_str::<serde_json::Value>(SUNSET).unwrap()})).unwrap();
    let id = s.execute("frame.add", &json!({"rect": [36, 36, 236, 136], "content": "graphic"})).unwrap()["id"].as_u64().unwrap();
    s.execute("object.fill", &json!({"ids": [id], "swatch": "Sunset"})).unwrap();
    s.execute("object.stroke", &json!({"ids": [id], "swatch": "Sunset", "weight": 4})).unwrap();
    let before = gradient(&mut s, "Sunset");

    // The .klayout file as the web app saves and opens it.
    let r = s.execute("file.serialize", &json!({})).unwrap();
    s.execute("file.openBytes", &json!({"name": "Gradient.klayout", "base64": r["base64"]})).unwrap();
    assert_eq!(gradient(&mut s, "Sunset"), before, "saved and reopened");
    let reopened = s.doc().unwrap().doc.clone();
    let it = reopened.all_items().into_iter().filter_map(|i| reopened.item(i)).find(|i| i.fill.swatch == "Sunset").cloned().unwrap();
    assert_eq!(it.stroke.swatch, "Sunset");

    // IDML: the named swatch, its kind, stops (colours, locations) and midpoint come back.
    let back = kition_layout_idml::import_idml(&kition_layout_idml::export_idml(&reopened)).unwrap();
    let w = back.swatch("Sunset").expect("the gradient swatch comes back by name");
    assert!(w.named && !w.hidden);
    let SwatchValue::Gradient { gradient: g } = &w.value else { panic!("{:?}", w.value) };
    assert_eq!(g.kind, before.kind);
    assert_eq!(g.stops.len(), 2);
    for (a, b) in g.stops.iter().zip(&before.stops) {
        assert!((a.offset - b.offset).abs() < 1e-4 && (a.midpoint - b.midpoint).abs() < 1e-4, "{a:?} vs {b:?}");
        assert_eq!(a.color.to_rgba8(1.0), b.color.to_rgba8(1.0));
    }
    let item =
        back.all_items().into_iter().filter_map(|i| back.item(i)).find(|i| i.fill.swatch == "Sunset").cloned().expect("the object still uses it");
    assert_eq!(item.stroke.swatch, "Sunset");
}
