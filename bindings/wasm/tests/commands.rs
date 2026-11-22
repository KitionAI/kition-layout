//! Every command says what it does and every read command an agent is offered says what it answers
//! (kition_kit::testing::check_commands, KHP A1, W1-46 P2-1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;

fn basic() -> LayoutDocument {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.klayout"));
    LayoutDocument::open("basic.klayout", &std::fs::read(path).unwrap()).unwrap()
}

/// Runs a read command on `basic.klayout` with its sample parameters (`command_samples.json`, `{}` when
/// it has none), after the sample's setup steps.
fn sample(id: &str) -> Result<serde_json::Value, String> {
    let samples: serde_json::Value = serde_json::from_str(include_str!("command_samples.json")).unwrap();
    let tmp = std::env::temp_dir().join(format!("kition-layout-samples-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let fill = |value: &serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&value.to_string().replace("$tmp", &tmp.to_string_lossy())).unwrap()
    };
    let mut doc = basic();
    let entry = &samples[id];
    if let Some(reason) = entry["skip"].as_str() {
        return Err(format!("skip: {reason}"));
    }
    for step in entry["setup"].as_array().into_iter().flatten() {
        doc.execute(step[0].as_str().unwrap(), fill(&step[1]))?;
    }
    doc.execute(id, entry.get("params").map_or(serde_json::json!({}), fill))
}

/// What each read command answers matches the output schema it declares (KHP A1).
#[test]
fn every_read_command_answers_what_its_output_schema_says() {
    let mut failures = Vec::new();
    for command in basic().commands().into_iter().filter(|c| c.schema.is_some() && !c.mutates) {
        let Some(schema) = command.output_schema else { continue };
        if let Err(error) = kition_kit::schema::validate_output(&schema) {
            failures.push(format!("{}: {error}", command.id));
            continue;
        }
        match sample(&command.id) {
            Ok(value) => {
                let wrapped = serde_json::json!({ "answer": schema });
                let params = serde_json::Map::from_iter([("answer".to_owned(), value)]);
                if let Err(error) = kition_kit::schema::check_params(&wrapped, &params) {
                    failures.push(format!("{}: {error}", command.id));
                }
            }
            Err(error) if error.starts_with("skip: ") => {}
            Err(error) => failures.push(format!("{}: the sample call failed: {error}", command.id)),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn every_command_is_described_and_every_read_command_declares_its_output() {
    if let Err(problems) = kition_kit::testing::check_commands(&basic().commands(), kition_layout_kit::NO_OUTPUT) {
        panic!("{} problems:\n{}", problems.len(), problems.join("\n"));
    }
}

/// regressions/issue-unreadable-param-notes: every entry of every command's parameter notation is
/// a key the notation reads. check_commands compares only a schema that is the notation's own, and
/// this kit adds typed nodes to its schemas (describe.rs, params.rs), so it does not see an entry
/// such as `xRelative?/yRelative?: …` that converts to no key: the schema then has no such
/// parameter and a call passing one is refused as unknown.
#[test]
fn every_parameter_notation_entry_is_a_key_the_notation_reads() {
    let mut problems = Vec::new();
    for command in basic().commands() {
        let Some(notation) = command.params.as_deref() else { continue };
        match kition_kit::schema::from_kit_params(notation) {
            Ok(converted) if converted.unreadable.is_empty() => {}
            Ok(converted) => problems.push(format!("{}: {:?}", command.id, converted.unreadable)),
            Err(error) => problems.push(format!("{}: {error}", command.id)),
        }
    }
    assert!(problems.is_empty(), "{} commands:\n{}", problems.len(), problems.join("\n"));
}

/// regressions/issue-unreadable-param-notes: Insert Anchored Object and Anchored Object Options take
/// the custom position's reference and points, which the handler reads.
#[test]
fn anchored_commands_take_their_custom_position_keys() {
    let commands = basic().commands();
    for id in ["anchored.insert", "anchored.options"] {
        let command = commands.iter().find(|command| command.id == id).unwrap();
        let schema = command.schema.as_ref().unwrap();
        for key in ["xRelative", "yRelative", "objectPoint", "refPoint"] {
            assert!(schema.get(key).is_some(), "{id} has no `{key}`: {schema}");
        }
    }
}

/// `KITION_DUMP_COMMANDS=<file>`: writes the command list and what each read command answers on
/// `basic.klayout` with no parameters, for writing descriptions and output schemas.
#[test]
fn dump_commands() {
    let Ok(out) = std::env::var("KITION_DUMP_COMMANDS") else { return };
    let doc = basic();
    let mut list = Vec::new();
    for command in doc.commands() {
        let mut entry = serde_json::to_value(&command).unwrap();
        if command.schema.is_some() && !command.mutates {
            entry["sample"] = match sample(&command.id) {
                Ok(value) => value,
                Err(error) => serde_json::json!({ "error": error }),
            };
        }
        list.push(entry);
    }
    std::fs::write(out, serde_json::to_string_pretty(&list).unwrap()).unwrap();
}
