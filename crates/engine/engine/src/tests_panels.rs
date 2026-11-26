//! Panels and styles by hand (plan 2026-11-25, stage 7): Swatch Options changes a swatch and
//! everything using it follows, a deleted swatch hands its uses to another, Redefine Style takes
//! the selected text's look, and a dialog previews several commands as one step.

use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 1})).unwrap();
    s
}

fn inspect(s: &mut Session) -> Value {
    s.execute("file.inspect", &json!({})).unwrap()
}

fn swatch(s: &mut Session, name: &str) -> Option<kition_layout_doc::color::Swatch> {
    s.doc().unwrap().doc.swatch(name).cloned()
}

fn item_fill(s: &mut Session, id: u64) -> String {
    s.doc().unwrap().doc.item(kition_layout_doc::ItemId(id)).unwrap().fill.swatch.clone()
}

/// A rectangle filled with `swatch`: its id.
fn filled(s: &mut Session, swatch: &str) -> u64 {
    let id = s.execute("frame.add", &json!({"rect": [36, 36, 136, 136], "content": "graphic"})).unwrap()["id"].as_u64().unwrap();
    s.execute("object.fill", &json!({"ids": [id], "swatch": swatch})).unwrap();
    id
}

#[test]
fn swatch_options_change_the_colour_and_a_new_name_follows_every_use() {
    let mut s = session();
    s.execute("swatch.add", &json!({"name": "Brand", "color": "#336699"})).unwrap();
    let rect = filled(&mut s, "Brand");
    let text = s.execute("frame.add", &json!({"rect": [36, 200, 300, 300], "content": "text", "text": "Coloured"})).unwrap();
    let story = text["story"].as_u64().unwrap();
    s.execute("text.select", &json!({"story": story, "anchor": 0, "focus": 8})).unwrap();
    s.execute("text.char", &json!({"attrs": {"fill": "Brand"}})).unwrap();
    s.execute("style.paragraph.add", &json!({"name": "Lead", "chars": {"fill": "Brand"}})).unwrap();
    let steps = s.doc().unwrap().undo_steps();

    // CMYK and spot, one undo step.
    s.execute("swatch.set", &json!({"name": "Brand", "color": {"c": 10, "m": 20, "y": 30, "k": 40}, "spot": true})).unwrap();
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);
    match swatch(&mut s, "Brand").unwrap().value {
        kition_layout_color::SwatchValue::Color { color, color_type } => {
            assert_eq!(color_type, kition_layout_color::ColorType::Spot);
            assert!(matches!(color, kition_layout_color::Color::Cmyk { .. }), "{color:?}");
        }
        other => panic!("{other:?}"),
    }

    // Renamed: the object, the text and the style use the new name.
    s.execute("swatch.set", &json!({"name": "Brand", "rename": "Ocean"})).unwrap();
    assert!(swatch(&mut s, "Brand").is_none());
    assert!(swatch(&mut s, "Ocean").is_some());
    assert_eq!(item_fill(&mut s, rect), "Ocean");
    s.execute("text.select", &json!({"story": story, "anchor": 1, "focus": 2})).unwrap();
    assert_eq!(s.execute("text.selectionAttrs", &json!({})).unwrap()["chars"]["fill"], "Ocean");
    let lead = s.doc().unwrap().doc.styles.para("Lead").unwrap().chars.fill.clone();
    assert_eq!(lead.as_deref(), Some("Ocean"));
    // No two swatches share a name; special swatches keep theirs.
    s.execute("swatch.add", &json!({"name": "Sand", "color": "#c2b280"})).unwrap();
    assert!(s.execute("swatch.set", &json!({"name": "Sand", "rename": "Ocean"})).is_err());
    assert!(s.execute("swatch.set", &json!({"name": "[Black]", "rename": "Ink"})).is_err());
    assert!(s.execute("swatch.set", &json!({"name": "[Black]", "color": "#101010"})).is_err());
    assert!(s.execute("swatch.set", &json!({"name": "Sand", "tint": 0.5})).is_err(), "a colour swatch has no tint");
    // Undo takes the rename back everywhere.
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(item_fill(&mut s, rect), "Brand");
}

#[test]
fn a_swatch_named_for_its_values_follows_them() {
    let mut s = session();
    let made = s.execute("swatch.add", &json!({"color": "#ff0000"})).unwrap()["name"].as_str().unwrap().to_string();
    assert_eq!(made, "R=255 G=0 B=0");
    s.edit(|d, _| {
        d.swatches.iter_mut().find(|w| w.name == made).unwrap().named = false;
        Ok(json!(null))
    })
    .unwrap();
    let rect = filled(&mut s, &made);
    let answer = s.execute("swatch.set", &json!({"name": made, "color": "#00ff00"})).unwrap();
    assert_eq!(answer["name"], "R=0 G=255 B=0");
    assert_eq!(item_fill(&mut s, rect), "R=0 G=255 B=0");
}

