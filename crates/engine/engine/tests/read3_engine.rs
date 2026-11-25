//! C8-layout-read-3: engine defects the readings named, each acting on something other than
//! what its parameters name or losing data.
use kition_layout_engine::Session;
use kition_layout_engine::doc::{Content, ItemId};
use serde_json::json;

fn png(shade: u8) -> String {
    let png = kition_layout_render::Rendered { width: 2, height: 2, pixels: vec![shade; 16] }.to_png();
    kition_layout_engine::cmd::base64_encode(&png)
}

fn graphic_asset(session: &Session, id: u64) -> kition_layout_engine::doc::AssetId {
    match &session.doc().unwrap().doc.item(ItemId(id)).unwrap().content {
        Content::Graphic(graphic) => graphic.asset,
        other => panic!("not a graphic: {other:?}"),
    }
}

/// A snippet's image whose asset id another image of the destination already uses keeps its own
/// pixels (cmd/object.rs `renumber` kept the destination's asset).
#[test]
fn placing_a_snippet_keeps_its_own_image_when_asset_ids_collide() {
    let make = |shade: u8| {
        let mut session = Session::new();
        session.execute("file.new", &json!({"pages":1})).unwrap();
        let frame = session.execute("frame.add", &json!({"rect":[10,10,60,60],"content":"unassigned"})).unwrap()["id"].as_u64().unwrap();
        session.execute("file.place", &json!({"frame":frame,"name":"image.png","base64":png(shade)})).unwrap();
        (session, frame)
    };
    let (mut source, source_frame) = make(30);
    let (mut target, target_frame) = make(220);
    let (source_asset, target_asset) = (graphic_asset(&source, source_frame), graphic_asset(&target, target_frame));
    assert_eq!(source_asset, target_asset, "the fixture needs colliding asset ids");
    source.execute("select.set", &json!({"ids":[source_frame]})).unwrap();
    let snippet = source.execute("snippet.export", &json!({})).unwrap()["base64"].as_str().unwrap().to_owned();
    let placed = target.execute("snippet.place", &json!({"base64":snippet})).unwrap()["ids"][0].as_u64().unwrap();
    let doc = &target.doc().unwrap().doc;
    let source_doc = &source.doc().unwrap().doc;
    let bytes = |doc: &kition_layout_engine::doc::Document, asset| doc.assets.get(&asset).map(|asset| asset.data.clone());
    assert_eq!(bytes(doc, graphic_asset(&target, placed)), bytes(source_doc, source_asset), "the placed item shows the snippet's image");
    assert_eq!(bytes(doc, target_asset), bytes(&make(220).0.doc().unwrap().doc, target_asset), "the existing image is unchanged");
}

/// Deleting type on a path whose story continues in another frame keeps that frame's story
/// (cmd/path_type.rs removed the whole story).
#[test]
fn deleting_path_text_keeps_the_story_of_threaded_frames() {
    let mut session = Session::new();
    session.execute("file.new", &json!({"pages":1})).unwrap();
    let path = session.execute("path.add", &json!({"anchors":[{"p":[20,100]},{"p":[300,100]}]})).unwrap()["id"].as_u64().unwrap();
    session.execute("text.onPath", &json!({"id":path,"text":"Along the path and on into the next frame"})).unwrap();
    let next = session.execute("frame.add", &json!({"rect":[20,200,300,400],"content":"unassigned"})).unwrap()["id"].as_u64().unwrap();
    session.execute("frame.thread", &json!({"from":path,"to":next})).unwrap();
    session.execute("text.pathOptions", &json!({"id":path,"delete":true})).unwrap();
    let doc = &session.doc().unwrap().doc;
    let Content::Text(frame) = &doc.item(ItemId(next)).unwrap().content else { panic!("the threaded frame keeps its text") };
    let story = doc.story(frame.story).expect("the threaded frame's story survives");
    assert_eq!(story.frames, vec![ItemId(next)]);
    assert!(matches!(doc.item(ItemId(path)).unwrap().content, Content::Unassigned));
}

/// `transform.set` on an item of a spread without pages moves it from the spread's origin
/// (cmd/object.rs indexed the spread's pages: a panic the last-resort guard turned into an error).
#[test]
fn transform_set_on_a_spread_without_pages_does_not_panic() {
    let mut session = Session::new();
    session.execute("file.new", &json!({"pages":1})).unwrap();
    let id = session.execute("frame.add", &json!({"rect":[10,10,60,60],"content":"unassigned"})).unwrap()["id"].as_u64().unwrap();
    {
        let state = session.doc_mut().unwrap();
        let mut doc = (*state.doc).clone();
        std::sync::Arc::make_mut(&mut doc.spreads[0]).pages.clear();
        state.doc = std::sync::Arc::new(doc);
    }
    let before = session.doc().unwrap().doc.item(ItemId(id)).unwrap().bounds();
    session.execute("transform.set", &json!({"ids":[id],"x":50})).unwrap();
    let after = session.doc().unwrap().doc.item(ItemId(id)).unwrap().bounds();
    // `x` places the stroke's visible edge; the path follows it by half the stroke weight.
    assert!(after.x0 > before.x0 + 39.0 && (after.x0 - 50.0).abs() <= 0.5, "{before:?} -> {after:?}");
}

