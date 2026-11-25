use super::*;

impl LayoutDocument {
    pub(super) fn page_premises(&self, command: &str, params: &Value, under: &mut Under) -> Result<(), String> {
        let state = self.active()?;
        let doc = &state.doc;
        self.layer_premises(command, params, under)?;
        let mut pages = Vec::new();
        if matches!(command, "layout.pages.applyParent" | "layout.pages.insert") {
            let resolved = kition_layout_engine::cmd::parent_ref(doc, params.get("parent"))
                .unwrap_or_else(|| if command == "layout.pages.insert" { doc.parents.first().map(|p| p.id) } else { None });
            if let Some(parent) = resolved {
                let fields = if let Some(given) = params.get("parent").and_then(Value::as_str) {
                    if doc.parents.iter().any(|p| p.id == parent && p.parent.as_ref().is_some_and(|info| info.prefix == given)) {
                        vec!["prefix"]
                    } else {
                        vec!["prefix", "name"]
                    }
                } else {
                    vec![]
                };
                under.premises.push(Premise::on(parent.0).fields(fields).positions(Vec::<String>::new()));
                if params.get("parent").is_none() {
                    under.within.push(Within { parent: json!("layout"), field: "parents".into(), from: 0, to: 1, time: None });
                }
            }
        }
        let mut add = |key: &str, one_based: bool| {
            for value in params.get(key).into_iter().flat_map(|v| v.as_array().map_or_else(|| vec![v], |a| a.iter().collect())) {
                if let Some(index) = value.as_u64().and_then(|n| usize::try_from(n).ok()) {
                    pages.push(if one_based { index.saturating_sub(1) } else { index });
                }
            }
        };
        match command {
            "layout.pages.insert" => {
                add("after", false);
                if pages.is_empty() {
                    pages.push(doc.page_count().saturating_sub(1));
                }
            }
            "layout.pages.delete" | "layout.marginsAndColumns" => add("pages", false),
            "layout.pages.move" => {
                add("from", false);
                add("to", false);
            }
            "layout.pages.applyParent" | "layout.pageSize" => add("pages", true),
            "button.set" => add("page", false),
            "toc.generate" | "index.generate" | "file.place" => add("page", true),
            "page.transition" | "liquid.pageRule" | "page.background" => add("pages", true),
            "layout.pages.toSpread" | "layout.section" | "frame.thread" | "bookmark.add" | "link.add" | "link.edit" => add("page", true),
            "layout.overrideParentItems" | "layout.removeOverrides" | "layout.detachAll" => {
                add("page", false);
                if pages.is_empty() {
                    pages.push(state.selection.items.first().and_then(|id| doc.page_of_item(*id)).unwrap_or(0));
                }
            }
            "layout.deleteAlternate" | "layout.addAlternate" => pages.extend(alternate_pages(doc, command, params)),
            _ => {}
        }
        // Without `pages` (`all` for transitions) these write every page there is when they run:
        // every page, and the place after the last one, where another writer adds a page.
        let every = match command {
            "liquid.pageRule" | "page.background" => params.get("pages").is_none_or(Value::is_null),
            "page.transition" => params.get("all").and_then(Value::as_bool) == Some(true),
            _ => false,
        };
        let alternate = matches!(command, "layout.deleteAlternate" | "layout.addAlternate");
        if every {
            pages.extend(0..doc.page_count());
        }
        if every || alternate {
            // A page another writer adds anywhere joins every page (and the last layout). Such a
            // step names no place parameter, so the guard reads no window for it: hold the
            // spread list itself.
            under.premises.push(Premise::on("layout").fields(["spreads"]).positions(Vec::<String>::new()));
        }
        if alternate {
            // The layouts' extents are the sections' markers; their pages use parents.
            under.premises.push(Premise::on("layout").fields(["sections"]).positions(Vec::<String>::new()));
        }
        // A page number counts across spreads. Hold the prefix's identities and placement,
        // not their content; windows detect insertions, explicit objects detect deletions.
        for abs in pages {
            let Some((si, pi)) = doc.page_loc(abs) else { continue };
            under.within.push(Within { parent: json!("layout"), field: "spreads".into(), from: 0, to: si.saturating_add(1), time: None });
            for (index, spread) in doc.spreads.iter().enumerate().take(si.saturating_add(1)) {
                under.premises.push(Premise::on(spread.id.0).fields(Vec::<String>::new()).positions(["spreads"]));
                let end = if index == si { pi.saturating_add(1) } else { spread.pages.len() };
                under.within.push(Within { parent: json!(spread.id.0), field: "pages".into(), from: 0, to: end, time: None });
                for page in spread.pages.iter().take(end) {
                    under.premises.push(Premise::on(page.id.0).fields(Vec::<String>::new()).positions(["pages"]));
                }
            }
            if let Some(page) = doc.page(abs) {
                under.objects.push(json!(page.id.0));
                // The items deletion consumes are under the place too: one another writer put
                // there since the observation (created, or moved onto the page) refuses, also
                // when that writer is another agent on this server, whose change set names the
                // item and not the page whose `~pageItems` it changes.
                let consumed = matches!(command, "layout.pages.delete" | "layout.deleteAlternate" | "layout.addAlternate");
                if consumed {
                    under.objects.extend(doc.page_items(si, pi).into_iter().map(|id| json!(id.0)));
                    if let Some(parent) = doc.page(abs).and_then(|page| page.parent) {
                        under.premises.push(Premise::on(parent.0));
                    }
                }
                // Only deletion (and copying, for an alternate layout) consumes the items on the
                // page (`~pageItems`); other page writes rest on its stored fields and spread
                // identity.
                under.premises.push(if consumed {
                    Premise::on(page.id.0)
                } else {
                    let schema = kition_layout_kdm::schema();
                    let stored = schema
                        .types
                        .get("Page")
                        .map(|page| page.fields.keys().cloned().collect::<Vec<_>>())
                        .ok_or("the object model has no Page type")?;
                    Premise::on(page.id.0).fields(stored.into_iter().chain(["~placeSpread".to_owned()]))
                });
            }
        }
        if matches!(command, "guide.move" | "guide.delete" | "guide.liquid") {
            self.guide_premises(command, params, under)?;
        }
        let spread_addressed =
            matches!(command, "layout.pages.duplicateSpread" | "layout.spreadShuffle" | "layout.rotateSpreadView" | "layout.pages.toSpread");
        if spread_addressed {
            let si = params.get("spread").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
            under.within.push(Within { parent: json!("layout"), field: "spreads".into(), from: 0, to: si.saturating_add(1), time: None });
            for spread in doc.spreads.iter().take(si.saturating_add(1)) {
                under.premises.push(Premise::on(spread.id.0).fields(Vec::<String>::new()).positions(["spreads"]));
            }
            if let Some(spread) = doc.spreads.get(si) {
                under.objects.push(json!(spread.id.0));
                under.premises.push(Premise::on(spread.id.0));
            }
        }
        Ok(())
    }

