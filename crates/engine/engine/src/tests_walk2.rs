//! The second hands-on walkthrough's fixes (plan 2026-11-25, phase 12): every key reaches one
//! command, Esc in a table cell, the spelling word list knows common typos are typos, and parent
//! objects wrap a page's text.

use std::collections::HashMap;

use kition_layout_tools::{Mods, ToolKey};

use super::*;

/// Cmd+Alt+3 belonged to Transform Again and to Show All on Spread; the editor ran the first
/// (Transform Again, greyed out with nothing selected), so the key never showed hidden objects.
/// No two commands share a shortcut.
#[test]
fn no_two_commands_share_a_shortcut() {
    let s = Session::new();
    let mut owners: HashMap<String, Vec<&str>> = HashMap::new();
    for info in s.commands() {
        if let Some(key) = info.shortcut {
            owners.entry(key.to_lowercase()).or_default().push(info.id);
        }
    }
    let shared: Vec<_> = owners.into_iter().filter(|(_, ids)| ids.len() > 1).collect();
    assert!(shared.is_empty(), "shared shortcuts: {shared:?}");
    let show = s.commands().into_iter().find(|c| c.id == "object.showAll").unwrap();
    assert_eq!(show.shortcut, Some("Cmd+Alt+3"));
}

/// Esc in a table cell's text selects the cell, and the selected cell takes no typed text, so a
/// tool key there never types into it (walkthrough 2, D-3: "v" went into the header after Esc and
/// V); Esc again goes back to the cell's text (as the reference app and InDesign: the existing
/// `text.mjs` and the text-and-tables cases pin this toggle).
#[test]
fn esc_in_a_cell_selects_it_and_typing_then_stops() {
    let mut s = Session::new();
    s.execute("file.new", &serde_json::json!({"pages": 1})).unwrap();
    let r = s.execute("frame.add", &serde_json::json!({"rect": [36, 36, 336, 500], "content": "text"})).unwrap();
    let frame = r["id"].as_u64().unwrap();
    s.execute("table.insert", &serde_json::json!({"rows": 2, "cols": 2})).unwrap();
    s.set_tool("type");
    s.type_text("City").unwrap();
    assert!(s.wants_text());
    let view = tooling::ViewInfo::default();
    s.tool_key(ToolKey::Escape, Mods::default(), view).unwrap();
    let sel = s.doc().unwrap().selection.clone();
    assert!(sel.cells.is_some(), "the cell is selected: {sel:?}");
    assert!(!s.wants_text(), "a selected cell takes no typed text");
    // Esc again: the cell's text (the toggle); and Esc once more, the cell.
    s.tool_key(ToolKey::Escape, Mods::default(), view).unwrap();
    let sel = s.doc().unwrap().selection.clone();
    assert!(sel.cells.is_none() && sel.text.is_some_and(|t| t.cell.is_some()), "{sel:?}");
    assert!(s.wants_text());
    s.tool_key(ToolKey::Escape, Mods::default(), view).unwrap();
    assert!(!s.wants_text());
    // The letter key the editor then gives to the tools (V) chooses the Selection tool, typing nothing.
    s.set_tool("selection");
    let story = s.doc().unwrap().doc.item(kition_layout_doc::ItemId(frame)).unwrap().text_frame().unwrap().story;
    let table = *s.doc().unwrap().doc.story(story).unwrap().tables.keys().next().unwrap();
    let cell = s.execute("table.get", &serde_json::json!({"story": story.0, "table": table})).unwrap();
    assert_eq!(cell["cells"][0]["text"], "City");
}

/// Check Spelling flagged neither "Teh" nor "teh" (walkthrough 2, D-5): the word list, built for
/// hyphenation, carries a few transposition typos of the commonest words as words. They are flagged
/// with the word they mistype among the suggestions; the user dictionary still wins.
#[test]
fn spelling_flags_teh_and_the_other_listed_typos() {
    if kition_layout_compose::hyphen::Dictionary::en_us().is_empty() {
        return; // the word list is not bundled in this build
    }
    let mut s = Session::new();
    s.execute("file.new", &serde_json::json!({})).unwrap();
    s.execute("frame.add", &serde_json::json!({"rect": [36, 36, 400, 200], "content": "text", "text": "Teh cat nad teh dog ont here."})).unwrap();
    let r = s.execute("spelling.check", &serde_json::json!({})).unwrap();
    let words: Vec<&str> = r.as_array().unwrap().iter().map(|m| m["word"].as_str().unwrap()).collect();
    assert_eq!(words, vec!["Teh", "nad", "teh", "ont"]);
    let suggests = |w: usize, word: &str| r[w]["suggestions"].as_array().unwrap().iter().any(|v| v.as_str().unwrap().eq_ignore_ascii_case(word));
    assert!(suggests(0, "the") && suggests(1, "and") && suggests(2, "the") && suggests(3, "not"), "{r}");
    s.execute("spelling.addWord", &serde_json::json!({"word": "teh"})).unwrap();
    let r = s.execute("spelling.check", &serde_json::json!({})).unwrap();
    let words: Vec<&str> = r.as_array().unwrap().iter().map(|m| m["word"].as_str().unwrap()).collect();
    assert_eq!(words, vec!["nad", "ont"]);
}

/// A parent's object with a text wrap pushes a page's text aside: the page's parent's, and that of
/// the parent it is based on (walkthrough 2 and phase 11: the based-on parent's object drew on the
/// page but the text ran under it). Changing the parent object's wrap recomposes the page's story.
#[test]
fn parent_objects_wrap_the_pages_text_through_the_based_on_chain() {
    let mut s = Session::new();
    s.execute("file.new", &serde_json::json!({"pages": 1, "facingPages": false})).unwrap();
    s.execute("layout.parents.new", &serde_json::json!({})).unwrap();
    s.execute("layout.parents.set", &serde_json::json!({"parent": "B", "basedOn": "A"})).unwrap();
    s.execute("layout.pages.applyParent", &serde_json::json!({"pages": [1], "parent": "B"})).unwrap();
    let text = ["Warm boots, a tripod, spare batteries, a red torch and patience."; 12].join(" ");
    let r = s.execute("frame.add", &serde_json::json!({"rect": [36, 36, 336, 500], "content": "text", "text": text})).unwrap();
    let story = kition_layout_doc::StoryId(r["story"].as_u64().unwrap());
    let first_x = |s: &Session| {
        let d = s.doc().unwrap().doc.clone();
        s.cache.get(&d, story, None).frames[0].lines[0].x0
    };
    let base = first_x(&s);
    // A square on parent A (B is based on A), over the frame's top left corner, wrapped.
    for prefix in ["A", "B"] {
        let pi = s.doc().unwrap().doc.parents.iter().position(|p| p.parent.as_ref().is_some_and(|i| i.prefix == prefix)).unwrap();
        let x = s.doc().unwrap().doc.parents[pi].pages[0].x;
        let id = s
            .execute("frame.add", &serde_json::json!({"spread": prefix, "rect": [x + 30.0, 30.0, x + 130.0, 90.0], "content": "graphic"}))
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        assert_eq!(first_x(&s), base, "{prefix}: no wrap on the square yet");
        s.execute("object.textWrap", &serde_json::json!({"ids": [id], "mode": "boundingBox"})).unwrap();
        assert!(first_x(&s) > base + 90.0, "{prefix}: the first line starts right of the parent's square: {} from {base}", first_x(&s));
        s.execute("object.textWrap", &serde_json::json!({"ids": [id], "mode": "none"})).unwrap();
        assert_eq!(first_x(&s), base, "{prefix}: no wrap again");
    }
}