fn table_cell_session() -> (Session, u64, u64) {
    let mut session = Session::new();
    session.execute("file.new", &json!({"pages":1})).unwrap();
    let story = session.execute("frame.add", &json!({"rect":[20,20,400,400],"content":"text","text":"Body"})).unwrap()["story"].as_u64().unwrap();
    session.execute("text.select", &json!({"story":story,"anchor":0,"focus":0})).unwrap();
    let table = session.execute("table.insert", &json!({"rows":1,"cols":1})).unwrap()["table"].as_u64().unwrap();
    (session, story, table)
}

/// Deleting a paragraph style used in table-cell text gives that text the replacement, as it
/// does in story text (cmd/style.rs `delete_para` changed top-level paragraphs only).
#[test]
fn deleting_a_paragraph_style_replaces_it_in_cells() {
    let (mut session, story, table) = table_cell_session();
    for name in ["P", "Q"] {
        session.execute("style.paragraph.add", &json!({"name":name})).unwrap();
    }
    session.execute("table.select", &json!({"story":story,"table":table,"row":0,"col":0})).unwrap();
    session.execute("style.paragraph.apply", &json!({"name":"P"})).unwrap();
    session.execute("style.paragraph.delete", &json!({"name":"P","replaceWith":"Q"})).unwrap();
    let doc = &session.doc().unwrap().doc;
    let cell = &doc.story(kition_layout_engine::doc::StoryId(story)).unwrap().tables[&table].cells[0];
    assert!(cell.text.paras.iter().all(|para| para.style == "Q"), "{:?}", cell.text.paras.iter().map(|p| &p.style).collect::<Vec<_>>());
}

/// Renaming a paragraph style to a name another style has refuses instead of making two styles
/// of one name (cmd/style.rs `edit_para`).
#[test]
fn renaming_a_paragraph_style_to_an_existing_name_refuses() {
    let mut session = Session::new();
    session.execute("file.new", &json!({"pages":1})).unwrap();
    for name in ["P", "Q"] {
        session.execute("style.paragraph.add", &json!({"name":name})).unwrap();
    }
    assert!(session.execute("style.paragraph.edit", &json!({"name":"P","rename":"Q"})).is_err());
    let doc = &session.doc().unwrap().doc;
    assert_eq!(doc.styles.paragraph.iter().filter(|style| style.name == "Q").count(), 1);
    assert!(doc.styles.para("P").is_some());
}

/// `style.paragraph.edit` takes the `nextStyle` its parameters document (it was ignored).
#[test]
fn editing_a_paragraph_style_sets_its_next_style() {
    let mut session = Session::new();
    session.execute("file.new", &json!({"pages":1})).unwrap();
    for name in ["P", "Q"] {
        session.execute("style.paragraph.add", &json!({"name":name})).unwrap();
    }
    session.execute("style.paragraph.edit", &json!({"name":"P","nextStyle":"Q"})).unwrap();
    assert_eq!(session.doc().unwrap().doc.styles.para("P").unwrap().next_style.as_deref(), Some("Q"));
}

/// Replacing a font used only in an endnote's text changes that text too (cmd/fonts.rs fixed
/// story, cell and footnote text, not endnotes).
#[test]
fn replacing_a_font_reaches_endnote_text() {
    let mut session = Session::new();
    session.execute("file.new", &json!({"pages":1})).unwrap();
    let story = session.execute("frame.add", &json!({"rect":[20,20,400,400],"content":"text","text":"Body"})).unwrap()["story"].as_u64().unwrap();
    session.execute("text.select", &json!({"story":story,"anchor":2,"focus":2})).unwrap();
    session.execute("endnote.insert", &json!({"text":"Endnote text"})).unwrap();
    let sid = kition_layout_engine::doc::StoryId(story);
    {
        let state = session.doc_mut().unwrap();
        let mut doc = (*state.doc).clone();
        let note = std::sync::Arc::make_mut(doc.story_mut(sid).unwrap().endnotes.first_mut().expect("an endnote"));
        for run in &mut note.text.chars {
            run.format.over.font_family = Some("Only In Endnote".into());
        }
        state.doc = std::sync::Arc::new(doc);
    }
    session.execute("font.replace", &json!({"family":"Only In Endnote","toFamily":"Replacement"})).unwrap();
    let doc = &session.doc().unwrap().doc;
    let note = &doc.story(sid).unwrap().endnotes[0];
    assert!(note.text.chars.iter().all(|run| run.format.over.font_family.as_deref() == Some("Replacement")));
}

