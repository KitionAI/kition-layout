//! C8-layout-read: named settings chosen from the independent handler reading.
use super::*;
use kition_kit::session::model::Exchange;

fn prepare(f: &Fixture, steps: &[(&str, Value)]) -> Vec<Value> {
    let mut doc = f.disk();
    let results = steps.iter().map(|(command, params)| doc.execute(command, params.clone()).unwrap()).collect();
    std::fs::write(f.root.join(FILE), doc.save(None).unwrap()).unwrap();
    results
}

fn setting(f: &Fixture, name: &str) -> Value {
    let mut doc = f.disk();
    let root = Exchange::new().batch(&mut doc, &json!({"ops":[],"load":[{"target":"layout","fields":["settings"]}]}), true).unwrap();
    root["loaded"][0]["fields"]["settings"][name].clone()
}

fn colleague_setting(f: &Fixture, name: &str, value: Value) {
    colleague_batch(f, json!([{"op":"map.set","target":"layout","field":"settings","key":name,"value":value}]));
}

/// The object of the cross-reference format named `name`.
fn xref_format(f: &Fixture, name: &str) -> Value {
    let mut doc = f.disk();
    let mut exchange = Exchange::new();
    let root = exchange.batch(&mut doc, &json!({"ops":[],"load":[{"target":"layout","fields":["xrefFormats"]}]}), true).unwrap();
    let loads: Vec<Value> =
        root["loaded"][0]["fields"]["xrefFormats"].as_array().unwrap().iter().map(|id| json!({"target":id,"fields":["name"]})).collect();
    let formats = exchange.batch(&mut doc, &json!({"ops":[],"load":loads}), true).unwrap();
    formats["loaded"].as_array().unwrap().iter().find(|format| format["fields"]["name"] == name).unwrap()["id"].clone()
}

/// A colleague renames a cross-reference format through the object model.
fn colleague_rename_format(f: &Fixture, format: Value, name: &str) {
    colleague_batch(f, json!([{"op":"set","target":format,"field":"name","value":name}]));
}

/// A colleague's object-model edit `ops`, saved to the file as an older change.
fn colleague_batch(f: &Fixture, ops: Value) {
    let path = f.root.join(FILE);
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let mut doc = f.disk();
    let before = doc.save(None).unwrap();
    Exchange::new().batch(&mut doc, &json!({ "ops": ops }), false).unwrap();
    let saved = doc.save(None).unwrap();
    assert_ne!(before, saved, "the independent writer must really change the file");
    std::fs::write(&path, saved).unwrap();
    std::fs::File::options().write(true).open(path).unwrap().set_modified(modified - std::time::Duration::from_secs(3600)).unwrap();
}

fn refused(answer: &Result<Value, Value>, needed: bool, context: &str) {
    if needed {
        assert_eq!(answer.as_ref().err().map(|error| &error["error"]["code"]), Some(&json!("conflict")), "{context}: {answer:?}");
    } else {
        assert!(answer.is_ok(), "{context}: {answer:?}");
    }
}

#[tokio::test]
async fn independent_settings_xref_definition_holds_only_its_named_record() {
    for needed in [false, true] {
        let f = Fixture::new(Guard::Premise);
        prepare(
            &f,
            &[
                ("xref.defineFormat", json!({"name":"Read","definition":"Original"})),
                ("xref.defineFormat", json!({"name":"Other","definition":"Other"})),
            ],
        );
        let basis = f.basis().await;
        f.colleague(&[("xref.defineFormat", json!({"name":if needed {"Read"} else {"Other"},"definition":"Colleague"}))]);
        let answer = f.write(basis, "xref.defineFormat", json!({"name":"Read","definition":"Mine"})).await;
        refused(&answer, needed, "xref.defineFormat");
        let records = f.disk().execute("xref.formats", json!({})).unwrap();
        let own = records.as_array().unwrap().iter().find(|record| record["name"] == "Read").unwrap();
        assert_eq!(own["definition"], if needed { "Colleague" } else { "Mine" });
        f.close().await;
    }
}

#[tokio::test]
async fn independent_settings_composite_font_holds_only_its_named_record() {
    for needed in [false, true] {
        let f = Fixture::new(Guard::Premise);
        prepare(
            &f,
            &[
                ("style.compositeFont.set", json!({"name":"Read","entries":[{"family":"Inter"}]})),
                ("style.compositeFont.set", json!({"name":"Other","entries":[{"family":"Inter"}]})),
            ],
        );
        let basis = f.basis().await;
        f.colleague(&[(
            "style.compositeFont.set",
            json!({"name":if needed {"Read"} else {"Other"},"entries":[{"family":"Inter","relativeSize":1.25}]}),
        )]);
        let answer = f.write(basis, "style.compositeFont.set", json!({"name":"Read","entries":[{"family":"Inter","relativeSize":1.5}]})).await;
        refused(&answer, needed, "style.compositeFont.set");
        let records = f.disk().execute("style.compositeFont.list", json!({})).unwrap();
        let own = records.as_array().unwrap().iter().find(|record| record["name"] == "Read").unwrap();
        assert_eq!(own["entries"][0]["relativeSize"], if needed { 1.25 } else { 1.5 });
        f.close().await;
    }
}

