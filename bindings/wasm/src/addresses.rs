//! Agent addresses follow the handlers' resolvers, not parameter spelling.

use std::collections::HashMap;
use std::sync::OnceLock;

use kition_kit::KitCommand;
use kition_kit::kdm::{ObjectId, Replica, tool_ids::ToolIds};
use serde_json::Value;

const ITEMS: &[&str] = &[
    "select.set",
    "edit.delete",
    "transform.move",
    "transform.resize",
    "transform.rotate",
    "transform.scale",
    "transform.again",
    "transform.clear",
    "transform.shear",
    "object.matchAttributes",
    "transform.flip",
    "transform.set",
    "transform.info",
    "object.arrange",
    "arrange.group",
    "arrange.ungroup",
    "object.lock",
    "object.hide",
    "object.setFlags",
    "object.fill",
    "object.stroke",
    "object.opacity",
    "object.dropShadow",
    "object.innerShadow",
    "object.outerGlow",
    "object.exportOptions",
    "object.innerGlow",
    "object.bevel",
    "object.satin",
    "object.directionalFeather",
    "object.feather",
    "object.content",
    "object.textFrameOptions",
    "object.cornerOptions",
    "object.textWrap",
    "object.fit",
    "object.fittingOptions",
    "edit.stepAndRepeat",
    "object.align",
    "object.distribute",
    "object.clippingPath",
    "object.primaryTextFrame",
    "object.attributes",
    "object.label",
    "object.altText",
    "object.transparencyGroup",
    "object.setLayer",
    "frame.fitText",
    "article.new",
    "article.add",
    "article.remove",
    "object.makeCompoundPath",
    "object.releaseCompoundPath",
    "text.convertToOutlines",
    "object.pathfinder",
    "object.gradientFeather",
    "button.set",
    "button.clear",
    "form.set",
    "form.clear",
    "object.caption",
    "conveyor.collect",
    "media.options",
    "object.layerOptions",
    "liquid.object",
    "xml.tag",
    "state.add",
    "object.convertShape",
    "style.object.apply",
    "object.gradient",
    "object.color",
    "layout.overrideParentItems",
    "anchored.insert",
    "link.add",
];

const ITEM: &[&str] = &[
    "select.toggle",
    "edit.pasteInto",
    "object.rename",
    "path.appendAnchor",
    "path.close",
    "path.moveAnchors",
    "path.addAnchor",
    "path.deleteAnchor",
    "path.convertAnchor",
    "path.erase",
    "path.smooth",
    "path.split",
    "math.insert",
    "media.get",
    "object.pdfLayers",
    "state.add",
    "state.list",
    "state.show",
    "state.rename",
    "state.release",
    "text.onPath",
    "text.pathOptions",
];

const STORIES: &[&str] = &[
    "file.exportText",
    "endnote.edit",
    "endnote.delete",
    "note.edit",
    "note.delete",
    "note.convertToText",
    "change.accept",
    "change.reject",
    "change.acceptAll",
    "change.rejectAll",
    "condition.apply",
    "story.placeAndLink",
    "story.updateLink",
    "story.unlink",
    "text.select",
    "story.setDirection",
    "story.setText",
    "story.replaceRange",
    "story.get",
    "text.fillWithPlaceholder",
    "style.cell.apply",
    "style.table.apply",
    "table.convertToText",
    "table.insertRow",
    "table.insertColumn",
    "table.deleteRow",
    "table.deleteColumn",
    "table.delete",
    "table.merge",
    "table.placeGraphic",
    "table.textCell",
    "table.moveRow",
    "table.moveColumn",
    "table.sortRows",
    "table.unmerge",
    "table.splitHorizontally",
    "table.splitVertically",
    "table.setRowHeight",
    "table.setColumnWidth",
    "table.distributeColumns",
    "table.distributeRows",
    "table.select",
    "table.get",
    "edit.find",
    "edit.replace",
    "edit.findNext",
    "footnote.list",
    "footnote.setText",
    "footnote.delete",
    "xref.insert",
    "xref.setFormat",
    "anchored.insert",
    "anchored.options",
    "anchored.release",
    "anchored.list",
    "spelling.check",
    "spelling.change",
    "data.placeholder.add",
    "file.exportPdf",
    "file.exportImage",
    "edit.undo",
    "edit.redo",
    "edit.selectAll",
    "object.directionalFeather",
    "object.layerOptions",
    "liquid.pageRule",
    "strokeStyle.new",
    "page.transition",
    "linkedFile.copyTo",
    "style.compositeFont.set",
    "swatch.save",
    "ink.options",
    "layout.marginsAndColumns",
    "layout.overrideParentItems",
    "prefs.set",
    "layout.addGuides",
    "linkedFile.update",
    "edit.findNext",
    "xref.setFormat",
];

