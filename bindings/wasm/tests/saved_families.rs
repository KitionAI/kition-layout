//! C8-layout review: each write family crosses every conformance file boundary.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::session::model::Exchange;
use kition_kit::{KitDocument, document_content};
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

fn fixtures() -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "klayout"))
        .collect();
    paths.sort();
    paths
}

#[test]
fn clipboard_paste_into_text_allocates_fresh_item_ids_after_reopen() {
    let mut doc = LayoutDocument::create("Paste.klayout", 612, 792).unwrap();
    let item = doc.execute("frame.create", json!({"rect":[20,20,100,100]})).unwrap()["id"].clone();
    doc.execute("selection.set", json!({"ids":[item]})).unwrap();
    doc.execute("edit.copy", json!({})).unwrap();
    let text = doc.execute("frame.create", json!({"rect":[20,200,400,600],"content":"text","text":"Body"})).unwrap();
    doc.execute("text.select", json!({"story":text["story"],"anchor":0})).unwrap();
    let state = doc.editor_state();
    let bytes = doc.seal_and_save(None).unwrap();
    let mut colleague = LayoutDocument::open("Paste.klayout", &bytes).unwrap();
    colleague.execute("object.opacity", json!({"ids":[item],"opacity":0.5})).unwrap();
    let mut reopened = LayoutDocument::open("Paste.klayout", &colleague.seal_and_save(None).unwrap()).unwrap();
    reopened.restore_editor_state(&state).unwrap();
    reopened.execute("edit.paste", json!({})).unwrap();
    reopened.execute("edit.paste", json!({})).unwrap();
    let live = document_content(reopened.inspect().unwrap(), &["/selection", "/tool", "/activeLayer"]);
    let saved = LayoutDocument::open("Paste.klayout", &reopened.seal_and_save(None).unwrap()).unwrap();
    assert_eq!(live, document_content(saved.inspect().unwrap(), &["/selection", "/tool", "/activeLayer"]));
}

