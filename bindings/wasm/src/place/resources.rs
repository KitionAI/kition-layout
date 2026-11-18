use super::*;
use kition_kit::kdm::{ObjectId, tool_ids::ToolIds};

impl LayoutDocument {
    pub(super) fn resource_premises(&self, command: &str, params: &Value, under: &mut Under) -> Result<(), String> {
        let state = self.active()?;
        let doc = &state.doc;
        if matches!(command, "toc.generate" | "toc.update" | "index.generate" | "index.update") {
            let generated = if command.starts_with("toc.") { "settings.toc" } else { "settings.index" };
            under.premises.push(Premise::on("layout").fields(["stories", "sections", generated]).positions(Vec::<String>::new()));
            // Page labels and composition determine generated entries and their order. Hold
            // geometry rather than item paint, so unrelated opacity changes remain admissible.
            for spread in &doc.spreads {
                under.premises.push(Premise::on(spread.id.0).fields(["pages"]).positions(["spreads"]));
            }
            for id in doc.all_items() {
                let Some(item) = doc.item(id) else { continue };
                if let Content::Text(frame) = &item.content {
                    under.premises.push(Premise::on(id.0).fields(["path", "content", "textOptions"]).positions(["xf", "items", "spread"]));
                    self.composition_premises(frame.story, under)?;
                }
            }
        }
        if command == "document.applyTemplate" {
            under.premises.push(Premise::on("layout"));
        }
        if command == "font.replace" {
            let mut fields = vec!["stories"];
            if params.get("redefineStyles").and_then(Value::as_bool).unwrap_or(true) {
                fields.extend(["paragraphStyles", "characterStyles", "settings.compositeFonts"]);
            }
            under.premises.push(Premise::on("layout").fields(fields).positions(Vec::<String>::new()));
        }
        if matches!(command, "find.objects" | "find.changeObjects")
            && let Some(layer) = params
                .get("layer")
                .and_then(|value| doc.layers.iter().find(|layer| value.as_u64() == Some(layer.id.0) || value.as_str() == Some(layer.name.as_str())))
        {
            under.premises.push(Premise::on(layer.id.0).fields(["name"]).positions(Vec::<String>::new()));
        }
        let source = matches!(
            command,
            "edit.copy"
                | "edit.cut"
                | "edit.clear"
                | "object.content"
                | "edit.duplicate"
                | "edit.stepAndRepeat"
                | "type.createOutlines"
                | "library.add"
                | "conveyor.collect"
        ) || command == "transform.move" && params.get("copy").and_then(Value::as_bool).unwrap_or(false);
        let mut items = Vec::new();
        if command == "layout.pages.delete" {
            for page in params.get("pages").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64) {
                let Some((si, pi)) = usize::try_from(page).ok().and_then(|page| doc.page_loc(page)) else { continue };
                items.extend(doc.page_items(si, pi));
            }
        }
        // An alternate layout's deletion deletes its pages' items; its creation copies them.
        for page in super::pages::alternate_pages(doc, command, params) {
            let Some((si, pi)) = doc.page_loc(page) else { continue };
            items.extend(doc.page_items(si, pi));
        }
        if command == "layer.delete"
            && let Some(layer) = params.get("id").and_then(Value::as_u64)
        {
            // The layer's content including `~layerItems`, and each item it deletes; the items
            // on it are also a place (`members::Members::Layer`), told after a write on this
            // server too.
            under.premises.push(Premise::on(layer));
            items.extend(doc.layer_items(kition_layout_engine::doc::LayerId(layer)));
        }
        if command == "object.setLayer"
            && let Some(layer) = params.get("layer").and_then(Value::as_u64)
        {
            // Moving items onto a layer rests on its stored fields, not on what else is on it.
            let schema = kition_layout_kdm::schema();
            let stored = schema
                .types
                .get("Layer")
                .map(|layer| layer.fields.keys().cloned().collect::<Vec<_>>())
                .ok_or("the object model has no Layer type")?;
            under.premises.push(Premise::on(layer).fields(stored).positions(Vec::<String>::new()));
        }
        if command == "frame.thread" {
            items.extend(["from", "to"].into_iter().filter_map(|key| params.get(key).and_then(Value::as_u64).map(ItemId)));
        }
        if matches!(command, "frame.unthread" | "frame.loadText") {
            items.extend(params.get("frame").and_then(Value::as_u64).map(ItemId));
        }
        if command == "type.pathOptions" && params.get("delete").and_then(Value::as_bool).unwrap_or(false) {
            items.extend(params.get("id").and_then(Value::as_u64).map(ItemId).or_else(|| {
                state.selection.items.first().copied().or_else(|| state.selection.text.and_then(|t| doc.story(t.story)?.frames.first().copied()))
            }));
        }
        if source {
            items.extend(
                params
                    .get("ids")
                    .and_then(Value::as_array)
                    .map(|ids| ids.iter().filter_map(Value::as_u64).map(ItemId).collect::<Vec<_>>())
                    .unwrap_or_else(|| state.selection.items.clone()),
            );
        }
        if command == "layout.pages.duplicateSpread" {
            let si = params.get("spread").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
            if let Some(spread) = doc.spreads.get(si) {
                items.extend(spread.items.iter().map(|item| item.id));
            }
        }
        if command == "layout.overrideParentItems" {
            let page = params
                .get("page")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .or_else(|| state.selection.items.first().and_then(|id| doc.page_of_item(*id)))
                .unwrap_or(0);
            if let Some((sources, _)) = kition_layout_engine::cmd::parent_items(doc, page) {
                let wanted = params.get("ids").and_then(Value::as_array);
                items.extend(sources.into_iter().filter(|id| wanted.is_none_or(|ids| ids.contains(&json!(id.0)))));
                if let Some((parent, _)) = doc.parent_page_for(page).and_then(|(index, page)| doc.parents.get(index).map(|parent| (parent, page))) {
                    under.premises.push(Premise::on(parent.id.0));
                }
            }
        }
        if matches!(command, "bookmark.rename" | "bookmark.delete") {
            let index = params.get("index").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
            let fields = (0..=index.min(doc.bookmarks.len())).map(|index| format!("settings.bookmarks.{index}")).collect::<Vec<_>>();
            under.premises.push(Premise::on("layout").fields(fields).positions(Vec::<String>::new()));
        }
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = items.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(item) = doc.item(id) else { continue };
            under.premises.push(Premise::on(id.0));
            match &item.content {
                Content::Text(frame) => {
                    under.premises.push(Premise::on(frame.story.0));
                    if command == "type.createOutlines" {
                        self.composition_premises(frame.story, under)?;
                    }
                }
                Content::Graphic(graphic) => under.premises.push(Premise::on(graphic.asset.0)),
                Content::Group { items: children } => items.extend(children.iter().map(|item| item.id)),
                Content::Unassigned => {}
            }
        }
        if command == "object.gradient" {
            let targets = params
                .get("ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_u64).map(ItemId).collect())
                .unwrap_or_else(|| state.selection.items.clone());
            if let Some(kdm) = self.session.kdm() {
                let replica = kdm.replica();
                let ids = ToolIds::of_replica(replica);
                let swatches = replica.list(&ObjectId::from_string("layout"), "swatches").map_err(crate::engine_error)?;
                for target in targets {
                    let Some(name) = doc.item(target).map(|item| &item.fill.swatch) else { continue };
                    for swatch in &swatches {
                        if replica.get(swatch, "name").map_err(crate::engine_error)? == *name {
                            under.premises.push(Premise::on(ids.show(swatch)));
                            break;
                        }
                    }
                }
            }
        }
        if command == "story.updateLink" {
            for story in doc.stories.values().filter(|story| params.get("story").and_then(Value::as_u64).is_none_or(|id| id == story.id.0)) {
                if let Some((parent, _)) = story.link {
                    under.premises.push(Premise::on(story.id.0));
                    under.premises.push(Premise::on(parent.0));
                }
            }
        }
        let record = match command {
            "hyperlink.edit" | "hyperlink.delete" | "hyperlink.goToSource" => Some("settings.hyperlinks"),
            "data.placeholder.remove" => Some("settings.dataMerge.placeholders"),
            _ => None,
        };
        if let Some(record) = record
            && let Some(id) = params.get("id").and_then(Value::as_u64)
        {
            under.premises.push(Premise::on("layout").fields([format!("{record}[id={id}]")]).positions(Vec::<String>::new()));
        }
        if let Some(kdm) = self.session.kdm() {
            let replica = kdm.replica();
            let ids = ToolIds::of_replica(replica);
            let root = ObjectId::from_string("layout");
            for (path, types) in crate::addresses::named_references(command) {
                let family = if types.contains(&"Style") { crate::addresses::style_family(command, path, params) } else { Some("swatches") };
                let Some(family) = family else { continue };
                let mut copy = params.clone();
                crate::addresses::visit(&mut copy, path, &mut |value| {
                    if let Some(name) = value.as_str() {
                        let name = if family == "swatches" { doc.swatch(name).map_or(name, |swatch| swatch.name.as_str()) } else { name };
                        let content = matches!(*path, "name" | "names" | "swatch" | "swatches")
                            && matches!(
                                command,
                                "style.paragraph.edit"
                                    | "style.character.edit"
                                    | "style.table.edit"
                                    | "style.cell.edit"
                                    | "style.paragraph.delete"
                                    | "style.character.delete"
                                    | "swatch.delete"
                                    | "style.group"
                                    | "swatch.addToSwatches"
                                    | "swatch.moveToGroup"
                            );
                        for object in replica.list(&root, family).map_err(crate::engine_error)? {
                            if replica.get(&object, "name").map_err(crate::engine_error)? == name {
                                // Suppress the SDK's full-definition default for references.
                                under.premises.push(if content {
                                    Premise::on(ids.show(&object))
                                } else {
                                    Premise::on(ids.show(&object)).fields(["name"]).positions(Vec::<String>::new())
                                });
                                break;
                            }
                        }
                    }
                    Ok(())
                })?;
            }
        }
        Ok(())
    }
}
