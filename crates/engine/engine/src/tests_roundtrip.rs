//! Command-level round trips: a realistic edit made only with commands survives saving the
//! `.klayout` file and opening it again, both as the web app saves (`file.serialize`, then
//! `file.openBytes`) and as the desktop does (`file.saveAs`, then `file.open`). The whole document
//! model is compared (id-blind), so any field the file drops fails the test.

use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages": 2})).unwrap();
    s
}

fn norm(s: &Session) -> Value {
    kition_layout_kdm::compare::normalize(&s.doc().unwrap().doc)
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap()
}

/// The first JSON pointer at which two documents differ, for a readable failure.
fn first_difference(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                let (u, v) = (x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null));
                if let Some(d) = first_difference(u, v, &format!("{at}/{k}")) {
                    return Some(d);
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            x.iter().zip(y).enumerate().find_map(|(i, (u, v))| first_difference(u, v, &format!("{at}/{i}")))
        }
        _ if a == b => None,
        _ => Some(format!("{at}: {a} != {b}")),
    }
}

/// Reading a file mints new local ids, so documents are compared in the kit's id-blind form
/// ([`kition_layout_kdm::compare::normalize`]), which renumbers every id by position and leaves out
/// only what the file does not store (the id counter, assets' disk paths, pages' view rotation).
fn assert_same(before: &Document, after: &Document, how: &str) {
    let (a, b) = (kition_layout_kdm::compare::normalize(before), kition_layout_kdm::compare::normalize(after));
    if a != b {
        panic!("{how} changed the document at {}", first_difference(&a, &b, "").unwrap_or_default());
    }
}

/// Save and reopen both ways; each reopened document must equal the edited one.
fn survives_save_and_reopen(s: &mut Session, name: &str) {
    s.doc().unwrap().doc.check().unwrap();
    let before = (*s.doc().unwrap().doc).clone();

    let r = s.execute("file.serialize", &json!({})).unwrap();
    s.execute("file.openBytes", &json!({"name": name, "base64": r["base64"]})).unwrap();
    assert_same(&before, &s.doc().unwrap().doc, "file.serialize then file.openBytes");

    let dir = std::env::temp_dir().join(format!("kition-layout-roundtrip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.klayout"));
    let path = path.to_str().unwrap();
    s.execute("file.saveAs", &json!({"path": path})).unwrap();
    s.execute("file.open", &json!({"path": path})).unwrap();
    assert_same(&before, &s.doc().unwrap().doc, "file.saveAs then file.open");
    std::fs::remove_file(path).ok();
}

#[test]
fn threaded_text_frames_survive_save_and_reopen() {
    let mut s = session();
    let a = s.execute("frame.add", &json!({"rect": [36, 36, 300, 200], "content": "text"})).unwrap();
    let story = a["story"].as_u64().unwrap();
    s.execute("text.insert", &json!({"text": "A story that runs from the first frame into the second one."})).unwrap();
    let b = s.execute("frame.add", &json!({"rect": [36, 260, 300, 420], "content": "text", "caret": false})).unwrap();
    s.execute("frame.thread", &json!({"from": id(&a), "to": id(&b)})).unwrap();
    s.execute("object.textFrameOptions", &json!({"ids": [id(&a)], "columns": 2, "gutter": 12})).unwrap();
    assert_eq!(s.execute("story.get", &json!({"story": story})).unwrap()["text"], "A story that runs from the first frame into the second one.");
    let d = norm(&s);
    let stories: Vec<&Value> = d["stories"].as_object().unwrap().values().collect();
    assert_eq!(stories.len(), 1, "two threaded frames share one story");
    assert_eq!(stories[0]["frames"].as_array().unwrap().len(), 2);
    survives_save_and_reopen(&mut s, "threaded-text");
}