/// Commands whose `spread` parameter is a spread counted from 0 (or a parent's name) that the
/// command creates or acts in.
pub(crate) const SPREAD_PLACES: &[&str] = &[
    "snippet.place",
    "place.drop",
    "file.place",
    "frame.add",
    "line.add",
    "path.add",
    "frame.grid",
    "gap.move",
    "frame.thread",
    "library.place",
    "conveyor.place",
    "story.placeAndLink",
    "math.insert",
    "object.qrCode",
    "guide.add",
    "guide.deleteAll",
    "guide.list",
    "layout.addGuides",
    "page.transition",
];

const FRAMES: &[&str] = &[
    "place.drop",
    "file.place",
    "file.exportText",
    "frame.unthread",
    "frame.loadText",
    "text.placeCaret",
    "text.release",
    "text.extendTo",
    "text.selectWord",
    "story.setDirection",
    "story.get",
    "text.fillWithPlaceholder",
    "table.dropCells",
];

#[derive(Default)]
struct Declaration {
    targets: Option<&'static str>,
    objects: &'static [&'static str],
    types: &'static [(&'static str, &'static [&'static str])],
    names: &'static [(&'static str, &'static str)],
    places: &'static [&'static str],
}

static DECLARATIONS: OnceLock<HashMap<String, Declaration>> = OnceLock::new();

