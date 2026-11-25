//! Executes the blind projections and compares their facts with effective guard holds.
//! This is a declaration check, not a substitute for an independent-writer behavior test.

use std::collections::{BTreeMap, BTreeSet};

use kition_kit::kdm::{FieldKind, ObjectId, Replica, Schema, tool_ids::ToolIds};
use kition_kit::session::model::Exchange;
use kition_kit::{KitCommand, KitDocument, Under};
use kition_layout_kit::LayoutDocument;
use serde::Serialize;
use serde_json::{Value, json};

use super::Check;

/// Classify the fact the independent probe names, before considering the declaration.
/// A command may appear in several classes; unseen conditional branches are not inferred.
pub fn class(command: &str, field: &str) -> &'static str {
    if field.contains("settings.styleDefaults")
        || field.contains("setup.rightToLeftBinding")
        || matches!(
            command,
            "format.increaseFontSize"
                | "format.decreaseFontSize"
                | "text.openType"
                | "style.breakLink"
                | "table.insert"
                | "table.convertFromText"
                | "condition.new"
                | "layout.parents.new"
        )
    {
        "default"
    } else if matches!(
        command,
        "edit.delete"
            | "edit.copy"
            | "edit.cut"
            | "edit.paste"
            | "edit.pasteInPlace"
            | "edit.pasteWithoutFormatting"
            | "object.textFrameOptions"
            | "text.pathOptions"
            | "text.para"
            | "style.paragraph.add"
            | "style.cell.add"
            | "style.cell.edit"
            | "style.table.edit"
    ) {
        "two paths"
    } else if matches!(command, "xref.insert" | "xref.setFormat" | "variable.insert" | "condition.apply")
        && matches!(field.split(['.', '[']).next(), Some("xrefFormats" | "textVariables" | "conditions"))
    {
        "value naming another object"
    } else {
        "read"
    }
}

#[derive(Debug, Serialize)]
pub struct Projection {
    pub id: &'static str,
    pub error: Option<String>,
    pub changed_document: bool,
    pub changed_editor: bool,
    pub missing: Vec<String>,
    pub held_free: Vec<String>,
    pub missing_editor: Vec<String>,
    pub extra_editor: Vec<String>,
    pub notes: Vec<String>,
    pub under: Option<Under>,
}

impl Projection {
    fn new(id: &'static str) -> Self {
        Self {
            id,
            error: None,
            changed_document: false,
            changed_editor: false,
            missing: Vec::new(),
            held_free: Vec::new(),
            missing_editor: Vec::new(),
            extra_editor: Vec::new(),
            notes: Vec::new(),
            under: None,
        }
    }
}

/// `on` is supplied for an item-selection fallback, as an agent call would supply it.
/// Explicit targets and selected-text paths retain their own parameter/editor fixtures.
pub fn project(check: &Check) -> Projection {
    let mut report = Projection::new(check.id);
    if let Err(error) = project_inner(check, &mut report) {
        report.error = Some(error);
    }
    report
}