#[test]
fn paragraph_and_character_styles_survive_save_and_reopen() {
    let mut s = session();
    let f = s.execute("frame.add", &json!({"rect": [36, 36, 400, 300], "content": "text"})).unwrap();
    let story = f["story"].as_u64().unwrap();
    s.execute("text.insert", &json!({"text": "Heading\nBody text with emphasis"})).unwrap();
    s.execute("style.paragraph.add", &json!({"name": "Head", "chars": {"size": 24}})).unwrap();
    s.execute("text.select", &json!({"story": story, "anchor": 0, "focus": 7})).unwrap();
    s.execute("style.paragraph.apply", &json!({"name": "Head"})).unwrap();
    s.execute("style.character.add", &json!({"name": "Emphasis", "chars": {"fontStyle": "Italic"}})).unwrap();
    s.execute("text.select", &json!({"story": story, "anchor": 23, "focus": 31})).unwrap();
    s.execute("style.character.apply", &json!({"name": "Emphasis"})).unwrap();
    s.execute("text.para", &json!({"attrs": {"listType": "numbers", "listName": "Steps"}})).unwrap();
    let d = norm(&s);
    let story = d["stories"].as_object().unwrap().values().next().unwrap().clone();
    assert_eq!(story["paras"][0]["style"], "Head");
    assert_eq!(story["paras"][1]["para"]["listType"], "numbers");
    assert_eq!(story["chars"][1], json!({"len": 8, "style": "Emphasis"}));
    assert!(d["styles"]["character"].as_array().unwrap().iter().any(|c| c["name"] == "Emphasis" && c["chars"]["fontStyle"] == "Italic"));
    survives_save_and_reopen(&mut s, "text-styles");
}

#[test]
fn paths_fills_strokes_and_effects_survive_save_and_reopen() {
    let mut s = session();
    s.execute("swatch.add", &json!({"name": "Brand Teal", "color": "#108080"})).unwrap();
    let shape = s
        .execute(
            "path.add",
            &json!({"anchors": [{"p": [100, 100]}, {"p": [300, 100], "in": [260, 60], "out": [340, 140]}, {"p": [200, 300]}], "closed": true}),
        )
        .unwrap();
    let frame = s.execute("frame.add", &json!({"rect": [320, 320, 480, 480], "content": "graphic"})).unwrap();
    s.execute("object.fill", &json!({"ids": [id(&shape)], "swatch": "Brand Teal"})).unwrap();
    s.execute("object.stroke", &json!({"ids": [id(&shape)], "weight": 4, "swatch": "[Black]"})).unwrap();
    s.execute("object.opacity", &json!({"ids": [id(&frame)], "opacity": 0.5})).unwrap();
    s.execute("object.dropShadow", &json!({"on": true, "angle": 45, "ids": [id(&frame)]})).unwrap();
    s.execute("select.set", &json!({"ids": [id(&frame)]})).unwrap();
    s.execute("object.gradient", &json!({"from": [320, 400], "to": [480, 400]})).unwrap();
    let d = norm(&s);
    let (path, framed) = (&d["spreads"][0]["items"][0], &d["spreads"][0]["items"][1]);
    assert_eq!(path["shape"], "path");
    assert_eq!(path["path"]["subpaths"][0]["anchors"][1]["out"], json!([340, 140]));
    assert_eq!((path["fill"]["swatch"].clone(), path["stroke"]["weight"].clone()), (json!("Brand Teal"), json!(4)));
    assert_eq!((framed["opacity"].clone(), framed["effects"]["dropShadow"]["on"].clone()), (json!(0.5), json!(true)));
    survives_save_and_reopen(&mut s, "paths-and-effects");
}