fn declaration(command: &KitCommand) -> Declaration {
    let id = command.id.as_str();
    let mut parameters: Vec<(&'static str, &'static [&'static str], bool)> = Vec::new();
    let mut targets = None;
    let mut places = Vec::new();
    if ITEMS.contains(&id) {
        parameters.push(("ids", &["Item"], false));
        targets = Some("ids");
    }
    if ITEM.contains(&id) {
        parameters.push(("id", &["Item"], false));
        if targets.is_none() {
            targets = Some("id");
        }
    }
    if STORIES.contains(&id) {
        parameters.push(("story", &["Story"], false));
    }
    if FRAMES.contains(&id) {
        parameters.push(("frame", &["Item"], false));
    }
    match id {
        "frame.thread" => parameters.extend([("from", &["Item"][..], false), ("to", &["Item"][..], false)]),
        "object.matchAttributes" => parameters.push(("from", &["Item"], false)),
        "object.setLayer" => parameters.push(("layer", &["Layer"], false)),
        "edit.findObjects" | "edit.replaceObjects" => parameters.push(("layer", &["Layer"], true)),
        "layout.pages.applyParent" | "layout.pages.insert" => parameters.push(("parent", &["Spread"], true)),
        "layer.move" | "layer.selectItems" | "layer.set" | "layer.delete" | "layer.others" | "layer.activate" => {
            targets = Some("id");
            parameters.push(("id", &["Layer"], false));
        }
        "layer.merge" => {
            targets = Some("ids");
            parameters.push(("ids", &["Layer"], false));
            parameters.push(("into", &["Layer"], false));
        }
        "article.options" => parameters.push(("order", &["Item"], false)),
        "linkedFile.copyTo" => parameters.extend([("asset", &["Asset"][..], false), ("assets", &["Asset"][..], false)]),
        "linkedFile.relinkFolder" => parameters.push(("assets", &["Asset"], false)),
        "data.placeholder.add" => parameters.push(("item", &["Item"], false)),
        "linkedFile.relink" | "linkedFile.update" | "linkedFile.embed" | "linkedFile.goTo" | "toc.update" | "index.update" | "data.source.update" => {
            parameters.push(("asset", &["Asset"], false))
        }
        _ => {}
    }
    // These ids are story-local or entries in a settings register; a bare number is not a
    // global object. Unlike tables below, these places still need holder/premise resolution.
    match id {
        "footnote.setText"
        | "footnote.delete"
        | "endnote.edit"
        | "endnote.delete"
        | "note.edit"
        | "note.delete"
        | "note.convertToText"
        | "link.edit"
        | "link.delete"
        | "link.goToSource"
        | "data.placeholder.remove" => places.push("id"),
        _ => {}
    }
    if (id.starts_with("table.") || matches!(id, "style.cell.add" | "style.cell.apply" | "style.table.apply"))
        && command.schema.as_ref().and_then(|schema| schema.get("table")).is_some()
    {
        places.extend(["table", "row", "col", "rows", "cols"]);
        if !parameters.iter().any(|(path, _, _)| *path == "story") {
            parameters.push(("story", &["Story"], false));
        }
    }
    if SPREAD_PLACES.contains(&id) {
        places.push("spread");
    }
    match id {
        "layer.move" => places.push("to"),
        // Positions inside an object a premise holds whole (an item's path or states, a
        // story's anchored objects or cross-references, a table's rows) or a page/spread number.
        "state.show" | "state.rename" | "anchored.options" | "anchored.release" | "xref.setFormat" => places.push("index"),
        "path.deleteAnchor" | "path.convertAnchor" => places.extend(["anchor", "subpath"]),
        "table.moveRow" | "table.moveColumn" => places.extend(["from", "to"]),
        "table.sortRows" => places.push("column"),
        "text.select" => places.extend(["anchor", "focus"]),
        "frame.thread" => places.extend(["page", "at"]),
        "button.set" | "toc.generate" | "index.generate" | "guide.add" | "layout.addGuides" | "file.place" => places.push("page"),
        "page.transition" | "file.exportPdf" | "file.exportImage" | "file.exportBytes" | "liquid.pageRule" | "page.background" => {
            places.push("pages")
        }
        "path.moveAnchors" => places.push("anchors"),
        "file.render" => places.extend(["slide", "target.page", "target.spread"]),
        "story.replaceRange" | "spelling.change" => places.extend(["start", "end"]),
        "anchored.insert" => places.push("pos"),
        "transform.move" => places.push("toSpread"),
        "gap.move" => places.push("at"),
        // The tag names the set of items it untags, told by `covers` (C8-layout review 3).
        "xml.deleteTag" => places.push("name"),
        // The layout's name names the pages (a place) whose items it deletes.
        "layout.deleteAlternate" => places.push("name"),
        // The layer is also the place of the items its deletion deletes, told by `covers`.
        "layer.delete" => places.push("id"),
        "edit.replaceObjects" => places.push("change"),
        "bookmark.rename" | "bookmark.delete" => places.push("index"),
        "xref.insert" => places.extend(["anchor", "para", "paragraph"]),
        "layout.pages.insert" => places.push("after"),
        "layout.pages.delete" => places.push("pages"),
        "layout.pages.move" => places.extend(["from", "to"]),
        "layout.pages.applyParent" | "layout.pageSize" | "layout.marginsAndColumns" => places.push("pages"),
        "layout.pages.duplicateSpread" | "layout.spreadShuffle" | "layout.rotateSpreadView" => places.push("spread"),
        "layout.pages.toSpread" => places.extend(["page", "spread"]),
        "guide.move" | "guide.delete" | "guide.liquid" => places.extend(["spread", "page", "index"]),
        "layout.section"
        | "layout.overrideParentItems"
        | "layout.removeOverrides"
        | "layout.detachAll"
        | "bookmark.add"
        | "link.add"
        | "link.edit" => places.push("page"),
        _ => {}
    }
    // Swatches are one namespace. Styles have five; UUID execution validates the family below,
    // but the host's generic Style-name guard cannot yet express that family scope.
    let mut name = |path, ty| parameters.push((path, ty, true));
    match id {
        "object.fill" => name("swatch", &["Swatch"]),
        "object.stroke" => {
            name("swatch", &["Swatch"]);
            name("gapSwatch", &["Swatch"]);
        }
        "object.dropShadow" | "object.innerShadow" | "object.outerGlow" | "object.innerGlow" | "object.satin" | "object.qrCode" => {
            name("color", &["Swatch"])
        }
        "object.bevel" => {
            name("highlight", &["Swatch"]);
            name("shadow", &["Swatch"]);
        }
        "object.textFrameOptions" => name("columnRuleColor", &["Swatch"]),
        "swatch.delete" => name("name", &["Swatch"]),
        "swatch.addToSwatches" => name("swatch", &["Swatch"]),
        "swatch.save" => name("names", &["Swatch"]),
        "swatch.moveToGroup" | "swatch.newColorGroup" => name("swatches", &["Swatch"]),
        "ink.options" => {
            name("ink", &["Swatch"]);
            name("alias", &["Swatch"]);
        }
        "style.paragraph.apply"
        | "style.character.apply"
        | "style.object.apply"
        | "style.cell.apply"
        | "style.table.apply"
        | "style.paragraph.edit"
        | "style.character.edit"
        | "style.cell.edit"
        | "style.table.edit"
        | "style.paragraph.delete"
        | "style.character.delete" => name("name", &["Style"]),
        "style.exportTag" | "xml.mapStyle" => name("style", &["Style"]),
        "style.group" => name("names", &["Style"]),
        "object.caption" => name("style", &["Style"]),
        "toc.generate" => {
            name("styles", &["Style"]);
            name("entries[].style", &["Style"]);
        }
        _ => {}
    }
    // CharAttrs and ParaAttrs are accepted both directly and under `attrs` by the text
    // handlers, and under `chars` / `para` by the style handlers.
    let chars: &[&str] = match id {
        "text.char" => &["", "attrs."],
        "style.paragraph.add" | "style.paragraph.edit" | "style.character.add" | "style.character.edit" => &["chars."],
        _ => &[],
    };
    for prefix in chars {
        for field in ["fill", "stroke", "underlineColor", "strikethroughColor"] {
            name(Box::leak(format!("{prefix}{field}").into_boxed_str()), &["Swatch"]);
        }
    }
    let paras: &[&str] = match id {
        "text.para" => &["", "attrs."],
        "style.paragraph.add" | "style.paragraph.edit" => &["para."],
        _ => &[],
    };
    for prefix in paras {
        for field in ["shadingColor", "borderColor", "ruleAbove.color", "ruleBelow.color"] {
            name(Box::leak(format!("{prefix}{field}").into_boxed_str()), &["Swatch"]);
        }
        for field in ["nestedStyles[].style", "grepStyles[].style", "nestedLineStyles[].style"] {
            name(Box::leak(format!("{prefix}{field}").into_boxed_str()), &["Style"]);
        }
    }
    match id {
        "style.paragraph.add" | "style.paragraph.edit" => {
            name("basedOn", &["Style"]);
            name("nextStyle", &["Style"]);
        }
        "style.character.add" | "style.character.edit" | "style.table.add" => name("basedOn", &["Style"]),
        "style.paragraph.delete" | "style.character.delete" => name("replaceWith", &["Style"]),
        "style.object.add" | "style.cell.add" | "style.cell.edit" => {
            name("fill", &["Swatch"]);
            name("paragraphStyle", &["Style"]);
        }
        _ => {}
    }
    if matches!(id, "style.table.add" | "style.table.edit") {
        for field in ["header", "body", "footer", "leftColumn", "rightColumn"] {
            name(field, &["Style"]);
        }
        for field in ["border.color", "altRows.firstColor", "altRows.nextColor"] {
            name(field, &["Swatch"]);
        }
        if id == "style.table.edit" {
            name("basedOn", &["Style"]);
        }
    }
    match id {
        "edit.findObjects" | "edit.replaceObjects" => {
            name("fill", &["Swatch"]);
            name("stroke", &["Swatch"]);
            if id == "edit.replaceObjects" {
                name("change.fill", &["Swatch"]);
                name("change.stroke", &["Swatch"]);
            }
        }
        "endnote.options" => {
            name("headingStyle", &["Style"]);
            name("paraStyle", &["Style"]);
        }
        "footnote.options" => {
            name("paraStyle", &["Style"]);
            name("refCharStyle", &["Style"]);
            name("rule.color", &["Swatch"]);
        }
        "table.setCell" => {
            name("fill", &["Swatch"]);
            name("stroke.color", &["Swatch"]);
        }
        "table.options" => {
            for path in ["border.color", "altRows.firstColor", "altRows.nextColor", "altCols.firstColor", "altCols.nextColor"] {
                name(path, &["Swatch"]);
            }
        }
        "style.cell.add" | "style.cell.edit" => name("stroke.color", &["Swatch"]),
        _ => {}
    }
    let names = parameters
        .iter()
        .filter(|(_, _, named)| *named)
        .map(|(path, types, _)| {
            let field = if *path == "parent" && types.contains(&"Spread") {
                "prefix"
            } else if types.contains(&"Style") && !matches!(id, "style.exportTag" | "style.group") {
                style_family(id, path, &serde_json::json!({}))
                    .and_then(|family| kition_layout_kdm::FAMILY_FIELDS.iter().position(|field| *field == family))
                    .and_then(|family| kition_layout_kdm::FAMILY_NAMES.get(family).copied())
                    .unwrap_or("name")
            } else {
                "name"
            };
            (*path, field)
        })
        .collect::<Vec<_>>();
    Declaration {
        targets,
        objects: Box::leak(parameters.iter().map(|(path, _, _)| *path).filter(|path| Some(*path) != targets).collect::<Vec<_>>().into_boxed_slice()),
        types: Box::leak(parameters.iter().map(|(path, types, _)| (*path, *types)).collect::<Vec<_>>().into_boxed_slice()),
        names: Box::leak(names.into_boxed_slice()),
        places: Box::leak(places.into_boxed_slice()),
    }
}

pub(crate) fn declare(commands: &mut [KitCommand]) {
    let declarations = DECLARATIONS.get_or_init(|| commands.iter().map(|command| (command.id.clone(), declaration(command))).collect());
    for command in commands {
        if let Some(declaration) = declarations.get(&command.id) {
            command.targets = declaration.targets;
            command.objects = declaration.objects;
            command.object_types = declaration.types;
            command.name_params = declaration.names;
            command.places = declaration.places;
            declare_reads(command);
            if let Some(schema) = command.schema.as_mut() {
                for (path, _) in declaration.types {
                    ensure_schema(schema, path);
                }
            }
        }
    }
}

fn declare_reads(command: &mut KitCommand) {
    use kition_kit::EditorState::{Active, Mode, Selection};
    const S: &[kition_kit::EditorState] = &[Selection];
    const M: &[kition_kit::EditorState] = &[Mode];
    const SM: &[kition_kit::EditorState] = &[Selection, Mode];
    const AM: &[kition_kit::EditorState] = &[Active, Mode];
    const SAM: &[kition_kit::EditorState] = &[Selection, Active, Mode];
    let id = command.id.as_str();
    // C8-layout-read-3: these set a selection, or a document setting, without reading one: the
    // path text's target is its `id` (cmd/path_type.rs), the blend space a parameter (cmd/prefs.rs).
    let writes_selection_only = matches!(id, "edit.transparencyBlendSpace" | "text.onPath");
    // Find and the two object searches declare no selection read; `edit.` below does not take them.
    let search = matches!(id, "edit.find" | "edit.findObjects" | "edit.replaceObjects");
    let selection = !writes_selection_only
        && !search
        && (ITEMS.contains(&id)
            || [
                "text.",
                "format.",
                "select.",
                "table.",
                "style.paragraph.apply",
                "style.character.apply",
                "edit.",
                "anchor.add",
                "xref.insert",
                "object.qrCode",
                "footnote.insert",
                "footnote.goToReference",
                "endnote.insert",
                "note.new",
                "style.paragraph.add",
                "style.object.add",
                "object.rename",
            ]
            .iter()
            .any(|prefix| id.starts_with(prefix))
            || matches!(
                id,
                "data.placeholder.add"
                    | "math.insert"
                    | "script.run"
                    | "state.show"
                    | "state.rename"
                    | "state.release"
                    | "xml.tagText"
                    | "layout.pages.delete"
                    | "layout.removeOverrides"
                    | "layout.detachAll"
                    | "layout.deleteAlternate"
                    | "file.place"
                    | "file.exportText"
                    | "arrange.bringToFront"
                    | "arrange.bringForward"
                    | "arrange.sendBackward"
                    | "arrange.sendToBack"
                    | "select.toggle"
                    | "anchored.options"
                    | "anchored.release"
                    | "library.add"
                    | "change.accept"
                    | "change.reject"
                    | "change.acceptAll"
                    | "change.rejectAll"
                    | "condition.apply"
                    | "variable.insert"
                    | "footnote.delete"
                    | "index.addReference"
                    | "edit.replace"
                    | "edit.findNext"
                    | "swatch.addToSwatches"
                    | "story.setDirection"
                    | "story.setText"
                    | "story.replaceRange"
                    | "style.breakLink"
                    | "style.cell.add"
                    | "style.cell.apply"
                    | "style.table.apply"
            ));
    let active = matches!(
        id,
        "frame.add"
            | "line.add"
            | "script.run"
            | "frame.grid"
            | "path.add"
            | "graphic.place"
            | "file.place"
            | "place.drop"
            | "media.place"
            | "math.insert"
            | "object.qrCode"
            | "object.caption"
            | "story.placeAndLink"
            | "frame.thread"
            | "guide.add"
            | "layout.addGuides"
    ); // C8-layout-read-3: paste keeps each object's own layer (cmd/object.rs duplicate_from), not the active one.
    // Session::execute runs smart reflow for every undoable command. Transform commands also
    // read scale/measurement preferences; paste/repeat use the session clipboard/history.
    let mode = command.mutates || id.starts_with("prefs.") || id.starts_with("transform.") || id.starts_with("tool.");
    command.reads = match (selection, active, mode) {
        (true, true, _) => SAM,
        (true, false, true) => SM,
        (true, false, false) => S,
        (false, true, _) => AM,
        (false, false, true) => M,
        _ => &[],
    };
}

fn ensure_schema(schema: &mut Value, path: &str) {
    let (head, tail) = path.split_once('.').map_or((path, None), |(head, tail)| (head, Some(tail)));
    let array = head.ends_with("[]");
    let head = head.trim_end_matches("[]");
    let Some(properties) = schema.as_object_mut() else { return };
    let node = properties.entry(head).or_insert_with(|| serde_json::json!({"type":"string"}));
    if let Some(tail) = tail {
        if array {
            node["type"] = Value::from("array");
            if !node["items"].is_object() {
                node["items"] = serde_json::json!({"type":"object","additionalProperties":true,"properties":{}});
            }
            node["items"]["type"] = Value::from("object");
            node["items"]["additionalProperties"] = Value::Bool(true);
            if !node["items"]["properties"].is_object() {
                node["items"]["properties"] = serde_json::json!({});
            }
        } else {
            node["type"] = Value::from("object");
            node["additionalProperties"] = Value::Bool(true);
            if !node["properties"].is_object() {
                node["properties"] = serde_json::json!({});
            }
        }
        let child = if array { &mut node["items"]["properties"] } else { &mut node["properties"] };
        ensure_schema(child, tail);
    }
}

/// Receipts spell numeric ids as strings in some transports. Convert only the explicitly typed
/// numeric object parameters; counts, indexes, points, and new names keep their values.
pub(crate) fn resolve(command: &str, params: &Value, replica: Option<&Replica>) -> Result<Value, String> {
    let mut params = params.clone();
    let Some(declaration) = DECLARATIONS.get().and_then(|declarations| declarations.get(command)) else { return Ok(params) };
    let ids = replica.map(ToolIds::of_replica);
    let original = params.clone();
    for (path, types) in declaration.types {
        let mut convert = |value: &mut Value| -> Result<(), String> {
            let Some(text) = value.as_str() else { return Ok(()) };
            if let (Some(replica), Some(ids)) = (replica, &ids)
                && let Ok(object) = ids.resolve(text, |object| replica.exists(object))
                && let Ok(ty) = replica.object_type(&object)
                && types.contains(&ty.as_str())
            {
                if ty == "Spread" && *path == "parent" {
                    *value = replica.get(&object, "prefix").map_err(|error| error.to_string())?;
                } else if matches!(ty.as_str(), "Style" | "Swatch") {
                    if ty == "Style" {
                        let family = style_family(command, path, &original)
                            .ok_or_else(|| format!("invalid-params: {command} {path} needs a style name in its family"))?;
                        let family = replica.list(&ObjectId::from_string("layout"), family).map_err(|error| error.to_string())?;
                        if !family.contains(&object) {
                            return Err(format!("invalid-params: {path} names a style of another family"));
                        }
                    }
                    *value = replica.get(&object, "name").map_err(|error| error.to_string())?;
                } else {
                    *value = replica.get(&object, "number").map_err(|error| error.to_string())?;
                }
            } else if types.iter().any(|ty| matches!(*ty, "Item" | "Story" | "Layer" | "Asset" | "Page" | "Spread"))
                && let Ok(id) = text.parse::<u64>()
            {
                *value = Value::from(id);
            } else if let Some(replica) = replica
                && declaration.names.iter().any(|(name, _)| name == path)
            {
                // A name two definitions of one family share (a swatch in a colour group, a style
                // another writer renamed) names neither: the handler would take the first.
                let family = if types.contains(&"Swatch") { Some("swatches") } else { style_family(command, path, &original) };
                if let Some(family) = family {
                    let root = ObjectId::from_string("layout");
                    let mut spelled = 0usize;
                    for object in replica.list(&root, family).map_err(|error| error.to_string())? {
                        if replica.get(&object, "name").map_err(|error| error.to_string())? == text {
                            spelled += 1;
                        }
                    }
                    if spelled > 1 {
                        return Err(format!(
                            "invalid-params: `{path}` names {spelled} definitions spelled `{text}`; name one by the address layout_query lists for it"
                        ));
                    }
                }
            }
            Ok(())
        };
        visit(&mut params, path, &mut convert)?;
    }
    Ok(params)
}

pub(crate) fn visit(value: &mut Value, path: &str, convert: &mut impl FnMut(&mut Value) -> Result<(), String>) -> Result<(), String> {
    let (head, tail) = path.split_once('.').map_or((path, None), |(head, tail)| (head, Some(tail)));
    let Some(value) = value.get_mut(head.trim_end_matches("[]")) else { return Ok(()) };
    if head.ends_with("[]") {
        if let Some(values) = value.as_array_mut() {
            for value in values {
                if let Some(tail) = tail {
                    visit(value, tail, convert)?;
                } else {
                    convert(value)?;
                }
            }
        }
    } else if let Some(tail) = tail {
        visit(value, tail, convert)?;
    } else if let Some(values) = value.as_array_mut() {
        for value in values {
            convert(value)?;
        }
    } else {
        convert(value)?;
    }
    Ok(())
}

pub(crate) fn named_references(command: &str) -> impl Iterator<Item = &'static (&'static str, &'static [&'static str])> {
    DECLARATIONS
        .get()
        .and_then(|declarations| declarations.get(command))
        .into_iter()
        .flat_map(|d| d.types.iter())
        .filter(|(_, types)| types.iter().any(|ty| matches!(*ty, "Style" | "Swatch")))
}

