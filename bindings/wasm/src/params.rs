//! The parameters commands read that their notation does not spell out, added to the schemas the
//! catalogue offers agents. Every command refuses a key its schema does not name (engine rule E7a),
//! so the schema must name every key the command reads: the keys a
//! handler reads beside its listed ones.

use serde_json::{Value, json};

#[derive(Clone, Copy)]
enum Kind {
    Integer,
    Number,
    String,
    Boolean,
    Array,
    IndexOrName,
}

impl Kind {
    fn node(self) -> Value {
        let description = "read by the command beside its listed parameters";
        let kind = match self {
            Kind::IndexOrName => return json!({ "oneOf": [{ "type": "integer" }, { "type": "string" }], "description": description }),
            Kind::Integer => "integer",
            Kind::Number => "number",
            Kind::String => "string",
            Kind::Boolean => "boolean",
            Kind::Array => "array",
        };
        json!({ "type": kind, "description": description })
    }
}

/// Keys a command's handler reads (directly or through a helper it passes its parameters to) that
/// its notation leaves out, found by reading each handler.
const EXTRA: &[(&str, &[(&str, Kind)])] = &[
    ("anchored.insert", &[("edge", Kind::String), ("to", Kind::String)]),
    ("anchored.options", &[("edge", Kind::String), ("to", Kind::String)]),
    ("button.clear", &[("action", Kind::String)]),
    ("condition.apply", &[("story", Kind::Integer)]),
    ("conveyor.collect", &[("to", Kind::String)]),
    ("conveyor.place", &[("base64", Kind::String)]),
    ("data.fields", &[("base64", Kind::String), ("sheet", Kind::String), ("delimiter", Kind::String)]),
    ("data.options", &[("to", Kind::String)]),
    ("data.source.select", &[("what", Kind::String)]),
    ("data.source.update", &[("asset", Kind::Integer)]),
    ("edit.redo", &[("story", Kind::Integer)]),
    ("edit.selectAll", &[("story", Kind::Integer)]),
    ("edit.undo", &[("story", Kind::Integer)]),
    ("endnote.list", &[("number", Kind::String)]),
    ("endnote.options", &[("text", Kind::String)]),
    ("file.exportFixedEpub", &[("fixedLayout", Kind::Boolean), ("cover", Kind::Boolean)]),
    ("file.exportIdml", &[("dir", Kind::String), ("instructions", Kind::String), ("idml", Kind::Boolean), ("pdf", Kind::Boolean)]),
    ("file.exportImage", &[("story", Kind::Integer)]),
    ("file.exportPdf", &[("story", Kind::Integer)]),
    ("file.open", &[("base64", Kind::String), ("name", Kind::String)]),
    ("file.openIdml", &[("dir", Kind::String), ("instructions", Kind::String), ("idml", Kind::Boolean), ("pdf", Kind::Boolean)]),
    ("file.place", &[("height", Kind::Number), ("rect", Kind::Array), ("autoflow", Kind::Boolean), ("page", Kind::Integer)]),
    ("find.next", &[("wholeWord", Kind::Boolean), ("caseSensitive", Kind::Boolean), ("scope", Kind::String), ("story", Kind::Integer)]),
    ("footnote.options", &[("text", Kind::String)]),
    ("form.set", &[("autoflow", Kind::Boolean), ("keepLoaded", Kind::Boolean)]),
    ("frame.create", &[("starInset", Kind::Number)]),
    ("frame.thread", &[("position", Kind::String), ("at", Kind::Integer), ("spreadGuide", Kind::Boolean), ("orientation", Kind::String)]),
    ("guide.list", &[("orientation", Kind::String)]),
    ("index.update", &[("asset", Kind::Integer)]),
    ("ink.options", &[("story", Kind::Integer)]),
    ("layout.createGuides", &[("story", Kind::Integer)]),
    ("layout.marginsAndColumns", &[("story", Kind::Integer)]),
    ("layout.overrideParentItems", &[("story", Kind::Integer)]),
    ("layout.pages.duplicateSpread", &[("to", Kind::String)]),
    ("library.place", &[("base64", Kind::String)]),
    ("links.copyTo", &[("story", Kind::Integer), ("path", Kind::String), ("asset", Kind::Integer)]),
    ("links.update", &[("story", Kind::Integer)]),
    ("liquid.pageRule", &[("story", Kind::Integer)]),
    ("math.insert", &[("id", Kind::IndexOrName)]),
    ("media.options", &[("what", Kind::String)]),
    ("object.bringForward", &[("to", Kind::String)]),
    ("object.bringToFront", &[("to", Kind::String)]),
    ("object.directionalFeather", &[("story", Kind::Integer)]),
    ("object.exportOptions", &[("edge", Kind::String), ("to", Kind::String)]),
    ("object.fittingOptions", &[("edge", Kind::String), ("to", Kind::String)]),
    ("object.layerOptions", &[("story", Kind::Integer), ("what", Kind::String)]),
    ("object.primaryTextFrame", &[("what", Kind::String)]),
    ("object.sendBackward", &[("to", Kind::String)]),
    ("object.sendToBack", &[("to", Kind::String)]),
    ("object.stroke", &[("edge", Kind::String), ("to", Kind::String)]),
    ("page.transition", &[("story", Kind::Integer)]),
    (
        "prefs.set",
        &[
            ("story", Kind::Integer),
            ("smartTextReflow", Kind::Boolean),
            ("autocorrect", Kind::Boolean),
            ("autocorrectList", Kind::Array),
            ("showAddedText", Kind::Boolean),
            ("showNoteAnchors", Kind::Boolean),
            ("showTextThreads", Kind::Boolean),
            ("recoveryMinutes", Kind::Number),
        ],
    ),
    ("select.container", &[("what", Kind::String)]),
    ("select.content", &[("what", Kind::String)]),
    ("select.firstAbove", &[("what", Kind::String)]),
    ("select.lastBelow", &[("what", Kind::String)]),
    ("select.nextAbove", &[("what", Kind::String)]),
    ("select.nextBelow", &[("what", Kind::String)]),
    ("select.nextInGroup", &[("what", Kind::String)]),
    ("select.previousInGroup", &[("what", Kind::String)]),
    ("snippet.place", &[("name", Kind::String)]),
    ("states.create", &[("what", Kind::String), ("id", Kind::IndexOrName)]),
    ("story.get", &[("lines", Kind::Integer)]),
    ("story.setDirection", &[("frame", Kind::IndexOrName)]),
    ("strokeStyle.new", &[("story", Kind::Integer)]),
    ("style.cell.apply", &[("rows", Kind::Integer), ("cols", Kind::Integer)]),
    ("style.compositeFont.set", &[("story", Kind::Integer), ("autoflow", Kind::Boolean), ("keepLoaded", Kind::Boolean)]),
    ("style.table.apply", &[("rows", Kind::Integer), ("cols", Kind::Integer)]),
    ("style.table.create", &[("basedOn", Kind::String)]),
    ("swatch.save", &[("story", Kind::Integer)]),
    ("table.convertFromText", &[("width", Kind::Number)]),
    ("table.convertToText", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.delete", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.deleteColumn", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.deleteRow", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.distributeColumns", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.distributeRows", &[("cols", Kind::Integer)]),
    ("table.insert", &[("columns", Kind::Integer)]),
    ("table.insertColumn", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.insertRow", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.merge", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.moveColumn", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.moveRow", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.placeGraphic", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.setColumnWidth", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.setRowHeight", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.sortRows", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.splitHorizontally", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.splitVertically", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.textCell", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("table.unmerge", &[("rows", Kind::Integer), ("cols", Kind::Integer), ("table", Kind::Integer), ("story", Kind::Integer)]),
    ("text.select", &[("what", Kind::String)]),
    ("toc.update", &[("asset", Kind::Integer)]),
    ("tool.select", &[("what", Kind::String)]),
    ("transform.again", &[("copy", Kind::Boolean)]),
    ("transform.resize", &[("axis", Kind::String), ("by", Kind::String), ("spacing", Kind::Number)]),
    ("type.align", &[("edge", Kind::String), ("to", Kind::String), ("text", Kind::String), ("raw", Kind::Boolean)]),
    ("type.alignCenter", &[("text", Kind::String), ("raw", Kind::Boolean), ("edge", Kind::String), ("to", Kind::String)]),
    ("type.alignLeft", &[("text", Kind::String), ("raw", Kind::Boolean), ("edge", Kind::String), ("to", Kind::String)]),
    ("type.alignRight", &[("text", Kind::String), ("raw", Kind::Boolean), ("edge", Kind::String), ("to", Kind::String)]),
    ("type.fillWithPlaceholder", &[("story", Kind::Integer)]),
    ("type.justify", &[("text", Kind::String), ("raw", Kind::Boolean), ("edge", Kind::String), ("to", Kind::String)]),
    ("type.onPath", &[("edge", Kind::String), ("to", Kind::String)]),
    ("type.pathOptions", &[("edge", Kind::String), ("to", Kind::String)]),
    ("type.warichu", &[("edge", Kind::String), ("to", Kind::String), ("text", Kind::String), ("raw", Kind::Boolean)]),
    ("xml.tag", &[("what", Kind::String)]),
    ("xml.tagText", &[("what", Kind::String)]),
];

/// Adds to `schema` (a command's `ParameterSchemaSpec`) the keys `id` reads that its notation
/// leaves out.
pub fn complete(id: &str, schema: &mut Value) {
    let Some(properties) = schema.as_object_mut() else { return };
    if matches!(id, "layout.pages.applyParent" | "layout.pages.insert") {
        properties.insert(
            "parent".into(),
            json!({"oneOf":[{"type":"string","x-khp-name":"parentLabel","x-khp-types":["Spread"]},{"type":"null"}],"description":"parent prefix or full prefix-name label; null clears"}),
        );
    }
    if matches!(id, "style.cell.create" | "style.cell.edit") {
        for (key, node) in [
            ("basedOn", Kind::String.node()),
            ("tint", Kind::Number.node()),
            ("vj", Kind::String.node()),
            ("insets", json!({"oneOf":[{"type":"number"},{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4}]})),
        ] {
            properties.entry(key).or_insert(node);
        }
    }
    if matches!(id, "style.table.create" | "style.table.edit") {
        for key in ["spaceBefore", "spaceAfter"] {
            properties.entry(key).or_insert_with(|| Kind::Number.node());
        }
    }
    for (key, kind) in EXTRA.iter().filter(|(command, _)| *command == id).flat_map(|(_, keys)| keys.iter()) {
        properties.entry(*key).or_insert_with(|| kind.node());
    }
    if (id.starts_with("table.") && !matches!(id, "table.insert" | "table.convertFromText" | "table.dropCells"))
        || matches!(id, "style.cell.create" | "style.cell.apply" | "style.table.apply")
    {
        for key in ["story", "table", "row", "col"] {
            properties.entry(key).or_insert_with(|| Kind::Integer.node());
        }
        for key in ["rows", "cols"] {
            properties.insert(
                key.to_owned(),
                json!({"type":"array","items":{"type":"integer"},"minItems":1,"maxItems":2,"description":"inclusive zero-based index range"}),
            );
        }
    }
    let array = match id {
        "layer.merge" => Some(("ids", "integer")),
        "article.options" => Some(("order", "integer")),
        "swatch.moveToGroup" => Some(("swatches", "string")),
        "swatch.save" | "style.group" => Some(("names", "string")),
        _ => None,
    };
    if let Some((name, kind)) = array
        && let Some(node) = properties.get_mut(name).and_then(Value::as_object_mut)
    {
        node.insert("items".to_owned(), json!({"type":kind}));
    }
}