#[test]
fn a_deleted_swatch_hands_its_uses_to_the_one_chosen() {
    let mut s = session();
    s.execute("swatch.add", &json!({"name": "Old", "color": "#884422"})).unwrap();
    s.execute("swatch.add", &json!({"name": "New", "color": "#224488"})).unwrap();
    let rect = filled(&mut s, "Old");
    let text = s.execute("frame.add", &json!({"rect": [36, 200, 300, 300], "content": "text", "text": "Words"})).unwrap();
    let story = text["story"].as_u64().unwrap();
    s.execute("text.select", &json!({"story": story, "anchor": 0, "focus": 5})).unwrap();
    s.execute("text.char", &json!({"attrs": {"fill": "Old", "stroke": "Old"}})).unwrap();
    assert!(s.execute("swatch.delete", &json!({"name": "Old", "replaceWith": "Old"})).is_err());
    assert!(s.execute("swatch.delete", &json!({"name": "Old", "replaceWith": "Missing"})).is_err());
    let steps = s.doc().unwrap().undo_steps();
    s.execute("swatch.delete", &json!({"name": "Old", "replaceWith": "New"})).unwrap();
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);
    assert!(swatch(&mut s, "Old").is_none());
    assert_eq!(item_fill(&mut s, rect), "New");
    s.execute("text.select", &json!({"story": story, "anchor": 1, "focus": 2})).unwrap();
    let chars = s.execute("text.selectionAttrs", &json!({})).unwrap()["chars"].clone();
    assert_eq!((chars["fill"].as_str(), chars["stroke"].as_str()), (Some("New"), Some("New")));
    // Without a replacement the uses keep the name, as before.
    s.execute("swatch.delete", &json!({"name": "New"})).unwrap();
    assert_eq!(item_fill(&mut s, rect), "New");
}

#[test]
fn a_tint_swatch_changes_its_tint() {
    let mut s = session();
    s.execute("swatch.add", &json!({"name": "Base", "color": "#0000ff"})).unwrap();
    s.edit(|d, _| {
        d.swatches.push(kition_layout_color::Swatch {
            name: "Base 50%".into(),
            value: kition_layout_color::SwatchValue::Tint { base: "Base".into(), tint: 0.5 },
            locked: false,
            named: true,
            hidden: false,
        });
        Ok(json!(null))
    })
    .unwrap();
    s.execute("swatch.set", &json!({"name": "Base 50%", "tint": 0.25})).unwrap();
    assert!(matches!(swatch(&mut s, "Base 50%").unwrap().value, kition_layout_color::SwatchValue::Tint { tint, .. } if (tint - 0.25).abs() < 1e-6));
    assert!(s.execute("swatch.set", &json!({"name": "Base 50%", "color": "#ffffff"})).is_err());
    assert!(s.execute("swatch.set", &json!({"name": "Base 50%", "tint": 2})).is_err());
    // Renaming the base renames the tint's base too.
    s.execute("swatch.set", &json!({"name": "Base", "rename": "Blue"})).unwrap();
    assert!(matches!(swatch(&mut s, "Base 50%").unwrap().value, kition_layout_color::SwatchValue::Tint { ref base, .. } if base == "Blue"));
}

/// A text frame with two paragraphs, both in `style`: (session, story).
fn styled_text(style: &str) -> (Session, u64) {
    let mut s = session();
    s.execute("style.paragraph.add", &json!({"name": style, "chars": {"size": 12}, "para": {"spaceAfter": 6}})).unwrap();
    let r = s.execute("frame.add", &json!({"rect": [36, 36, 400, 300], "content": "text", "text": "First paragraph\rSecond paragraph"})).unwrap();
    let story = r["story"].as_u64().unwrap();
    s.execute("text.select", &json!({"story": story, "anchor": 0, "focus": 32})).unwrap();
    s.execute("style.paragraph.apply", &json!({"name": style})).unwrap();
    (s, story)
}