async fn xref_format_reference(command: &str, default: bool) {
    for change in ["definition", "other-record", "renamed"] {
        let f = Fixture::new(Guard::Premise);
        let made = prepare(
            &f,
            &[
                ("xref.defineFormat", json!({"name":"Read","definition":"Original"})),
                ("xref.defineFormat", json!({"name":"Other","definition":"Other"})),
                ("text.select", json!({"story":f.story,"anchor":0})),
                ("anchor.create", json!({"name":"Destination"})),
            ],
        );
        let anchor = made[3]["id"].clone();
        prepare(&f, &[("text.select", json!({"story":f.story,"anchor":4})), ("xref.insert", json!({"anchor":anchor,"format":"Other"}))]);
        let format = if default { "Full Paragraph & Page Number" } else { "Read" };
        let basis = f.basis().await;
        if change == "renamed" {
            let object = xref_format(&f, format);
            colleague_rename_format(&f, object, "Gone");
        } else {
            f.colleague(&[("xref.defineFormat", json!({"name":if change == "definition" {format} else {"Other"},"definition":"Colleague"}))]);
        }
        let mut params = if command == "xref.insert" { json!({"anchor":anchor}) } else { json!({"story":f.story,"index":0}) };
        if !default {
            params["format"] = json!(format);
        }
        let answer = f
            .call(
                "run",
                json!({"basis":basis,"steps":[
                    {"command":"text.select","params":{"story":f.story,"anchor":8}},
                    {"command":command,"params":params}
                ]}),
            )
            .await;
        refused(&answer, change == "renamed", &format!("{command} default={default} {change}"));
        let refs = f.disk().execute("xref.list", json!({})).unwrap();
        assert_eq!(refs.as_array().unwrap().len(), if command == "xref.insert" && change != "renamed" { 2 } else { 1 });
        if command == "xref.setFormat" && change != "renamed" {
            assert_eq!(refs[0]["format"], format);
        }
        f.close().await;
    }
}

#[tokio::test]
async fn independent_settings_xref_insert_holds_format_existence() {
    xref_format_reference("xref.insert", false).await;
}

#[tokio::test]
async fn independent_settings_xref_insert_holds_default_format_existence() {
    xref_format_reference("xref.insert", true).await;
}

#[tokio::test]
async fn independent_settings_xref_set_format_holds_format_existence() {
    xref_format_reference("xref.setFormat", false).await;
}

#[tokio::test]
async fn independent_settings_ink_global_option_holds_only_the_written_property() {
    for needed in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        if needed {
            f.colleague(&[("ink.options", json!({"allToProcess":true}))]);
        } else {
            colleague_setting(&f, "inks", json!({"allToProcess":false,"toProcess":["Unrelated"],"aliases":[]}));
        }
        let answer = f.write(basis, "ink.options", json!({"allToProcess":true})).await;
        refused(&answer, needed, "ink.options allToProcess");
        assert_eq!(setting(&f, "inks")["allToProcess"], true);
        if !needed {
            assert_eq!(setting(&f, "inks")["toProcess"], json!(["Unrelated"]));
        }
        f.close().await;
    }
}

#[tokio::test]
async fn independent_settings_ungroup_holds_only_the_named_group_and_its_members() {
    for needed in [false, true] {
        let f = Fixture::new(Guard::Premise);
        prepare(
            &f,
            &[
                ("swatch.create", json!({"name":"Red","color":[255,0,0]})),
                ("swatch.newColorGroup", json!({"name":"Read"})),
                ("swatch.newColorGroup", json!({"name":"Other"})),
            ],
        );
        let basis = f.basis().await;
        f.colleague(&[("swatch.moveToGroup", json!({"group":if needed {"Read"} else {"Other"},"swatches":["Red"]}))]);
        let answer = f.write(basis, "swatch.ungroupColorGroup", json!({"name":"Read"})).await;
        refused(&answer, needed, "swatch.ungroupColorGroup");
        assert_eq!(setting(&f, "colorGroups").as_array().unwrap().iter().any(|group| group["name"] == "Read"), needed);
        f.close().await;
    }
}

#[tokio::test]
async fn independent_settings_group_rename_holds_names_but_preserves_new_members() {
    for change in ["members", "other-name", "source-name", "destination-name"] {
        let f = Fixture::new(Guard::Premise);
        prepare(
            &f,
            &[
                ("swatch.create", json!({"name":"Red","color":[255,0,0]})),
                ("swatch.newColorGroup", json!({"name":"Read"})),
                ("swatch.newColorGroup", json!({"name":"Other"})),
            ],
        );
        let basis = f.basis().await;
        match change {
            "members" => f.colleague(&[("swatch.moveToGroup", json!({"group":"Read","swatches":["Red"]}))]),
            "other-name" => f.colleague(&[("swatch.renameColorGroup", json!({"name":"Other","to":"Elsewhere"}))]),
            "source-name" => f.colleague(&[("swatch.renameColorGroup", json!({"name":"Read","to":"Gone"}))]),
            _ => f.colleague(&[("swatch.renameColorGroup", json!({"name":"Other","to":"Mine"}))]),
        }
        let answer = f.write(basis, "swatch.renameColorGroup", json!({"name":"Read","to":"Mine"})).await;
        refused(&answer, matches!(change, "source-name" | "destination-name"), &format!("swatch.renameColorGroup {change}"));
        let groups = setting(&f, "colorGroups");
        if change == "members" {
            assert_eq!(groups.as_array().unwrap().iter().find(|group| group["name"] == "Mine").unwrap()["swatches"].as_array().unwrap().len(), 1);
        }
        f.close().await;
    }
}
