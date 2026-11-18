//! C8-land-13-layout: declarations are checked against the engine's resolvers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use kition_layout_kit::LayoutDocument;
use serde_json::Value;

#[test]
fn every_addressable_noun_has_a_stable_tool_address() {
    let schema = kition_layout_kdm::schema();
    for (name, definition) in &schema.types {
        if name != "Layout" {
            assert!(definition.tool_id.is_some(), "{name} has no stable tool address");
        }
    }
}

#[test]
fn environment_schema_names_objects_and_bounds_opacity() {
    let schema: Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
    assert_eq!(schema["types"]["Item"]["label"], "name");
    assert_eq!(schema["types"]["Style"]["label"], "name");
    assert_eq!(schema["types"]["Item"]["fields"]["opacity"]["minimum"], 0);
    assert_eq!(schema["types"]["Item"]["fields"]["opacity"]["maximum"], 1);
    assert_eq!(schema["types"]["Item"]["fields"]["key"]["sample"], false);
    assert_eq!(schema["types"]["Item"]["fields"]["number"]["readOnly"], true);
}

#[test]
fn concurrent_creations_share_one_id_space_across_types() {
    let bytes = LayoutDocument::create("Ids.klayout", 612, 792).unwrap().save(None).unwrap();
    let mut a = LayoutDocument::open("Ids.klayout", &bytes).unwrap();
    let mut b = LayoutDocument::open("Ids.klayout", &bytes).unwrap();
    let item = a.execute("frame.create", serde_json::json!({"rect":[20,20,60,60],"content":"unassigned"})).unwrap()["id"].clone();
    let layer = b.execute("layer.new", serde_json::json!({"name":"Colleague"})).unwrap()["id"].clone();
    assert_eq!(item, layer, "the independent writers allocate the same next number");
    a.import_changes(&b.export_changes(&[]).unwrap()).unwrap();
    let inspected = a.inspect().unwrap();
    let item = &inspected["spreads"][0]["items"][0]["id"];
    assert!(
        inspected["layers"].as_array().unwrap().iter().all(|layer| layer["id"] != *item),
        "one id cannot name both an item and a layer: {inspected}"
    );
}

#[test]
fn model_addresses_are_command_addresses() {
    let mut doc = LayoutDocument::create("Addresses.klayout", 612, 792).unwrap();
    let item = doc.execute("frame.create", serde_json::json!({"rect":[20,20,60,60],"content":"unassigned"})).unwrap()["id"].clone();
    let mut exchange = Exchange::new();
    let root = exchange.batch(&mut doc, &serde_json::json!({"ops":[],"load":[{"target":"layout","fields":["swatches"]}]}), true).unwrap();
    let loads: Vec<Value> =
        root["loaded"][0]["fields"]["swatches"].as_array().unwrap().iter().map(|id| serde_json::json!({"target":id,"fields":["name"]})).collect();
    let swatches = exchange.batch(&mut doc, &serde_json::json!({"ops":[],"load":loads}), true).unwrap();
    let black = swatches["loaded"].as_array().unwrap().iter().find(|swatch| swatch["fields"]["name"] == "[Black]").unwrap()["id"].clone();
    doc.execute("object.fill", serde_json::json!({"ids":[item.to_string()],"swatch":black})).unwrap();
    assert_eq!(doc.inspect().unwrap()["spreads"][0]["items"][0]["fill"], "[Black]", "a queried swatch address must name that swatch to a command");
}