pub(crate) fn style_family(command: &str, path: &str, params: &Value) -> Option<&'static str> {
    if command == "style.exportTag" {
        return Some(if params.get("character").and_then(Value::as_bool).unwrap_or(false) { "characterStyles" } else { "paragraphStyles" });
    }
    if matches!(command, "xml.mapStyle" | "toc.generate") {
        return Some("paragraphStyles");
    }
    if path.ends_with("[].style") {
        return Some("characterStyles");
    }
    if command == "object.caption" && path == "style" {
        return Some("paragraphStyles");
    }
    if matches!(path, "paragraphStyle" | "paraStyle" | "headingStyle") {
        return Some("paragraphStyles");
    }
    if path == "refCharStyle" {
        return Some("characterStyles");
    }
    if matches!(path, "header" | "body" | "footer" | "leftColumn" | "rightColumn") {
        return Some("cellStyles");
    }
    if command.starts_with("style.paragraph.") {
        return Some("paragraphStyles");
    }
    if command.starts_with("style.character.") {
        return Some("characterStyles");
    }
    if command.starts_with("style.object.") {
        return Some("objectStyles");
    }
    if command.starts_with("style.cell.") {
        return Some("cellStyles");
    }
    if command.starts_with("style.table.") {
        return Some("tableStyles");
    }
    if command == "style.group" {
        return match params.get("kind").and_then(Value::as_str).unwrap_or("paragraph") {
            "paragraph" => Some("paragraphStyles"),
            "character" => Some("characterStyles"),
            "object" => Some("objectStyles"),
            "table" => Some("tableStyles"),
            "cell" => Some("cellStyles"),
            _ => None,
        };
    }
    None
}