fn writes(doc: &mut LayoutDocument) -> Vec<(&'static str, Value)> {
    let a = doc.execute("frame.create", json!({"rect":[20,20,200,180],"content":"text","text":"Guard review text"})).unwrap();
    let b = doc.execute("frame.create", json!({"rect":[220,20,400,180],"content":"unassigned"})).unwrap();
    let layer = doc.execute("layer.new", json!({"name":"Saved review"})).unwrap()["id"].clone();
    doc.execute("style.paragraph.create", json!({"name":"Saved para"})).unwrap();
    doc.execute("style.paragraph.create", json!({"name":"Saved applied para","chars":{"size":19}})).unwrap();
    doc.execute("style.character.create", json!({"name":"Saved char"})).unwrap();
    doc.execute("style.object.create", json!({"name":"Saved object","fromSelection":false,"fill":"Black"})).unwrap();
    doc.execute("style.cell.create", json!({"name":"Saved cell","fill":"Black"})).unwrap();
    doc.execute("style.table.create", json!({"name":"Saved table","body":"Saved cell"})).unwrap();
    doc.execute("condition.new", json!({"name":"Saved condition"})).unwrap();
    doc.execute("bookmark.add", json!({"name":"Saved bookmark"})).unwrap();
    let table_frame = doc.execute("frame.create", json!({"rect":[20,420,400,700],"content":"text","text":""})).unwrap();
    doc.execute("text.select", json!({"story":table_frame["story"],"anchor":0})).unwrap();
    let table = doc.execute("table.insert", json!({"rows":3,"cols":3})).unwrap();
    doc.execute("table.setCell", json!({"story":table_frame["story"],"table":table["table"],"row":0,"col":0,"text":"Distinct first cell"})).unwrap();
    let table_write = |command, mut params: Value| {
        params["story"] = table_frame["story"].clone();
        params["table"] = table["table"].clone();
        (command, params)
    };
    let guide = doc.execute("guide.add", json!({"orientation":"horizontal","position":27,"page":0})).unwrap();
    let line = doc.execute("line.create", json!({"a":[30,300],"b":[300,300]})).unwrap();
    let path_text = doc.execute("line.create", json!({"a":[30,330],"b":[300,330]})).unwrap();
    doc.execute("type.onPath", json!({"id":path_text["id"],"text":"Saved path text"})).unwrap();
    let c = doc.execute("frame.create", json!({"rect":[420,20,480,80]})).unwrap();
    let d = doc.execute("frame.create", json!({"rect":[480,20,540,80]})).unwrap();
    let group = doc.execute("states.create", json!({"ids":[c["id"],d["id"]]})).unwrap();
    let media = doc.execute("file.place", json!({"base64":"AAAAGGZ0eXBtcDQy","name":"Saved.mp4","x":420,"y":220})).unwrap();
    doc.execute("selection.set", json!({"ids":[b["id"]]})).unwrap();
    doc.execute("edit.copy", json!({})).unwrap();
    doc.execute("conveyor.collect", json!({"ids":[b["id"]]})).unwrap();
    doc.execute("library.new", json!({})).unwrap();
    doc.execute("library.add", json!({"name":"Saved library"})).unwrap();
    doc.execute("variables.define", json!({"name":"Saved variable","text":"Before"})).unwrap();
    doc.execute("list.define", json!({"name":"Saved list"})).unwrap();
    doc.execute("strokeStyle.new", json!({"name":"Saved stroke","type":"dot"})).unwrap();
    doc.execute("text.select", json!({"story":a["story"],"anchor":17})).unwrap();
    let note = doc.execute("note.new", json!({"text":"Before note"})).unwrap();
    let footnote = doc.execute("footnote.insert", json!({"text":"Before footnote"})).unwrap();
    doc.execute("text.select", json!({"story":a["story"],"anchor":17})).unwrap();
    let endnote = doc.execute("endnote.insert", json!({"text":"Before endnote"})).unwrap();
    doc.execute("text.select", json!({"story":a["story"],"anchor":0,"focus":4})).unwrap();
    doc.execute("style.paragraph.apply", json!({"name":"Saved para"})).unwrap();
    doc.execute("index.addReference", json!({"topic":"Saved topic"})).unwrap();
    let anchor = doc.execute("anchor.create", json!({"name":"Saved anchor"})).unwrap();
    doc.execute("toc.generate", json!({"styles":["Saved para"],"title":"Before contents"})).unwrap();
    doc.execute("index.generate", json!({"title":"Before index"})).unwrap();
    let linked = doc.execute("story.placeAndLink", json!({"story":a["story"],"rect":[20,220,300,400]})).unwrap();
    doc.execute("story.replaceRange", json!({"story":a["story"],"start":0,"end":0,"text":"X"})).unwrap();
    doc.execute("data.source.select", json!({"name":"Saved.csv","base64":"bmFtZQpBZGEK"})).unwrap();
    doc.execute("text.select", json!({"story":a["story"],"anchor":0})).unwrap();
    let placeholder = doc.execute("data.placeholder.add", json!({"field":"name"})).unwrap();
    doc.execute("style.paragraph.create", json!({"name":"Saved missing font","chars":{"fontFamily":"ReviewMissing"}})).unwrap();
    doc.execute("text.select", json!({"story":a["story"],"anchor":0,"focus":4})).unwrap();
    let mut scenarios = vec![
        ("object.rename", json!({"id":a["id"],"name":"Saved name"})),
        ("object.opacity", json!({"ids":[a["id"]],"opacity":0.123456789})),
        ("object.stroke", json!({"ids":[a["id"]],"weight":1.23456789})),
        ("transform.move", json!({"ids":[a["id"]],"dx":0.123456789,"dy":-2.3456789})),
        ("object.group", json!({"ids":[a["id"],b["id"]]})),
        ("edit.clear", json!({"ids":[b["id"]]})),
        ("transform.move", json!({"ids":[a["id"]],"dx":10,"copy":true})),
        ("edit.stepAndRepeat", json!({"ids":[a["id"]],"count":2,"dx":15})),
        ("story.replaceRange", json!({"story":a["story"],"start":1,"end":3,"text":"Saved"})),
        ("type.char", json!({"attrs":{"size":18.123456789}})),
        ("type.para", json!({"attrs":{"leftIndent":1.23456789}})),
        ("style.paragraph.apply", json!({"name":"Saved applied para"})),
        ("style.character.apply", json!({"name":"Saved char"})),
        ("style.object.apply", json!({"ids":[a["id"]],"name":"Saved object"})),
        ("style.paragraph.edit", json!({"name":"Saved para","chars":{"size":20}})),
        ("style.character.edit", json!({"name":"Saved char","chars":{"size":20}})),
        ("style.cell.edit", json!({"name":"Saved cell","tint":0.42})),
        ("style.table.edit", json!({"name":"Saved table","spaceBefore":4})),
        ("swatch.create", json!({"name":"Saved colour","color":"#123456"})),
        ("object.gradient", json!({"ids":[b["id"]],"angle":31})),
        ("layer.set", json!({"id":layer,"visible":false})),
        ("layer.move", json!({"id":layer,"to":1})),
        ("layout.pages.insert", json!({"count":1})),
        ("layout.pages.duplicateSpread", json!({"spread":0})),
        ("layout.pageSize", json!({"pages":[1],"width":500})),
        ("layout.pages.applyParent", json!({"pages":[1],"parent":null})),
        ("guide.add", json!({"orientation":"horizontal","position":25})),
        ("bookmark.rename", json!({"index":0,"name":"Saved changed"})),
        ("hyperlink.create", json!({"ids":[b["id"]],"url":"https://saved.example"})),
        ("article.new", json!({"ids":[b["id"]],"name":"Saved article"})),
        ("condition.apply", json!({"name":"Saved condition"})),
        ("variables.define", json!({"name":"Saved variable","type":"custom","text":"Saved"})),
        ("note.new", json!({"text":"Saved note"})),
        ("footnote.insert", json!({"text":"Saved footnote"})),
        ("endnote.insert", json!({"text":"Saved endnote"})),
        ("table.insert", json!({"rows":2,"cols":2})),
        table_write("table.setCell", json!({"row":1,"col":1,"text":"Saved cell text"})),
        table_write("table.setRowHeight", json!({"row":1,"height":32})),
        table_write("table.setColumnWidth", json!({"col":1,"width":70})),
        table_write("table.merge", json!({"rows":[0,1],"cols":[0,1]})),
        table_write("table.insertRow", json!({"row":1})),
        table_write("table.insertColumn", json!({"col":1})),
        table_write("table.deleteRow", json!({"row":1})),
        table_write("table.deleteColumn", json!({"col":1})),
        table_write("table.convertToText", json!({})),
        table_write("style.cell.apply", json!({"name":"Saved cell","row":1,"col":1})),
        table_write("style.table.apply", json!({"name":"Saved table"})),
        ("story.placeAndLink", json!({"story":a["story"],"rect":[20,220,200,400]})),
        ("xml.tag", json!({"ids":[b["id"]],"tag":"SavedTag"})),
        ("button.set", json!({"ids":[b["id"]],"action":"nextPage"})),
        ("object.caption", json!({"ids":[b["id"]],"text":"Saved caption"})),
        ("strokeStyle.new", json!({"name":"Saved new stroke","type":"dot"})),
        ("list.define", json!({"name":"Saved new list"})),
        ("footnote.options", json!({"prefix":"Saved prefix"})),
        ("endnote.options", json!({"prefix":"Saved prefix"})),
        ("object.convertShape", json!({"ids":[b["id"]],"to":"ellipse"})),
        (
            "file.place",
            json!({"base64":"PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyIiBoZWlnaHQ9IjIiPjxyZWN0IHdpZHRoPSIyIiBoZWlnaHQ9IjIiIGZpbGw9InJlZCIvPjwvc3ZnPg==","name":"Saved.svg","x":400,"y":200}),
        ),
    ];
    scenarios.extend([
        ("transform.resize", json!({"ids":[b["id"]],"from":[220,20,400,180],"to":[220,20,420,200]})),
        ("transform.rotate", json!({"ids":[b["id"]],"angle":23})),
        ("transform.scale", json!({"ids":[b["id"]],"sx":1.2,"sy":0.8})),
        ("transform.shear", json!({"ids":[b["id"]],"angle":12})),
        ("object.align", json!({"ids":[a["id"],b["id"]],"edge":"left"})),
        ("object.ungroup", json!({"ids":[group["id"]]})),
        ("object.textFrameOptions", json!({"ids":[a["id"]],"columns":2})),
        ("object.textWrap", json!({"ids":[b["id"]],"mode":"bounds","offset":5})),
        ("object.dropShadow", json!({"ids":[b["id"]],"on":true,"opacity":0.6})),
        ("object.cornerOptions", json!({"ids":[b["id"]],"size":12})),
        ("object.qrCode", json!({"content":"https://saved.example","rect":[420,420,500,500]})),
        ("form.set", json!({"ids":[b["id"]],"kind":"textField","name":"Saved field","value":"Saved"})),
        ("states.show", json!({"id":group["id"],"index":1})),
        ("states.rename", json!({"id":group["id"],"index":0,"name":"Saved state"})),
        ("states.release", json!({"id":group["id"]})),
        ("media.options", json!({"ids":[media["id"]],"loop":true})),
        ("math.insert", json!({"latex":"x^2"})),
        ("path.addAnchor", json!({"id":line["id"],"at":[165,300]})),
        ("path.convertAnchor", json!({"id":line["id"],"anchor":0,"to":[50,320]})),
        ("type.pathOptions", json!({"id":path_text["id"],"flip":true})),
        ("type.pathOptions", json!({"id":path_text["id"],"delete":true})),
        ("type.createOutlines", json!({"ids":[path_text["id"]]})),
        ("edit.paste", json!({"text":"Saved paste"})),
        ("edit.pasteWithoutFormatting", json!({"text":"Saved plain paste"})),
        ("edit.pasteInto", json!({"id":b["id"]})),
        ("library.place", json!({"index":0,"x":400,"y":400})),
        ("conveyor.place", json!({"index":0,"x":400,"y":500})),
        ("note.edit", json!({"story":a["story"],"id":note["id"],"text":"Saved note edit"})),
        ("note.delete", json!({"story":a["story"],"id":note["id"]})),
        ("note.convertToText", json!({"story":a["story"],"id":note["id"]})),
        ("footnote.setText", json!({"story":a["story"],"id":footnote["id"],"text":"Saved footnote edit"})),
        ("footnote.delete", json!({"story":a["story"],"id":footnote["id"]})),
        ("endnote.edit", json!({"story":a["story"],"id":endnote["id"],"text":"Saved endnote edit"})),
        ("endnote.delete", json!({"story":a["story"],"id":endnote["id"]})),
        ("index.addReference", json!({"topic":"Saved extra topic"})),
        ("index.generate", json!({"title":"Saved index"})),
        ("toc.generate", json!({"styles":["Saved para"],"title":"Saved contents"})),
        ("xref.insert", json!({"anchor":anchor["id"]})),
        ("xref.defineFormat", json!({"name":"Saved format","definition":"<pageNum />"})),
        ("anchor.create", json!({"name":"Saved new anchor"})),
        ("story.updateLink", json!({"story":linked["story"]})),
        ("story.unlink", json!({"story":linked["story"]})),
        ("anchored.insert", json!({"ids":[b["id"]],"story":a["story"],"pos":0})),
        ("variables.insert", json!({"name":"Saved variable"})),
        ("condition.options", json!({"name":"Saved condition","visible":false})),
        ("condition.delete", json!({"name":"Saved condition"})),
        ("variables.delete", json!({"name":"Saved variable"})),
        ("spelling.setWords", json!({"words":["savedword"]})),
        ("hyphenation.addException", json!({"word":"sav~edword"})),
        ("spelling.change", json!({"story":a["story"],"start":0,"end":0,"to":"Saved"})),
        ("find.change", json!({"story":a["story"],"find":"review","change":"saved"})),
        ("font.replace", json!({"family":"ReviewMissing","toFamily":"Inter"})),
        ("guide.move", json!({"page":guide["page"],"index":guide["index"],"position":45})),
        ("guide.delete", json!({"page":guide["page"],"index":guide["index"]})),
        ("guide.liquid", json!({"page":guide["page"],"index":guide["index"],"on":true})),
        ("layout.createGuides", json!({"rows":2,"columns":2})),
        ("page.background", json!({"pages":[1],"color":"#123456"})),
        ("page.transition", json!({"pages":[1],"kind":"fade"})),
        ("liquid.pageRule", json!({"pages":[1],"rule":"scale"})),
        ("liquid.object", json!({"ids":[b["id"]],"pinLeft":true})),
        ("layout.createAlternate", json!({"name":"Saved alternate","width":500,"height":700})),
        ("document.preferences", json!({"keyboardIncrement":2.3456789})),
        ("edit.transparencyBlendSpace", json!({"space":"rgb"})),
        ("document.applyTemplate", json!({"template":"event"})),
        ("style.group", json!({"kind":"paragraph","names":["Saved para"],"group":"Saved group"})),
        ("style.exportTag", json!({"style":"Saved para","tag":"h2"})),
        ("xml.newTag", json!({"name":"SavedNewTag"})),
        ("xml.tagText", json!({"tag":"SavedTextTag"})),
        ("xml.mapStyle", json!({"style":"Saved para","tag":"SavedStyleTag"})),
        ("data.options", json!({"perPage":"multiple","columnSpacing":17})),
        ("data.placeholder.remove", json!({"id":placeholder["id"]})),
        ("data.source.remove", json!({})),
        table_write("table.unmerge", json!({"row":0,"col":0})),
        table_write("table.splitHorizontally", json!({"row":0,"col":0})),
        table_write("table.splitVertically", json!({"row":0,"col":0})),
        table_write("table.moveRow", json!({"from":0,"to":2})),
        table_write("table.moveColumn", json!({"from":0,"to":2})),
        table_write("table.options", json!({"repeatHeader":false})),
        table_write("table.setCell", json!({"row":0,"col":0,"stroke":{"weight":3,"color":"Black"}})),
    ]);
    scenarios
}