#[test]
fn character_export_tagging_resolves_the_character_family() {
    let mut doc = LayoutDocument::create("Styles.klayout", 612, 792).unwrap();
    doc.execute("style.character.create", serde_json::json!({"name":"Mark","chars":{"size":14}})).unwrap();
    let mut exchange = Exchange::new();
    let root = exchange.batch(&mut doc, &serde_json::json!({"ops":[],"load":[{"target":"layout","fields":["characterStyles"]}]}), true).unwrap();
    let loads: Vec<_> = root["loaded"][0]["fields"]["characterStyles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| serde_json::json!({"target":id,"fields":["name"]}))
        .collect();
    let styles = exchange.batch(&mut doc, &serde_json::json!({"ops":[],"load":loads}), true).unwrap();
    let id = styles["loaded"].as_array().unwrap().iter().find(|style| style["fields"]["name"] == "Mark").unwrap()["id"].clone();
    doc.execute("style.exportTag", serde_json::json!({"style":id,"character":true,"tag":"em"})).unwrap();
}

#[test]
fn integer_object_parameters_are_declared() {
    let doc = LayoutDocument::create("Ids.klayout", 612, 792).unwrap();
    let commands = doc.commands();
    kition_kit::testing::check_id_params(&commands, &[]).unwrap();
    println!(
        "C8 scanner: {} typed parameters; {} named parameters; {} place parameters; 0 integer exceptions; 3 name exceptions",
        commands.iter().map(|command| command.object_types.len()).sum::<usize>(),
        commands.iter().map(|command| command.name_params.len()).sum::<usize>(),
        commands.iter().map(|command| command.places.len()).sum::<usize>()
    );
}

fn nested_integer_gaps(commands: &[kition_kit::KitCommand]) -> Vec<String> {
    fn walk(command: &kition_kit::KitCommand, path: &str, name: &str, node: &Value, inherited_id: bool, gaps: &mut Vec<String>) {
        let described = node
            .get("description")
            .and_then(Value::as_str)
            .is_some_and(|s| s.split(|c: char| !c.is_alphanumeric()).any(|word| word.eq_ignore_ascii_case("id") || word.eq_ignore_ascii_case("ids")));
        let id = inherited_id || described || matches!(name, "id" | "ids") || ["Id", "Ids", "_id", "_ids"].iter().any(|end| name.ends_with(end));
        if node.get("type").and_then(Value::as_str) == Some("integer") && id {
            let declared = command.targets == Some(path) || command.objects.contains(&path) || command.places.contains(&path);
            if !declared {
                gaps.push(format!("{} {path}", command.id));
            }
        }
        for branch in ["oneOf", "anyOf", "allOf"].iter().flat_map(|key| node.get(key).and_then(Value::as_array).into_iter().flatten()) {
            walk(command, path, name, branch, id, gaps);
        }
        if let Some(items) = node.get("items") {
            let child = if items.get("properties").is_some() || items.get("items").is_some() { format!("{path}[]") } else { path.to_owned() };
            walk(command, &child, name, items, id, gaps);
        }
        if let Some(properties) = node.get("properties").and_then(Value::as_object) {
            for (key, child) in properties {
                walk(command, &format!("{path}.{key}"), key, child, false, gaps);
            }
        }
    }
    let mut gaps = Vec::new();
    for command in commands {
        for (key, node) in command.schema.as_ref().and_then(Value::as_object).into_iter().flatten() {
            walk(command, key, key, node, false, &mut gaps);
        }
    }
    gaps.sort();
    gaps.dedup();
    gaps
}

#[test]
fn integer_scanner_reaches_nested_members_arrays_and_union_branches() {
    let command = kition_kit::KitCommand {
        id: "synthetic.nested".to_owned(),
        schema: Some(serde_json::json!({"rules":{"type":"array","items":{"type":"object","properties":{
            "sourceId":{"oneOf":[{"type":"string"},{"type":"integer"}]},
            "nested":{"type":"object","properties":{"ids":{"type":"array","items":{"type":"integer"}}}}
        }}}})),
        ..Default::default()
    };
    assert_eq!(nested_integer_gaps(&[command]), ["synthetic.nested rules[].nested.ids", "synthetic.nested rules[].sourceId"]);
    let doc = LayoutDocument::create("Nested.klayout", 612, 792).unwrap();
    assert_eq!(nested_integer_gaps(&doc.commands()), Vec::<String>::new(), "zero exceptions at every declared schema depth");
}

fn name_gaps(commands: &[kition_kit::KitCommand]) -> Vec<String> {
    // These strings are values or new names, never lookups. Each exception follows parse_color
    // or the new-name assignment in cmd/style.rs, rather than the schema's imprecise wording.
    const VALUES: &[(&str, &str, &str)] = &[
        ("swatch.create", "color", "parse_color accepts a literal hex colour, never a swatch name"),
        ("object.color", "color", "parse_color accepts a literal hex colour, never a swatch name"),
        ("swatch.addToSwatches", "name", "the new name assigned to the selected swatch, not a lookup"),
    ];
    fn walk(command: &kition_kit::KitCommand, path: &str, node: &Value, gaps: &mut Vec<String>) {
        let description = node.get("description").and_then(Value::as_str).unwrap_or_default().to_lowercase();
        let candidate = description.contains("swatch") || description.contains("style name") || description.contains("id or name");
        if candidate && node.get("type").and_then(Value::as_str) == Some("string") && node.get("enum").is_none() {
            let declared = command.targets == Some(path) && command.name_field.is_some() || command.name_params.iter().any(|(name, _)| *name == path);
            if !declared && !VALUES.iter().any(|(id, name, reason)| *id == command.id && *name == path && !reason.is_empty()) {
                gaps.push(format!("{} {path}", command.id));
            }
        }
        if let Some(properties) = node.get("properties").and_then(Value::as_object) {
            for (key, value) in properties {
                walk(command, &format!("{path}.{key}"), value, gaps);
            }
        }
        if let Some(items) = node.get("items").filter(|items| items.get("properties").is_some()) {
            walk(command, &format!("{path}[]"), items, gaps);
        }
    }
    let mut gaps = Vec::new();
    for command in commands {
        if let Some(properties) = command.schema.as_ref().and_then(|schema| schema.get("properties").unwrap_or(schema).as_object()) {
            for (key, value) in properties {
                walk(command, key, value, &mut gaps);
            }
        }
    }
    gaps
}

#[test]
fn name_object_parameters_are_declared() {
    let doc = LayoutDocument::create("Names.klayout", 612, 792).unwrap();
    assert_eq!(name_gaps(&doc.commands()), Vec::<String>::new());
}

#[test]
fn typed_parameters_follow_the_implementation() {
    let doc = LayoutDocument::create("Types.klayout", 612, 792).unwrap();
    let commands = doc.commands();
    for (id, parameter, types) in [
        ("object.rename", "id", &["Item"][..]),
        ("object.fill", "ids", &["Item"][..]),
        ("frame.thread", "from", &["Item"][..]),
        ("frame.thread", "to", &["Item"][..]),
        ("story.setText", "story", &["Story"][..]),
        ("object.setLayer", "layer", &["Layer"][..]),
        ("layer.merge", "into", &["Layer"][..]),
        ("object.fill", "swatch", &["Swatch"][..]),
        ("type.char", "attrs.fill", &["Swatch"][..]),
        ("type.para", "attrs.nestedStyles[].style", &["Style"][..]),
        ("object.caption", "style", &["Style"][..]),
        ("style.table.create", "body", &["Style"][..]),
        ("style.table.edit", "altRows.firstColor", &["Swatch"][..]),
    ] {
        let command = commands.iter().find(|command| command.id == id).unwrap();
        assert!(command.object_types.contains(&(parameter, types)), "{id} {parameter}: {:?}", command.object_types);
    }
    for command in &commands {
        for path in command.targets.into_iter().chain(command.objects.iter().copied()) {
            assert!(command.object_types.iter().any(|(name, types)| *name == path && !types.is_empty()), "{} {path}", command.id);
        }
    }
}