fn project_inner(check: &Check, report: &mut Projection) -> Result<(), String> {
    let mut fixture = Fixture::new(check.id, &mut report.notes)?;
    if matches!(check.id, "style.cell.add" | "style.cell.apply" | "style.cell.edit" | "style.table.apply" | "style.table.edit") {
        fixture.execute("table.select", r#"{"story":"$TABLESTORY","table":"$TABLE","row":0,"col":0}"#)?;
    }
    for (command, params) in check.setup {
        fixture.execute(command, params).map_err(|error| format!("setup {command}: {error}"))?;
    }
    fixture.finish_setup(check.id, &mut report.notes)?;
    let params = fixture.params(check.params)?;
    let spec =
        fixture.doc.commands().into_iter().find(|command| command.id == check.id).ok_or_else(|| format!("unregistered command {}", check.id))?;
    let under = fixture.doc.under(check.id, &params).map_err(|error| format!("under: {error}"))?;
    report.under = under.clone();
    let covered = if under.as_ref().is_some_and(|under| under.covers.is_some()) {
        fixture.doc.covered(check.id, &params, &[])?.map(|covered| covered.under).unwrap_or_default()
    } else {
        Vec::new()
    };
    let editor = fixture.doc.editor_state();
    let selected = selection_items(&editor);
    let on = if spec.reads.contains(&kition_kit::EditorState::Selection) && spec.targets.is_none_or(|target| params.get(target).is_none()) {
        selected
    } else {
        Vec::new()
    };
    if !on.is_empty() {
        report.notes.push(format!("item-selection fallback modeled with on={on:?}"));
    }
    let mut exchange = Exchange::new();
    let replica = exchange.replica(&mut fixture.doc).map_err(|error| error.to_string())?;
    let state = replica.state_json().map_err(|error| error.to_string())?;
    let guard = Guard::new(replica, &spec, &params, &on, under.as_ref(), &covered)?;
    for (role, fields) in check.required {
        let id = fixture.role(role)?;
        let id = guard.resolve(&id, &[]).ok_or_else(|| format!("required role {role} names no live object: {id}"))?;
        if fields.is_empty() && !guard.exists(&id) {
            report.missing.push(format!("{role}:existence"));
        }
        for field in *fields {
            match guard.held(check.id, &id, field, true) {
                Held::Direct => {}
                Held::Equivalent(reason) => report.notes.push(format!("{role}:{field}: {reason}")),
                Held::No => report.missing.push(format!("{role}:{field}")),
            }
        }
    }
    for (role, fields) in check.free {
        let id = fixture.role(role)?;
        let id = guard.resolve(&id, &[]).ok_or_else(|| format!("free role {role} names no live object: {id}"))?;
        for field in *fields {
            if *field == "*" {
                if guard.exists(&id) {
                    report.held_free.push(format!("{role}:{field}"));
                }
                continue;
            }
            match guard.held(check.id, &id, field, false) {
                Held::Direct | Held::Equivalent(_) => report.held_free.push(format!("{role}:{field}")),
                Held::No => {}
            }
        }
    }
    let expected: BTreeSet<_> = check.editor.iter().copied().collect();
    let declared: BTreeSet<_> = spec.reads.iter().map(|axis| axis.as_str()).collect();
    report.missing_editor = expected.difference(&declared).map(|axis| (*axis).to_owned()).collect();
    report.extra_editor = declared.difference(&expected).map(|axis| (*axis).to_owned()).collect();
    drop(guard);
    if kition_layout_kit::files::refuse_for_agent(check.id).is_err() {
        report.notes.push("agent gate blocks this handler; evaluated under, without running its file/program effects".to_owned());
        return Ok(());
    }
    let answer = fixture.doc.execute(check.id, params).map_err(|error| format!("primary path: {error}"))?;
    if check.id == "file.close" {
        report.changed_editor = true;
        return Ok(());
    }
    let after = Exchange::new().replica(&mut fixture.doc).map_err(|error| error.to_string())?.state_json().map_err(|error| error.to_string())?;
    report.changed_document = state != after;
    report.changed_editor = editor != fixture.doc.editor_state();
    if !report.changed_document && !report.changed_editor {
        report.notes.push(format!(
            "successful call changed neither canonical document nor captured editor state; answer type={}",
            if answer.is_object() { "object" } else { "other" }
        ));
    }
    Ok(())
}

fn selection_items(editor: &Value) -> Vec<Value> {
    editor.pointer("/state/selection/items").or_else(|| editor.pointer("/selection/items")).and_then(Value::as_array).cloned().unwrap_or_default()
}

struct Fixture {
    doc: LayoutDocument,
    tokens: BTreeMap<String, Value>,
    scratch: Option<std::path::PathBuf>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(path) = &self.scratch {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Fixture {
    fn new(command: &str, notes: &mut Vec<String>) -> Result<Self, String> {
        let mut doc = LayoutDocument::create("Independent.klayout", 612, 792)?;
        let mut tokens = BTreeMap::from([("$ROOT".to_owned(), json!("layout"))]);
        // A has siblings on both sides in stacking order; the horizontal gaps are unequal.
        for (role, rect) in [("$B", [140, 10, 230, 100]), ("$A", [10, 10, 100, 100]), ("$C", [300, 10, 390, 100])] {
            let answer = doc.execute("frame.add", json!({"rect":rect,"content":"unassigned","caret":false}))?;
            tokens.insert(role.to_owned(), member(&answer, "id")?);
        }
        let frame = doc.execute("frame.add", json!({"rect":[10,150,300,300],"content":"text","text":"Layout audit text.","caret":false}))?;
        tokens.insert("$FRAME".to_owned(), member(&frame, "id")?);
        tokens.insert("$STORY".to_owned(), member(&frame, "story")?);
        let other = doc.execute("layer.new", json!({"name":"Other"}))?;
        tokens.insert("$OTHER_LAYER".to_owned(), member(&other, "id")?);
        doc.execute("swatch.add", json!({"name":"Ink","color":"#108080"}))?;
        doc.execute("style.paragraph.add", json!({"name":"Body"}))?;
        doc.execute("style.character.add", json!({"name":"Em","chars":{"size":14}}))?;
        let table_frame = doc.execute("frame.add", json!({"rect":[10,350,400,650],"content":"text","text":"Table source","caret":false}))?;
        let table_story = member(&table_frame, "story")?;
        tokens.insert("$TABLEFRAME".to_owned(), member(&table_frame, "id")?);
        tokens.insert("$TABLESTORY".to_owned(), table_story.clone());
        doc.execute("text.select", json!({"story":table_story,"anchor":0,"focus":0}))?;
        let table = doc.execute("table.insert", json!({"rows":2,"cols":2}))?;
        tokens.insert("$TABLE".to_owned(), member(&table, "table")?);
        doc.execute("layout.pages.insert", json!({"count":3,"parent":null}))?;
        let layer = doc.inspect()?["spreads"][0]["items"][0]["layer"].clone();
        doc.execute("layer.activate", json!({"id":layer}))?;
        doc.execute("guide.add", json!({"spread":0,"page":0,"orientation":"vertical","position":20}))?;
        let mut exchange = Exchange::new();
        let replica = exchange.replica(&mut doc).map_err(|error| error.to_string())?;
        let ids = ToolIds::of_replica(replica);
        let root = ObjectId::from_string("layout");
        let spread = replica.list(&root, "spreads").map_err(|error| error.to_string())?.into_iter().next().ok_or("fixture has no spread")?;
        let page = replica.list(&spread, "pages").map_err(|error| error.to_string())?.into_iter().next().ok_or("fixture has no page")?;
        tokens.insert("$SPREAD".to_owned(), json!(ids.show(&spread)));
        tokens.insert("$PAGE".to_owned(), json!(ids.show(&page)));
        for (role, field, name) in [("$INK", "swatches", "Ink"), ("$BODY", "paragraphStyles", "Body"), ("$EM", "characterStyles", "Em")] {
            let object = replica
                .list(&root, field)
                .map_err(|error| error.to_string())?
                .into_iter()
                .find(|object| replica.get(object, "name").ok().as_ref() == Some(&json!(name)))
                .ok_or_else(|| format!("fixture has no {name}"))?;
            tokens.insert(role.to_owned(), json!(ids.show(&object)));
        }
        let next = replica.list(&root, "spreads").map_err(|error| error.to_string())?.get(1).cloned().ok_or("fixture has no next spread")?;
        let next_page = replica.list(&next, "pages").map_err(|error| error.to_string())?.into_iter().next().ok_or("fixture has no next page")?;
        tokens.insert("$NEXT_SPREAD".to_owned(), json!(ids.show(&next)));
        tokens.insert("$NEXT_PAGE".to_owned(), json!(ids.show(&next_page)));
        let parent = replica.list(&root, "parents").map_err(|error| error.to_string())?.into_iter().next().ok_or("fixture has no parent")?;
        let parent_page =
            replica.list(&parent, "pages").map_err(|error| error.to_string())?.into_iter().next().ok_or("fixture has no parent page")?;
        tokens.insert("$PARENT".to_owned(), json!(ids.show(&parent)));
        tokens.insert("$PARENT_PAGE".to_owned(), json!(ids.show(&parent_page)));
        let guide = replica.list(&page, "guides").or_else(|_| replica.list(&spread, "guides")).map_err(|error| error.to_string())?;
        let guide = guide.into_iter().next().ok_or("fixture has no guide")?;
        tokens.insert("$GUIDE".to_owned(), json!(ids.show(&guide)));
        let table_story = text(&table_story).and_then(|id| ids.resolve(&id, |id| replica.exists(id)).ok()).ok_or("fixture has no table story")?;
        let table = replica.list(&table_story, "tables").map_err(|error| error.to_string())?.into_iter().next().ok_or("fixture has no table")?;
        tokens.insert("$TABLE_OBJECT".to_owned(), json!(ids.show(&table)));
        let cells = replica.map_entries(&table, "cells").map_err(|error| error.to_string())?;
        let rows = replica.list(&table, "rows").map_err(|error| error.to_string())?;
        let columns = replica.list(&table, "columns").map_err(|error| error.to_string())?;
        for (field, prefix) in [("rows", "$ROW"), ("columns", "$COL")] {
            for (index, object) in replica.list(&table, field).map_err(|error| error.to_string())?.iter().enumerate() {
                tokens.insert(format!("{prefix}{index}"), json!(ids.show(object)));
            }
        }
        for (r, row) in rows.iter().enumerate() {
            for (c, column) in columns.iter().enumerate() {
                let coordinate = format!("{row}/{column}");
                let cell = cells.iter().find(|(key, _)| *key == coordinate).and_then(|(_, value)| value.as_str()).ok_or("fixture cell is absent")?;
                tokens.insert(format!("$CELL{r}{c}"), json!(ids.show(&ObjectId::from_string(cell))));
            }
        }
        let original =
            tokens.get("$A").and_then(|id| text(id).and_then(|id| ids.resolve(&id, |id| replica.exists(id)).ok())).ok_or("fixture has no A")?;
        let layer = replica.get(&original, "layer").map_err(|error| error.to_string())?;
        let layer = layer.as_str().map(ObjectId::from_string).ok_or("fixture A has no layer")?;
        let layer = ids.show(&layer);
        tokens.insert("$LAYER".to_owned(), json!(layer));
        doc.execute("layer.activate", json!({"id":layer}))?;
        let mut fixture = Self { doc, tokens, scratch: None };
        let png = kition_layout_render::Rendered { width: 2, height: 2, pixels: vec![90; 16] }.to_png();
        fixture.tokens.insert("$PNG".to_owned(), json!(kition_layout_engine::cmd::base64_encode(&png)));
        if command == "file.openBytes" {
            let bytes = fixture.doc.save(None)?;
            fixture.tokens.insert("$DOCUMENT_BYTES".to_owned(), json!(kition_layout_engine::cmd::base64_encode(&bytes)));
        }
        if command == "file.revert" {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "c8-layout-read-2-revert-{}-{}.klayout",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fixture.doc.execute("file.saveAs", json!({"path":path}))?;
            fixture.scratch = Some(path);
            fixture.execute("object.rename", r#"{"id":"$A","name":"Unsaved"}"#)?;
        }
        if command == "place.drop" {
            fixture.execute("place.load", r#"{"name":"input.png","base64":"$PNG"}"#)?;
        }
        fixture.execute("select.set", r#"{"ids":["$A"]}"#)?;
        if matches!(command, "text.fillWithPlaceholder" | "file.exportText") {
            fixture.execute("select.set", r#"{"ids":["$FRAME"]}"#)?;
        }
        if matches!(command, "object.fit" | "object.fittingOptions" | "object.clippingPath") {
            let pixels = (0..16 * 16)
                .flat_map(|index| if (3..13).contains(&(index % 16)) && (3..13).contains(&(index / 16)) { [40, 120, 90, 255] } else { [0, 0, 0, 0] })
                .collect();
            let png = kition_layout_render::Rendered { width: 16, height: 16, pixels }.to_png();
            let encoded = kition_layout_engine::cmd::base64_encode(&png);
            let frame = fixture.tokens.get("$A").cloned().ok_or("fixture has no A")?;
            fixture.doc.execute("file.place", json!({"frame":frame,"name":"audit.png","base64":encoded}))?;
            if command == "object.fit" {
                fixture.execute("object.fittingOptions", r#"{"ids":["$A"],"crop":[3,2,4,1]}"#)?;
            } else if command == "object.fittingOptions" {
                fixture.execute("object.fittingOptions", r#"{"ids":["$A"],"autoFit":false}"#)?;
            }
            notes.push("fixture repair after review: A holds an original generated PNG, exercising graphic fitting/clipping".to_owned());
        }
        if command == "transform.clear" {
            fixture.execute("transform.rotate", r#"{"ids":["$A"],"angle":15}"#)?;
            notes.push("fixture repair after review: clear starts with a nonidentity transform".to_owned());
        }
        if command == "object.matchAttributes" {
            fixture.execute("object.opacity", r#"{"ids":["$B"],"opacity":0.75}"#)?;
            notes.push("fixture repair after review: source B has different attributes from target A".to_owned());
        }
        if command == "conveyor.clear" {
            fixture.execute("conveyor.collect", r#"{"ids":["$A"]}"#)?;
            notes.push("fixture repair after review: the conveyor contains an item before clear".to_owned());
        }
        if matches!(command, "object.unlockAll" | "object.showAll") {
            let key = if command == "object.unlockAll" { "locked" } else { "hidden" };
            let ids = ["$A", "$B", "$C", "$FRAME"].into_iter().map(|role| fixture.role(role)).collect::<Result<Vec<_>, _>>()?;
            fixture.doc.execute("object.setFlags", json!({"ids":ids,key:true}))?;
            notes.push(format!("fixture repair after review: all fixture items start with {key}=true"));
        }
        if command == "path.close" {
            let path = fixture.doc.execute("path.add", json!({"anchors":[{"p":[10,10]},{"p":[100,10]},{"p":[100,100]}]}))?;
            fixture.tokens.insert("$A".to_owned(), member(&path, "id")?);
            notes.push("fixture repair after review: path.close uses an open path".to_owned());
        }
        if command == "object.releaseCompoundPath" {
            let extra = fixture.doc.execute("frame.add", json!({"rect":[30,30,50,50],"caret":false}))?;
            let a = fixture.tokens.get("$A").cloned().ok_or("fixture has no A")?;
            fixture.doc.execute("object.makeCompoundPath", json!({"ids":[a,member(&extra,"id")?]}))?;
            notes.push("fixture repair after review: releaseCompoundPath has two subpaths; B remains unrelated".to_owned());
        }
        if matches!(command, "select.container" | "select.content" | "select.nextInGroup" | "select.previousInGroup") {
            fixture.execute("arrange.group", r#"{"ids":["$A","$B"]}"#)?;
            if command != "select.content" {
                let role = if command == "select.previousInGroup" { "$B" } else { "$A" };
                let id = fixture.tokens.get(role).cloned().ok_or("fixture has no selected child")?;
                fixture.doc.execute("select.set", json!({"ids":[id]}))?;
            }
            notes.push("fixture repair after review: group navigation uses a real two-child group".to_owned());
        } else if !matches!(command, "object.releaseCompoundPath" | "path.close" | "text.fillWithPlaceholder" | "file.exportText") {
            fixture.execute("select.set", r#"{"ids":["$A"]}"#)?;
        }
        if matches!(
            command,
            "book.new"
                | "book.open"
                | "book.close"
                | "book.add"
                | "book.remove"
                | "book.styleSource"
                | "book.paginate"
                | "book.syncStyles"
                | "book.exportPdf"
                | "data.source.update"
                | "linkedFile.relink"
                | "linkedFile.relinkFolder"
                | "linkedFile.update"
                | "linkedFile.embed"
                | "linkedFile.unembed"
                | "linkedFile.goTo"
                | "object.layerOptions"
        ) {
            misc_files(&mut fixture, command, notes)?;
        }
        Ok(fixture)
    }

    fn role(&self, role: &str) -> Result<Value, String> {
        let role = if role == "$TABLE" { "$TABLE_OBJECT" } else { role };
        if role.starts_with('$') {
            self.tokens.get(role).cloned().ok_or_else(|| format!("unknown role {role}; available: {:?}", self.tokens.keys()))
        } else {
            Ok(json!(role))
        }
    }

    fn params(&self, input: &str) -> Result<Value, String> {
        fn fill(value: &mut Value, tokens: &BTreeMap<String, Value>) -> Result<(), String> {
            match value {
                Value::String(token) if token.starts_with('$') => {
                    *value = tokens.get(token).cloned().ok_or_else(|| format!("unknown token {token}"))?
                }
                Value::Array(items) => {
                    for item in items {
                        fill(item, tokens)?;
                    }
                }
                Value::Object(members) => {
                    for member in members.values_mut() {
                        fill(member, tokens)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }
        let mut value = serde_json::from_str(input).map_err(|error| format!("invalid fixture JSON: {error}"))?;
        fill(&mut value, &self.tokens)?;
        if value.get("base64").and_then(Value::as_str) == Some("") {
            let token = match value.get("name").and_then(Value::as_str) {
                Some(name) if name.ends_with(".png") => Some("$PNG"),
                Some(name) if name.ends_with(".klayout") => Some("$DOCUMENT_BYTES"),
                _ => None,
            };
            if let Some(bytes) = token.and_then(|token| self.tokens.get(token)) {
                value["base64"] = bytes.clone();
            }
        }
        Ok(value)
    }

    fn execute(&mut self, command: &str, input: &str) -> Result<Value, String> {
        self.doc.execute(command, self.params(input)?)
    }

    fn finish_setup(&mut self, command: &str, notes: &mut Vec<String>) -> Result<(), String> {
        if matches!(command, "layout.overrideParentItems" | "layout.removeOverrides" | "layout.detachAll") {
            self.execute("frame.add", r#"{"spread":"A","rect":[20,20,70,70],"content":"unassigned","caret":false}"#)?;
            if command != "layout.overrideParentItems" {
                let answer = self.execute("layout.overrideParentItems", r#"{"page":0}"#)?;
                let copy = answer.get("ids").and_then(Value::as_array).and_then(|ids| ids.first()).cloned().ok_or("fixture made no override copy")?;
                self.tokens.insert("$A".to_owned(), copy);
            }
            notes.push("fixture provides a real parent item and, for removal/detach, its override copy".to_owned());
        }
        if matches!(command, "style.cell.add" | "style.cell.apply" | "style.cell.edit" | "style.table.apply" | "style.table.edit") {
            self.execute("table.select", r#"{"story":"$TABLESTORY","table":"$TABLE","row":0,"col":0}"#)?;
            notes.push("fixture supplies the table caret used by style handlers".to_owned());
        }
        if command == "story.updateLink" {
            self.execute("story.setText", r#"{"story":"$STORY","text":"Revised layout audit text."}"#)?;
            notes.push("fixture repair after review: parent text changes after link creation so updateLink replaces child content".to_owned());
        }
        if command == "story.unlink" {
            let linked = self.execute("story.placeAndLink", r#"{"story":"$STORY","rect":[320,150,500,300]}"#)?;
            self.tokens.insert("$STORY".to_owned(), member(&linked, "story")?);
            notes.push("fixture repair after review: story.unlink targets a linked child".to_owned());
        }
        if matches!(
            command,
            "data.placeholder.remove" | "link.delete" | "link.edit" | "link.goToSource" | "state.show" | "state.rename" | "state.release"
        ) {
            misc_records(self, command, notes)?;
        }
        Ok(())
    }
}

/// Misc projections: a session book of two saved chapters, a linked data file, and a linked or
/// placed-PDF graphic on A. Files go to one bounded scratch folder that later runs overwrite.
fn misc_files(fixture: &mut Fixture, command: &str, notes: &mut Vec<String>) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join("c8-layout-misc-fixtures").join(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed).to_string());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
    let write = |name: &str, bytes: &[u8]| std::fs::write(dir.join(name), bytes).map_err(|error| format!("{name}: {error}"));
    fixture.tokens.insert("$SCRATCH_DIR".to_owned(), json!(dir.to_string_lossy()));
    let png =
        |shade: u8, side: u32| kition_layout_render::Rendered { width: side, height: side, pixels: vec![shade; (side * side * 4) as usize] }.to_png();
    if command.starts_with("book.") {
        let bytes = fixture.doc.save(None)?;
        for (token, name) in [("$CHAPTER_A", "a.klayout"), ("$CHAPTER_B", "b.klayout"), ("$CHAPTER_C", "c.klayout")] {
            write(name, &bytes)?;
            fixture.tokens.insert(token.to_owned(), json!(path(name)));
        }
        fixture.tokens.insert("$BOOK_PATH".to_owned(), json!(path("book.dcbook")));
        fixture.tokens.insert("$NEW_BOOK_PATH".to_owned(), json!(path("new.dcbook")));
        fixture.execute("book.new", r#"{"path":"$BOOK_PATH"}"#)?;
        fixture.execute("book.add", r#"{"path":"$CHAPTER_A"}"#)?;
        fixture.execute("book.add", r#"{"path":"$CHAPTER_B"}"#)?;
        notes.push("misc fixture: a session book of two saved chapters (copies of this document)".to_owned());
    }
    if command == "data.source.update" {
        write("data.csv", b"name,qty\nAda,1\nBob,2\n")?;
        fixture.tokens.insert("$CSV_PATH".to_owned(), json!(path("data.csv")));
        fixture.execute("data.source.select", r#"{"path":"$CSV_PATH"}"#)?;
        write("data.csv", b"name,qty\nAda,1\nBob,2\nCy,3\n")?;
        notes.push("misc fixture: a linked CSV source whose file gained a row after it was read".to_owned());
    }
    if command.starts_with("linkedFile.") || command == "object.layerOptions" {
        let a = fixture.role("$A")?;
        if command == "object.layerOptions" {
            let pdf = fixture.doc.execute("file.exportPdf", json!({}))?;
            let pdf = member(&pdf, "base64")?;
            fixture.doc.execute("file.place", json!({"frame":a,"base64":pdf,"name":"art.pdf"}))?;
            notes.push("misc fixture: A shows a placed PDF exported from this document".to_owned());
        } else {
            write("link.png", &png(90, 2))?;
            fixture.doc.execute("file.place", json!({"frame":a,"path":path("link.png")}))?;
            write("other.png", &png(200, 4))?;
            fixture.tokens.insert("$RELINK_PATH".to_owned(), json!(path("other.png")));
            fixture.tokens.insert("$LINK_DIR".to_owned(), json!(dir.to_string_lossy()));
            if command == "linkedFile.update" {
                write("link.png", &png(30, 3))?;
            }
            notes.push("misc fixture: A shows a graphic linked to a file (modified on disk for linkedFile.update)".to_owned());
        }
        let mut exchange = Exchange::new();
        let replica = exchange.replica(&mut fixture.doc).map_err(|error| error.to_string())?;
        let ids = ToolIds::of_replica(replica);
        let asset = replica.list(&ObjectId::from_string("layout"), "assets").map_err(|error| error.to_string())?;
        let asset = asset.into_iter().last().ok_or("misc fixture placed no asset")?;
        fixture.tokens.insert("$ASSET".to_owned(), json!(ids.show(&asset)));
        fixture.execute("select.set", r#"{"ids":["$A"]}"#)?;
    }
    Ok(())
}

/// Misc projections whose parameters name a settings record created by an earlier step.
fn misc_records(fixture: &mut Fixture, command: &str, notes: &mut Vec<String>) -> Result<(), String> {
    if command == "data.placeholder.remove" {
        fixture.execute("data.source.select", r#"{"base64":"bmFtZSxxdHkKQWRhLDEKQm9iLDIK","name":"data.csv"}"#)?;
        let answer = fixture.execute("data.placeholder.add", r#"{"field":"name","story":"$STORY","at":0}"#)?;
        fixture.tokens.insert("$PLACEHOLDER".to_owned(), member(&answer, "id")?);
        notes.push("misc fixture: a text placeholder at the start of STORY".to_owned());
    }
    if matches!(command, "link.delete" | "link.edit" | "link.goToSource") {
        let answer = fixture.execute("link.add", r#"{"url":"https://example.com","ids":["$B"]}"#)?;
        let id = answer.get("ids").and_then(Value::as_array).and_then(|ids| ids.first()).cloned().ok_or("misc fixture made no hyperlink")?;
        fixture.tokens.insert("$HYPERLINK".to_owned(), id);
        notes.push("misc fixture: a hyperlink whose source is B".to_owned());
    }
    if matches!(command, "state.show" | "state.rename" | "state.release") {
        let answer = fixture.execute("state.add", r#"{"ids":["$A","$B"]}"#)?;
        fixture.tokens.insert("$STATES".to_owned(), member(&answer, "id")?);
        fixture.execute("select.set", r#"{"ids":["$STATES"]}"#)?;
        notes.push("misc fixture: A and B are the two states of a multi-state group".to_owned());
    }
    Ok(())
}

fn member(value: &Value, field: &str) -> Result<Value, String> {
    value.get(field).filter(|value| !value.is_null()).cloned().ok_or_else(|| format!("fixture answer has no {field}: {value}"))
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

#[derive(Clone)]
struct Need {
    fields: Option<Vec<String>>,
    positions: Option<Vec<String>>,
}

enum Held {
    Direct,
    Equivalent(String),
    No,
}

struct Guard<'a> {
    replica: &'a Replica,
    schema: &'a Schema,
    ids: ToolIds,
    needs: BTreeMap<ObjectId, Need>,
    owned: BTreeMap<ObjectId, Vec<(ObjectId, String)>>,
    covered: BTreeMap<ObjectId, Vec<String>>,
    under_covers: Vec<String>,
    params: &'a Value,
}

impl<'a> Guard<'a> {
    fn new(
        replica: &'a Replica,
        spec: &KitCommand,
        params: &'a Value,
        on: &[Value],
        under: Option<&'a Under>,
        covered: &[Value],
    ) -> Result<Self, String> {
        let mut guard = Self {
            replica,
            schema: replica.schema(),
            ids: ToolIds::of_replica(replica),
            needs: BTreeMap::new(),
            owned: BTreeMap::new(),
            covered: BTreeMap::new(),
            under_covers: under.and_then(|under| under.covers.as_ref()).map(|covers| covers.fields.clone()).unwrap_or_default(),
            params,
        };
        if let Some(covers) = under.and_then(|under| under.covers.as_ref()) {
            for value in covered {
                if let Some(id) = guard.resolve(value, &[]) {
                    guard.covered.insert(id, covers.fields.clone());
                }
            }
        }
        for id in on.iter().chain(spec.targets.and_then(|key| params.get(key)).into_iter().flat_map(values)) {
            if let Some(id) = guard.resolve(id, &[]) {
                guard.needs.insert(id, Need { fields: None, positions: spec.position_independent.then(Vec::new) });
            }
        }
        // The SDK scans strings at one level, typed numeric parameters and declared nested
        // object paths. A place parameter is excluded. Kit premises replace the default later.
        for (key, value) in params.as_object().into_iter().flatten() {
            if spec.targets == Some(key.as_str()) || spec.places.contains(&key.as_str()) {
                continue;
            }
            let declared = spec.objects.contains(&key.as_str());
            let types = spec.object_types.iter().find(|(path, _)| *path == key).map_or(&[][..], |(_, types)| *types);
            for candidate in shallow(value).into_iter().filter(|value| value.is_string() || (declared && value.is_u64())) {
                if let Some(id) = guard.resolve(candidate, types) {
                    guard.needs.entry(id).or_insert(Need { fields: None, positions: None });
                }
            }
        }
        for path in spec.objects.iter().filter(|path| path.contains('.') || path.contains("[]")) {
            let types = spec.object_types.iter().find(|(parameter, _)| parameter == path).map_or(&[][..], |(_, types)| *types);
            for candidate in at_path(params, path) {
                if let Some(id) = guard.resolve(candidate, types) {
                    guard.needs.entry(id).or_insert(Need { fields: None, positions: None });
                }
            }
        }
        if let Some(under) = under {
            for id in &under.objects {
                if let Some(id) = guard.resolve(id, &[]) {
                    guard.needs.entry(id).or_insert(Need { fields: None, positions: None });
                }
            }
            for premise in &under.premises {
                let id = guard.resolve(&premise.id, &[]).ok_or_else(|| format!("premise names no object: {}", premise.id))?;
                guard.needs.insert(id, Need { fields: premise.fields.clone(), positions: premise.positions.clone() });
            }
        }
        for id in guard.needs.keys() {
            let mut pending = vec![(id.clone(), String::new())];
            let mut seen = BTreeSet::from([id.clone()]);
            let mut children = Vec::new();
            while let Some((owner, prefix)) = pending.pop() {
                let ty = replica.object_type(&owner).map_err(|error| error.to_string())?;
                let definition = guard.schema.types.get(&ty).ok_or_else(|| format!("unknown type {ty}"))?;
                for (field, definition) in &definition.fields {
                    if !definition.owning() {
                        continue;
                    }
                    let path = if prefix.is_empty() { field.clone() } else { format!("{prefix}.{field}") };
                    let held: Vec<_> = match definition.kind {
                        FieldKind::List => replica
                            .list(&owner, field)
                            .map_err(|error| error.to_string())?
                            .into_iter()
                            .enumerate()
                            .map(|(index, id)| (id, index.to_string()))
                            .collect(),
                        FieldKind::Tree => replica
                            .tree(&owner, field)
                            .map_err(|error| error.to_string())?
                            .into_iter()
                            .enumerate()
                            .map(|(index, entry)| (entry.object, index.to_string()))
                            .collect(),
                        FieldKind::Ref => replica
                            .get(&owner, field)
                            .map_err(|error| error.to_string())?
                            .as_str()
                            .map(|id| vec![(ObjectId::from_string(id), String::new())])
                            .unwrap_or_default(),
                        FieldKind::Map => {
                            let map = Value::Object(replica.map_entries(&owner, field).map_err(|error| error.to_string())?.into_iter().collect());
                            kition_kit::kdm::FieldDef::map_held(&map, |id| replica.exists(&ObjectId::from_string(id)))
                                .into_iter()
                                .map(|(id, key)| (ObjectId::from_string(id), key))
                                .collect()
                        }
                        _ => Vec::new(),
                    };
                    for (child, place) in held {
                        if !seen.insert(child.clone()) {
                            continue;
                        }
                        let path = if place.is_empty() { path.clone() } else { format!("{path}.{place}") };
                        children.push((child.clone(), path.clone()));
                        pending.push((child, path));
                    }
                }
            }
            guard.owned.insert(id.clone(), children);
        }
        Ok(guard)
    }

    fn resolve(&self, value: &Value, types: &[&str]) -> Option<ObjectId> {
        let value = text(value)?;
        let id = self.ids.resolve(&value, |id| self.replica.exists(id)).ok()?;
        (types.is_empty() || self.replica.object_type(&id).ok().is_some_and(|ty| types.contains(&ty.as_str()))).then_some(id)
    }

    fn exists(&self, id: &ObjectId) -> bool {
        self.needs.contains_key(id)
            || self.owned.iter().any(|(owner, children)| {
                children.iter().any(|(child, path)| child == id && self.needs.get(owner).is_some_and(|need| any_overlap(&need.fields, path)))
            })
    }

    fn held(&self, command: &str, id: &ObjectId, field: &str, required: bool) -> Held {
        let ty = self.replica.object_type(id).ok();
        // The blind reader used the engine story spelling; the model stores link in options.
        let normalized = if ty.as_deref() == Some("Story") { field.strip_prefix("props.").map(|path| format!("options.{path}")) } else { None };
        let field = normalized.as_deref().unwrap_or(field);
        if field == "*" {
            if self.needs.get(id).is_some_and(|need| need.fields.is_none()) || self.held_by_owner(id, None, required) {
                return Held::Direct;
            }
            return Held::No;
        }
        if field == "@position" {
            if self.needs.get(id).is_some_and(|need| {
                need.positions.as_ref().is_none_or(|positions| {
                    positions.iter().any(|field| matches!(field.as_str(), "items" | "spreads" | "parents" | "layers" | "pages" | "spread"))
                })
            }) || self.held_by_owner(id, None, required)
            {
                return Held::Direct;
            }
            return Held::No;
        }
        let position = ty
            .as_ref()
            .and_then(|ty| self.schema.types.get(ty))
            .and_then(|ty| ty.fields.get(field.split('.').next().unwrap_or(field)))
            .is_some_and(|field| field.position);
        if self.needs.get(id).is_some_and(|need| {
            let held = if position { &need.positions } else { &need.fields };
            if required { covers(held, field) } else { any_overlap(held, field) }
        }) || self.held_by_owner(id, Some(field), required)
        {
            return normalized.as_ref().map_or(Held::Direct, |field| Held::Equivalent(format!("engine story property maps to model {field}")));
        }
        if self.covered.get(id).is_some_and(|fields| {
            let held = Some(fields.clone());
            if required { covers(&held, field) } else { any_overlap(&held, field) }
        }) {
            return Held::Equivalent("covered() includes this object at the declared field; text is the addressed offset prefix/range".to_owned());
        }
        if required
            && command == "edit.replaceObjects"
            && matches!(field, "content" | "shape")
            && self.covered.get(id).is_some_and(|fields| fields.iter().any(|field| field == "~queryKind"))
        {
            return Held::Equivalent("the query reads kind derived from content/shape (cmd/find.rs:298), held by covers ~queryKind".to_owned());
        }
        if required
            && command == "style.cell.add"
            && ty.as_deref() == Some("Cell")
            && matches!(field, "props.fill" | "props.insets" | "props.strokes" | "text")
            && self.has_field(id, "~styleSource")
        {
            return Held::Equivalent(
                "cell_style_create copies formatting, stroke[0] and first paragraph style, held by the cell's ~styleSource (cmd/table.rs:1106-1111)"
                    .to_owned(),
            );
        }
        if required {
            let named = match (command, field) {
                ("style.compositeFont.set", "settings.compositeFonts")
                | ("xref.defineFormat", "xrefFormats")
                | ("condition.options" | "condition.delete", "conditions")
                | ("swatch.ungroupColorGroup", "settings.colorGroups")
                | ("list.define" | "list.delete", "setup.lists") => Some(("name", "")),
                ("variable.define", "textVariables")
                    if self.params.get("name").and_then(Value::as_str).is_some_and(|name| {
                        self.replica.list(id, "textVariables").ok().is_some_and(|vars| {
                            vars.iter().any(|v| self.replica.get(v, "name").ok().and_then(|n| n.as_str().map(str::to_owned)).as_deref() == Some(name))
                        })
                    }) =>
                {
                    Some(("name", ""))
                }
                ("condition.apply", "conditions") => Some(("name", ".name")),
                ("xref.insert" | "xref.setFormat", "xrefFormats") => Some(("format", ".name")),
                _ => None,
            };
            if let Some((key, suffix)) = named {
                let name =
                    self.params.get(key).and_then(Value::as_str).or_else(|| (command == "xref.insert").then_some("Full Paragraph & Page Number"));
                if let Some(name) = name {
                    let selected = format!("{field}[name={}]{suffix}", json!(name));
                    if self.needs.get(id).is_some_and(|need| covers(&need.fields, &selected)) {
                        return Held::Equivalent(format!("handler reads named entry {selected}, not unrelated entries"));
                    }
                }
            }
            if command == "swatch.renameColorGroup" && field == "settings.colorGroups" {
                let held = ["name", "to"].into_iter().all(|key| {
                    self.params
                        .get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|name| self.has_field(id, &format!("{field}[name={}].name", json!(name.trim()))))
                });
                if held {
                    return Held::Equivalent("rename reads the old and new names, not other groups' membership".to_owned());
                }
            }
        }
        if command == "ink.options" && field == "settings.inks.allToProcess" && self.has_field(id, "~inkAllToProcess") {
            return Held::Equivalent("effective allToProcess includes the absent false default".to_owned());
        }
        // A `within` window follows the named slots, not every future member of its list.
        // Even a window spanning the current list cannot satisfy a whole-list requirement.
        if required
            && self.has_composition_for(id)
            && ((field == "setup.baselineGrid" && command == "frame.fitText")
                || (ty.as_deref() == Some("Item")
                    && matches!(field, "path" | "xf" | "textOptions")
                    && matches!(command, "text.placeCaret" | "text.release" | "text.extendTo" | "text.selectWord" | "text.move" | "frame.fitText")))
        {
            return Held::Equivalent("composition reads effective frame area/options/grid, held by ~compositionFrames".to_owned());
        }
        if ty.as_deref() == Some("Layout") && field == "layers" {
            if command == "layer.new" && self.has_field(id, "~layerCount") {
                return Held::Equivalent("new-layer naming/color reads count, held by ~layerCount".to_owned());
            }
            // C8-layout-read-3: deletion refuses the last layer (cmd/layout.rs:513); a
            // background goes on the last stored layer (cmd/background.rs:113).
            if command == "layer.delete" && self.has_field(id, "~layerCount") {
                return Held::Equivalent("last-layer refusal reads count, held by ~layerCount".to_owned());
            }
            if command == "page.background" && self.has_field(id, "~lastLayer") {
                return Held::Equivalent("background layer is the last stored layer, held by ~lastLayer".to_owned());
            }
            if command == "layer.deleteUnused" && self.has_field(id, "~layerIds") && self.has_field(id, "~usedLayerIds") {
                return Held::Equivalent("unused-layer decision uses identity and used-layer sets".to_owned());
            }
        }
        // C8-layout-read-3: arrange reads "the containing spread's item order" (row, objects.rs:107),
        // the item tree's order among the target's siblings, not every item's content.
        if required
            && ty.as_deref() == Some("Layout")
            && field == "items"
            && matches!(command, "object.arrange" | "arrange.bringToFront" | "arrange.bringForward" | "arrange.sendBackward" | "arrange.sendToBack")
            && self.stacking_held(id)
        {
            return Held::Equivalent("stacking order is each sibling's place in the item tree (positions items)".to_owned());
        }
        // C8-layout-read-3: a `covers` place on an item field holds every item's membership by
        // that field: covered() is asked for any item another writer changes there.
        if command == "xml.deleteTag" && ty.as_deref() == Some("Item") && self.under_covers.iter().any(|held| held == field) {
            return Held::Equivalent("membership by this field is the command's covers place".to_owned());
        }
        if command == "layer.deleteUnused" && ty.as_deref() == Some("Item") && field == "layer" {
            let root = ObjectId::from_string("layout");
            if self.has_field(&root, "~usedLayerIds") {
                return Held::Equivalent("unused-layer decision uses the aggregate ~usedLayerIds".to_owned());
            }
        }
        if matches!(command, "guide.move" | "guide.delete")
            && ty.as_deref() == Some("Layer")
            && matches!(field, "locked" | "visible" | "props.showGuides")
            && self.has_field(id, "~guideEditable")
        {
            return Held::Equivalent("guide protection is the effective ~guideEditable predicate".to_owned());
        }
        Held::No
    }

    fn held_by_owner(&self, id: &ObjectId, field: Option<&str>, required: bool) -> bool {
        self.owned.iter().any(|(owner, children)| {
            children.iter().any(|(child, path)| {
                child == id
                    && self.needs.get(owner).is_some_and(|need| {
                        let path = field.map_or_else(|| path.clone(), |field| format!("{path}.{field}"));
                        if required { covers(&need.fields, &path) } else { any_overlap(&need.fields, &path) }
                    })
            })
        })
    }

    fn has_composition(&self) -> bool {
        self.needs.values().any(|need| need.fields.as_ref().is_some_and(|fields| fields.iter().any(|field| field.starts_with("~compositionFrames"))))
    }

    fn has_composition_for(&self, id: &ObjectId) -> bool {
        if self.replica.object_type(id).ok().as_deref() == Some("Layout") {
            return self.has_composition();
        }
        self.needs.iter().any(|(story, need)| {
            need.fields.as_ref().is_some_and(|fields| fields.iter().any(|field| field == "~compositionFrames"))
                && self.replica.object_type(story).ok().as_deref() == Some("Story")
                && self.replica.list(story, "frames").ok().is_some_and(|frames| frames.contains(id))
        })
    }

    /// Every top-level item on a spread holding a target whose tree place is held, is held at
    /// its tree place.
    fn stacking_held(&self, root: &ObjectId) -> bool {
        let Ok(tree) = self.replica.tree(root, "items") else { return false };
        let place = |id: &ObjectId| {
            self.needs.get(id).is_some_and(|need| need.positions.as_ref().is_none_or(|positions| positions.iter().any(|p| p == "items")))
        };
        let spread = |id: &ObjectId| self.replica.get(id, "spread").ok();
        let top: Vec<_> = tree.iter().filter(|entry| entry.parent.is_none()).map(|entry| entry.object.clone()).collect();
        let spreads: Vec<_> = top.iter().filter(|id| place(id)).filter_map(&spread).collect();
        !spreads.is_empty() && top.iter().filter(|id| spread(id).is_some_and(|s| spreads.contains(&s))).all(place)
    }

    fn has_field(&self, id: &ObjectId, field: &str) -> bool {
        self.needs.get(id).is_some_and(|need| any_overlap(&need.fields, field))
    }
}

fn values(value: &Value) -> Vec<&Value> {
    value.as_array().map_or_else(|| vec![value], |items| items.iter().collect())
}

fn shallow(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items
            .iter()
            .flat_map(|item| match item {
                Value::Object(members) => members.values().collect(),
                other => vec![other],
            })
            .collect(),
        Value::Object(members) => members.values().collect(),
        other => vec![other],
    }
}

fn at_path<'a>(value: &'a Value, path: &str) -> Vec<&'a Value> {
    let mut found = vec![value];
    for segment in path.split('.') {
        let (key, array) = segment.strip_suffix("[]").map_or((segment, false), |key| (key, true));
        found = found.into_iter().filter_map(|value| value.get(key)).flat_map(|value| if array { values(value) } else { vec![value] }).collect();
    }
    found
}

fn any_overlap(fields: &Option<Vec<String>>, wanted: &str) -> bool {
    fields.as_ref().is_none_or(|fields| fields.iter().any(|held| overlaps(held, wanted)))
}

fn covers(fields: &Option<Vec<String>>, wanted: &str) -> bool {
    fields.as_ref().is_none_or(|fields| {
        fields.iter().any(|held| held == wanted || wanted.strip_prefix(held).is_some_and(|rest| rest.starts_with('.') || rest.starts_with('[')))
    })
}

fn overlaps(a: &str, b: &str) -> bool {
    a == b
        || a.strip_prefix(b).is_some_and(|rest| rest.starts_with('.') || rest.starts_with('['))
        || b.strip_prefix(a).is_some_and(|rest| rest.starts_with('.') || rest.starts_with('['))
}
