//! C8-layout review 2: every parameter that accepts an integer, at any depth, is classified.
//! An id must be a declared target or object parameter, a positional index must be a declared
//! place, and a plain quantity is neither. A new unclassified integer parameter fails.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    /// Names an object: the command's `targets` or one of its `objects`.
    Id,
    /// A position in an ordered list (a page number, an index, a byte offset): a declared place.
    Place,
    /// A count, a size, a setting: names nothing.
    Quantity,
    /// Not a number: a value the schema leaves untyped (`json`, an array of unstated items) and so
    /// admits an integer, but which the handler reads as text, a colour, a flag, a point or bytes.
    Other,
}
use Class::{Id, Other, Place, Quantity};

include!("integer_params/table.rs");

/// Every integer-accepting parameter path of `command`: `integer`, a type list or a union
/// branch that admits one, or an array of them, through nested members and array items.
fn integer_paths(command: &kition_kit::KitCommand) -> Vec<String> {
    fn accepts(node: &Value) -> bool {
        let unions = ["oneOf", "anyOf", "allOf"].iter().any(|key| node.get(key).is_some());
        let typed = match node.get("type") {
            // `json` (the engine's notation for any value) and an array of unstated items admit one.
            Some(Value::String(ty)) => ty == "integer" || ty == "json" || ty == "array" && node.get("items").is_none(),
            Some(Value::Array(types)) => types.iter().any(|ty| ty == "integer"),
            // No type, no union, no enumeration: any value, an integer too.
            None => !unions && node.get("enum").is_none() && node.get("const").is_none() && node.get("properties").is_none(),
            _ => false,
        };
        typed || ["oneOf", "anyOf", "allOf"].iter().flat_map(|key| node.get(key).and_then(Value::as_array).into_iter().flatten()).any(accepts)
    }
    fn walk(path: &str, node: &Value, out: &mut Vec<String>) {
        if accepts(node) {
            out.push(path.to_owned());
        }
        let branches = ["oneOf", "anyOf", "allOf"].iter().flat_map(|key| node.get(key).and_then(Value::as_array).into_iter().flatten());
        for branch in branches.chain(node.get("items")) {
            let structured = branch.get("properties").is_some() || branch.get("items").is_some();
            let child = if node.get("items") == Some(branch) && structured { format!("{path}[]") } else { path.to_owned() };
            if node.get("items") == Some(branch) && !structured {
                if accepts(branch) {
                    out.push(path.to_owned());
                }
                continue;
            }
            walk(&child, branch, out);
        }
        for (key, child) in node.get("properties").and_then(Value::as_object).into_iter().flatten() {
            walk(&format!("{path}.{key}"), child, out);
        }
    }
    let mut out = Vec::new();
    for (key, node) in command.schema.as_ref().and_then(Value::as_object).into_iter().flatten() {
        walk(key, node, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn describe(command: &kition_kit::KitCommand, path: &str) -> String {
    let mut node = command.schema.clone().unwrap_or_default();
    for segment in path.split('.') {
        let key = segment.trim_end_matches("[]");
        node = if node.get("properties").is_some() { node["properties"][key].clone() } else { node[key].clone() };
        if segment.ends_with("[]") {
            node = node["items"].clone();
        }
    }
    format!(
        "{} | targets={:?} objects={:?} places={:?}",
        node.get("description").and_then(Value::as_str).unwrap_or(""),
        command.targets,
        command.objects,
        command.places
    )
}

#[test]
fn every_integer_parameter_is_classified_and_declared_as_classified() {
    let doc = LayoutDocument::create("Integers.klayout", 612, 792).unwrap();
    let commands = doc.commands();
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    for command in &commands {
        for path in integer_paths(command) {
            let class = TABLE.iter().find(|(id, p, _)| *id == command.id && *p == path).map(|(_, _, class)| *class);
            seen.push((command.id.clone(), path.clone()));
            let declared_id = command.targets == Some(path.as_str()) || command.objects.contains(&path.as_str());
            let declared_place = command.places.contains(&path.as_str());
            match class {
                None => problems.push(format!("    (\"{}\", \"{path}\", ?), // {}", command.id, describe(command, &path))),
                Some(Id) if !declared_id => problems.push(format!("{} {path}: an id, not a declared target/object", command.id)),
                Some(Place) if !declared_place => problems.push(format!("{} {path}: a positional index, not a declared place", command.id)),
                Some(Quantity | Other) if declared_id || declared_place => {
                    problems.push(format!("{} {path}: a quantity, but declared as an address", command.id))
                }
                _ => {}
            }
        }
    }
    for (id, path, _) in TABLE {
        if !seen.iter().any(|(c, p)| c == id && p == path) {
            problems.push(format!("{id} {path}: classified, but no such integer parameter"));
        }
    }
    let counts = [Id, Place, Quantity, Other].map(|class| TABLE.iter().filter(|(_, _, c)| *c == class).count());
    println!("C8 integer parameters: {} ({} id, {} place, {} quantity, {} other)", TABLE.len(), counts[0], counts[1], counts[2], counts[3]);
    assert!(problems.is_empty(), "integer parameters:\n{}", problems.join("\n"));
}

#[test]
fn the_scanner_reaches_type_lists_unions_nested_members_and_items() {
    let command = kition_kit::KitCommand {
        id: "synthetic.nested".to_owned(),
        schema: Some(serde_json::json!({
            "page": {"type": ["integer", "string"]},
            "count": {"anyOf": [{"type": "string"}, {"type": "integer"}]},
            "rules": {"type": "array", "items": {"type": "object", "properties": {"index": {"type": "integer"}}}},
            "rows": {"type": "array", "items": {"type": "integer"}},
            "size": {"type": "number"},
            "any": {"type": "json"},
            "list": {"type": "array"},
            "named": {"enum": ["a", "b"]}
        })),
        ..Default::default()
    };
    assert_eq!(integer_paths(&command), ["any", "count", "list", "page", "rows", "rules[].index"]);
}