#[test]
fn every_conformance_fixture_write_family_survives_native_save() {
    let mut tested = 0;
    let mut failures = Vec::new();
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        for imported in [false, true] {
            let mut source = LayoutDocument::open("Review.klayout", &bytes).unwrap();
            if imported {
                source = LayoutDocument::open("Review.idml", &source.save(Some("Review.idml")).unwrap()).unwrap();
            }
            let scenarios = writes(&mut source);
            let state = source.editor_state();
            let prepared = source.seal_and_save(None).unwrap();
            for (command, params) in scenarios {
                let context = format!("{} imported={imported} {command} {params}", path.display());
                let mut doc = LayoutDocument::open("Review.klayout", &prepared).unwrap();
                doc.restore_editor_state(&state).unwrap();
                if command.starts_with("states.") {
                    doc.execute("selection.set", json!({"ids":[params["id"]]})).unwrap();
                }
                if command == "table.unmerge" {
                    doc.execute("table.merge", json!({"story":params["story"],"table":params["table"],"rows":[0,1],"cols":[0,1]})).unwrap();
                }
                let mut exchange = Exchange::new();
                let before = exchange.replica(&mut doc).unwrap().state_json().unwrap();
                let receipt = match exchange.batch(&mut doc, &json!({"ops":[{"op":"command","id":command,"params":params}]}), false) {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        failures.push(format!("{context}: {error}"));
                        continue;
                    }
                };
                let live = document_content(doc.inspect().unwrap(), &["/selection", "/tool", "/activeLayer"]);
                let mut reopened = LayoutDocument::open("Review.klayout", &doc.seal_and_save(None).unwrap()).unwrap();
                assert_eq!(live, document_content(reopened.inspect().unwrap(), &["/selection", "/tool", "/activeLayer"]), "{context}");
                let after = exchange.replica(&mut doc).unwrap().state_json().unwrap();
                let disk = Exchange::new().replica(&mut reopened).unwrap().state_json().unwrap();
                let schema = kition_layout_kdm::schema();
                let memory = kition_kit::kdm::reread::diff_paired(&schema, &before, &after).to_json();
                let persisted = kition_kit::kdm::reread::diff_paired(&schema, &before, &disk).to_json();
                assert_eq!(memory, persisted, "{context}: receipt source differs from saved diff: {receipt}");
                if !["changed", "created", "deleted", "moved"].iter().any(|key| persisted[key].as_array().is_some_and(|v| !v.is_empty())) {
                    failures.push(format!("{context}: no write"));
                }
                tested += 1;
            }
            println!("SAVEDFAMILIES {} imported={imported}: {tested} writes checked", path.display());
            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }
    }
    println!("SAVEDFAMILIES {tested} real writes: native fixtures and IDML imports saved as native");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