#[test]
fn layers_groups_and_transforms_survive_save_and_reopen() {
    let mut s = session();
    s.execute("layer.new", &json!({"name": "Art"})).unwrap();
    let a = s.execute("frame.add", &json!({"rect": [100, 100, 200, 200]})).unwrap();
    let b = s.execute("frame.add", &json!({"rect": [220, 100, 320, 200]})).unwrap();
    let g = s.execute("arrange.group", &json!({"ids": [id(&a), id(&b)]})).unwrap();
    let g = g["id"].as_u64().or_else(|| g["group"].as_u64()).unwrap_or_else(|| panic!("arrange.group answered {g}"));
    s.execute("transform.rotate", &json!({"angle": 10, "ids": [g]})).unwrap();
    s.execute("select.set", &json!({"ids": [g]})).unwrap();
    s.execute("transform.move", &json!({"dx": 24, "dy": 12})).unwrap();
    s.execute("object.arrange", &json!({"to": "back"})).unwrap();
    let layers = s.execute("file.inspect", &json!({})).unwrap()["layers"].clone();
    let art = layers.as_array().unwrap().iter().find(|l| l["name"] == "Art").unwrap()["id"].clone();
    s.execute("layer.set", &json!({"id": art, "locked": true})).unwrap();
    let d = norm(&s);
    assert!(d["layers"].as_array().unwrap().iter().any(|l| l["name"] == "Art" && l["locked"] == true));
    let group = &d["spreads"][0]["items"][0];
    assert_eq!(group["content"]["items"].as_array().unwrap().len(), 2, "the group is sent to the back with both frames");
    assert_ne!(group["xf"], json!([1, 0, 0, 1, 0, 0]), "rotated and moved");
    survives_save_and_reopen(&mut s, "layers-groups-transforms");
}

#[test]
fn pages_guides_and_backgrounds_survive_save_and_reopen() {
    let mut s = session();
    s.execute("layout.pages.insert", &json!({"count": 2})).unwrap();
    s.execute("guide.add", &json!({"orientation": "vertical", "position": 200, "page": 0})).unwrap();
    s.execute("layout.addGuides", &json!({"spread": 0, "columns": 3, "rows": 2, "rowGutter": 0.0})).unwrap();
    s.execute("page.background", &json!({"color": "#f8f5e8"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.page_count(), 4);
    let d = norm(&s);
    assert!(d["spreads"][0]["pages"][0]["guides"].as_array().unwrap().len() > 1, "the added guide and the grid's");
    let parent_items = d["parents"].as_array().unwrap().iter().flat_map(|p| p["items"].as_array().unwrap().clone()).collect::<Vec<_>>();
    assert!(parent_items.iter().any(|it| it["label"] == "kition.background" && it["locked"] == true));
    survives_save_and_reopen(&mut s, "pages-and-guides");
}

#[test]
fn tables_survive_save_and_reopen() {
    let mut s = session();
    s.execute("frame.add", &json!({"rect": [36, 36, 500, 400], "content": "text"})).unwrap();
    s.execute("table.insert", &json!({"rows": 3, "cols": 3, "headerRows": 1})).unwrap();
    for (row, text) in ["Name", "Alpha", "Beta"].iter().enumerate() {
        s.execute("table.setCell", &json!({"row": row, "col": 0, "text": text})).unwrap();
    }
    s.execute("table.setCell", &json!({"row": 1, "col": 1, "text": "12"})).unwrap();
    s.execute("table.merge", &json!({"rows": [2, 2], "cols": [1, 2]})).unwrap();
    let d = norm(&s);
    let story = d["stories"].as_object().unwrap().values().find(|st| st.get("tables").is_some()).unwrap().clone();
    let table = story["tables"].as_object().unwrap().values().next().unwrap().clone();
    let cells = table["cells"].as_array().unwrap();
    let texts: Vec<&str> = cells.iter().map(|c| c["text"]["text"].as_str().unwrap()).collect();
    assert!(["Name", "Alpha", "Beta", "12"].iter().all(|t| texts.contains(t)), "{texts:?}");
    assert!(cells.iter().any(|c| c["colSpan"] == 2), "the merged cell");
    survives_save_and_reopen(&mut s, "tables");
}