#[test]
fn redefine_paragraph_style_takes_the_selected_texts_look() {
    let (mut s, story) = styled_text("Body");
    // The first paragraph is changed by hand: bigger, centred, more space after.
    s.execute("text.select", &json!({"story": story, "anchor": 0, "focus": 15})).unwrap();
    s.execute("text.char", &json!({"attrs": {"size": 18}})).unwrap();
    s.execute("text.para", &json!({"attrs": {"align": "center", "spaceAfter": 12}})).unwrap();
    let at = s.execute("text.selectionAttrs", &json!({})).unwrap();
    assert!(at["paraOverrides"].as_u64().unwrap() > 0 && at["charOverrides"].as_u64().unwrap() > 0);
    let steps = s.doc().unwrap().undo_steps();
    s.execute("style.paragraph.edit", &json!({"name": "Body", "fromSelection": true})).unwrap();
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);
    let st = s.doc().unwrap().doc.styles.para("Body").unwrap().clone();
    assert_eq!(st.chars.size, Some(18.0));
    assert_eq!(st.para.space_after, Some(12.0));
    assert_eq!(serde_json::to_value(st.para.align).unwrap(), json!("center"));
    // The first paragraph's overrides say what the style says now: no "+".
    let at = s.execute("text.selectionAttrs", &json!({})).unwrap();
    assert_eq!((at["paraOverrides"].as_u64(), at["charOverrides"].as_u64()), (Some(0), Some(0)), "{at}");
    // The second paragraph follows the style.
    s.execute("text.select", &json!({"story": story, "anchor": 20, "focus": 21})).unwrap();
    let second = s.execute("text.selectionAttrs", &json!({})).unwrap();
    assert_eq!(second["chars"]["size"], 18.0);
    assert_eq!(second["para"]["align"], "center");
    // With nothing selected there is no look to take.
    s.execute("select.set", &json!({"ids": []})).unwrap();
    assert!(s.execute("style.paragraph.edit", &json!({"name": "Body", "fromSelection": true})).is_err());
}

#[test]
fn redefine_character_style_takes_the_selected_texts_look() {
    let (mut s, story) = styled_text("Body");
    s.execute("style.character.add", &json!({"name": "Strong", "chars": {"fontStyle": "Bold"}})).unwrap();
    s.execute("text.select", &json!({"story": story, "anchor": 0, "focus": 5})).unwrap();
    s.execute("style.character.apply", &json!({"name": "Strong"})).unwrap();
    s.execute("text.char", &json!({"attrs": {"tracking": 50}})).unwrap();
    s.execute("style.character.edit", &json!({"name": "Strong", "fromSelection": true})).unwrap();
    let st = s.doc().unwrap().doc.styles.char_style("Strong").unwrap().clone();
    assert_eq!(st.chars.tracking, Some(50.0), "where the text differs from its paragraph");
    assert_eq!(st.chars.font_style.as_deref(), Some("Bold"), "what the style set stays");
    assert_eq!(st.chars.size, None, "what the text shares with its paragraph is not set");
}

#[test]
fn a_dialog_previews_several_commands_as_one_step() {
    let mut s = session();
    let rect = filled(&mut s, "[Black]");
    s.execute("select.set", &json!({"ids": [rect]})).unwrap();
    let opacity = |s: &mut Session| s.doc().unwrap().doc.item(kition_layout_doc::ItemId(rect)).unwrap().opacity;
    let shadow = |s: &mut Session| s.doc().unwrap().doc.item(kition_layout_doc::ItemId(rect)).unwrap().effects.drop_shadow.on;
    let steps = s.doc().unwrap().undo_steps();
    let effects = |o: f64| {
        vec![
            ("object.opacity".to_string(), json!({"ids": [rect], "opacity": o})),
            ("object.dropShadow".to_string(), json!({"ids": [rect], "on": true})),
        ]
    };
    s.preview_steps("Effects", &effects(0.5)).unwrap();
    assert!(s.previewing());
    assert!((opacity(&mut s) - 0.5).abs() < 1e-6 && shadow(&mut s));
    // A new preview replaces the last one (the shadow is not turned off by a second toggle).
    s.preview_steps("Effects", &effects(0.25)).unwrap();
    assert!((opacity(&mut s) - 0.25).abs() < 1e-6 && shadow(&mut s));
    assert_eq!(s.doc().unwrap().undo_steps(), steps);
    // Cancel: as it was, nothing to undo or redo.
    assert!(s.cancel_preview());
    assert!((opacity(&mut s) - 1.0).abs() < 1e-6 && !shadow(&mut s));
    assert_eq!(s.doc().unwrap().undo_steps(), steps);
    assert!(!s.doc().unwrap().can_redo());
    // OK: one step named for the dialog, journaled as its commands.
    s.preview_steps("Effects", &effects(0.4)).unwrap();
    let journaled = s.journal.len();
    assert!(s.commit_preview().unwrap());
    assert_eq!(s.doc().unwrap().undo_steps(), steps + 1);
    assert_eq!(s.doc().unwrap().history_labels().0.last().map(String::as_str), Some("Effects"));
    assert_eq!(s.journal.len(), journaled + 2);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!((opacity(&mut s) - 1.0).abs() < 1e-6 && !shadow(&mut s));
    // A step that fails leaves the document as it was before the steps.
    let bad = vec![
        ("object.opacity".to_string(), json!({"ids": [rect], "opacity": 0.3})),
        ("object.opacity".to_string(), json!({"ids": [rect], "blend": "nonsense"})),
    ];
    assert!(s.preview_steps("Effects", &bad).is_err());
    assert!((opacity(&mut s) - 1.0).abs() < 1e-6);
    s.cancel_preview();
    // A query is no preview.
    assert!(s.preview_steps("Effects", &[("file.inspect".to_string(), json!({}))]).is_err());
    let _ = inspect(&mut s);
}