/// Placing an image into a text frame leaves the frame's story without that frame (cmd/file.rs
/// replaced the content and kept the story's backlink to a frame that no longer holds it).
#[test]
fn placing_an_image_into_a_text_frame_releases_its_story() {
    let mut session = Session::new();
    session.execute("file.new", &json!({"pages":1})).unwrap();
    let answer = session.execute("frame.add", &json!({"rect":[20,20,200,200],"content":"text","text":"Was text"})).unwrap();
    let (frame, story) = (answer["id"].as_u64().unwrap(), answer["story"].as_u64().unwrap());
    session.execute("file.place", &json!({"frame":frame,"name":"image.png","base64":png(90)})).unwrap();
    let doc = &session.doc().unwrap().doc;
    assert!(matches!(doc.item(ItemId(frame)).unwrap().content, Content::Graphic(_)));
    let sid = kition_layout_engine::doc::StoryId(story);
    assert!(doc.story(sid).is_none_or(|story| !story.frames.contains(&ItemId(frame))), "the story still names the frame");
}

/// Replacing a story's whole text removes its endnotes with their markers (doc/src/edit.rs
/// `set_story_text` cleared footnotes, not endnotes).
#[test]
fn replacing_a_story_text_drops_its_endnotes() {
    let mut session = Session::new();
    session.execute("file.new", &json!({"pages":1})).unwrap();
    let story = session.execute("frame.add", &json!({"rect":[20,20,400,400],"content":"text","text":"Body"})).unwrap()["story"].as_u64().unwrap();
    session.execute("text.select", &json!({"story":story,"anchor":2,"focus":2})).unwrap();
    session.execute("endnote.insert", &json!({"text":"Endnote text"})).unwrap();
    session.execute("story.setText", &json!({"story":story,"text":"Replaced"})).unwrap();
    let doc = &session.doc().unwrap().doc;
    assert!(doc.story(kition_layout_engine::doc::StoryId(story)).unwrap().endnotes.is_empty());
}

/// Deleting a variable turns its instances in table cells into text and renumbers later
/// variables' instances there (cmd/variables.rs changed story text only, so a cell's instance
/// of the deleted variable then named the next one).
#[test]
fn deleting_a_variable_reaches_instances_in_cells() {
    let (mut session, story, table) = table_cell_session();
    session.execute("variable.define", &json!({"name":"Gone","type":"custom","text":"Gone value"})).unwrap();
    session.execute("variable.define", &json!({"name":"Kept","type":"custom","text":"Kept value"})).unwrap();
    session.execute("table.select", &json!({"story":story,"table":table,"row":0,"col":0})).unwrap();
    session.execute("variable.insert", &json!({"name":"Gone"})).unwrap();
    session.execute("variable.insert", &json!({"name":"Kept"})).unwrap();
    session.execute("variable.delete", &json!({"name":"Gone"})).unwrap();
    let doc = &session.doc().unwrap().doc;
    let kept = doc.variable("Kept").unwrap();
    let cell = &doc.story(kition_layout_engine::doc::StoryId(story)).unwrap().tables[&table].cells[0].text.text;
    let indexes: Vec<usize> = cell.chars().filter_map(kition_layout_engine::doc::vars::var_index).collect();
    assert_eq!(indexes, vec![kept], "{cell:?}");
    assert!(cell.contains("Gone value"), "{cell:?}");
}

/// Copying text with an endnote reference and pasting it keeps the endnote's text (doc/src/slice.rs
/// carried footnotes, not endnotes: the pasted reference got an empty endnote).
#[test]
fn pasting_text_with_an_endnote_keeps_the_endnote_text() {
    let mut session = Session::new();
    session.execute("file.new", &json!({"pages":1})).unwrap();
    let story =
        session.execute("frame.add", &json!({"rect":[20,20,400,400],"content":"text","text":"Body text"})).unwrap()["story"].as_u64().unwrap();
    session.execute("text.select", &json!({"story":story,"anchor":4,"focus":4})).unwrap();
    session.execute("endnote.insert", &json!({"text":"Endnote text"})).unwrap();
    let sid = kition_layout_engine::doc::StoryId(story);
    let len = session.doc().unwrap().doc.story(sid).unwrap().text.trim_end_matches('\n').len();
    session.execute("text.select", &json!({"story":story,"anchor":0,"focus":len})).unwrap();
    session.execute("edit.copy", &json!({})).unwrap();
    session.execute("text.select", &json!({"story":story,"anchor":len,"focus":len})).unwrap();
    session.execute("edit.paste", &json!({})).unwrap();
    let doc = &session.doc().unwrap().doc;
    let notes = &doc.story(sid).unwrap().endnotes;
    assert_eq!(notes.len(), 2);
    assert!(notes.iter().all(|note| note.text.text.starts_with("Endnote text")), "{:?}", notes.iter().map(|n| &n.text.text).collect::<Vec<_>>());
}