    fn layer_premises(&self, command: &str, params: &Value, under: &mut Under) -> Result<(), String> {
        let doc = &self.active()?.doc;
        match command {
            "layer.new" => {
                // Both the default name and the color use the count, even with an explicit name.
                under.premises.push(Premise::on(ROOT).fields(["~layerCount"]).positions(Vec::<String>::new()));
            }
            "layer.deleteUnused" => {
                let mut fields = vec!["~layerIds", "~usedLayerIds"];
                if !doc.spreads.iter().chain(&doc.parents).any(|spread| !spread.items.is_empty()) {
                    fields.push("~firstLayer");
                }
                under.premises.push(Premise::on(ROOT).fields(fields).positions(Vec::<String>::new()));
            }
            "layer.others" => {
                let excluded = params.get("id").and_then(Value::as_u64).unwrap_or(0);
                let enabled = |name: &str| params.get(name).and_then(Value::as_bool).unwrap_or(false);
                let (hide, lock, show, unlock) = (enabled("hide"), enabled("lock"), enabled("show"), enabled("unlock"));
                // `id` excludes this layer from hide/lock; it does not make its content a target.
                if doc.layers.iter().any(|layer| layer.id.0 == excluded) {
                    under.premises.push(Premise::on(excluded).fields(Vec::<String>::new()).positions(Vec::<String>::new()));
                }
                if hide || lock || show || unlock {
                    under.premises.push(Premise::on(ROOT).fields(["~layerIds"]).positions(Vec::<String>::new()));
                }
                for layer in &doc.layers {
                    let mut fields = Vec::new();
                    if show || hide && layer.id.0 != excluded {
                        fields.push("visible");
                    }
                    if unlock || lock && layer.id.0 != excluded {
                        fields.push("locked");
                    }
                    if !fields.is_empty() {
                        under.premises.push(Premise::on(layer.id.0).fields(fields).positions(Vec::<String>::new()));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// A guide is addressed by its spread, its page's index in that spread and its index on the
    /// page, resolved as each handler does. Hold that counted prefix and the guide's object: the
    /// object model keeps guides positionally, so any shift before it changes that object.
    fn guide_premises(&self, command: &str, params: &Value, under: &mut Under) -> Result<(), String> {
        use kition_layout_engine::doc::SpreadRef;
        let doc = &self.active()?.doc;
        let resolved = if command == "guide.liquid" {
            kition_layout_engine::cmd::spread_param(&self.session, params, "spread").ok().and_then(|spread| {
                let page = params.get("page").and_then(Value::as_u64).map_or(Some(0), |n| usize::try_from(n).ok())?;
                let index = usize::try_from(params.get("index").and_then(Value::as_u64)?).ok()?;
                Some((spread, page, index))
            })
        } else {
            kition_layout_engine::cmd::guide_ref(params, command).ok()
        };
        let Some((reference, page, index)) = resolved else { return Ok(()) };
        let Some(spread) = doc.spread(reference) else { return Ok(()) };
        if command != "guide.liquid"
            && let Some(layer) = spread.pages.get(page).and_then(|page| page.guides.get(index)).and_then(|guide| guide.layer)
            && doc.layer(layer).is_some()
        {
            under.premises.push(Premise::on(layer.0).fields(["~guideEditable"]).positions(Vec::<String>::new()));
        }
        let (field, si, list) = match reference {
            SpreadRef::Doc(si) => ("spreads", si, &doc.spreads),
            SpreadRef::Parent(si) => ("parents", si, &doc.parents),
        };
        under.within.push(Within { parent: json!("layout"), field: field.into(), from: 0, to: si.saturating_add(1), time: None });
        for prefix in list.iter().take(si.saturating_add(1)) {
            under.premises.push(Premise::on(prefix.id.0).fields(Vec::<String>::new()).positions([field]));
        }
        // A parent named by its prefix or label: the name it was found by.
        if params.get("spread").is_some_and(Value::is_string) {
            under.premises.push(Premise::on(spread.id.0).fields(["prefix", "name"]).positions(Vec::<String>::new()));
        }
        under.within.push(Within { parent: json!(spread.id.0), field: "pages".into(), from: 0, to: page.saturating_add(1), time: None });
        for prefix in spread.pages.iter().take(page.saturating_add(1)) {
            under.premises.push(Premise::on(prefix.id.0).fields(Vec::<String>::new()).positions(["pages"]));
        }
        let flat = spread.pages.iter().take(page).map(|p| p.guides.len()).sum::<usize>().saturating_add(index);
        let Some(kdm) = self.session.kdm() else { return Ok(()) };
        let replica = kdm.replica();
        let Some(object) = kdm.model().ids().spreads.object(spread.id.0) else { return Ok(()) };
        let guides = replica.list(object, "guides").map_err(crate::engine_error)?;
        under.within.push(Within { parent: json!(spread.id.0), field: "guides".into(), from: 0, to: flat.saturating_add(1), time: None });
        if let Some(guide) = guides.get(flat) {
            under.premises.push(Premise::on(kition_kit::kdm::tool_ids::ToolIds::of_replica(replica).show(guide)));
        }
        Ok(())
    }
}

/// The absolute pages an alternate layout command consumes: the layout `layout.deleteAlternate`
/// deletes, or the source layout `layout.addAlternate` copies, as each handler resolves it.
pub(super) fn alternate_pages(doc: &kition_layout_engine::doc::Document, command: &str, params: &Value) -> Vec<usize> {
    let wanted = match command {
        "layout.deleteAlternate" => params.get("name").and_then(Value::as_str),
        "layout.addAlternate" => params.get("source").and_then(Value::as_str),
        _ => return vec![],
    };
    let all = kition_layout_engine::cmd::layouts(doc);
    let range = match wanted {
        Some(name) => all.into_iter().find(|(layout, _)| layout.as_deref() == Some(name)).map(|(_, range)| range),
        None if command == "layout.addAlternate" => all.into_iter().next().map(|(_, range)| range),
        None => None,
    };
    range.map(Iterator::collect).unwrap_or_default()
}
