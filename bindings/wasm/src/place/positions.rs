//! Places the integer scanner found (C8-layout review 2): a spread counted from 0 that a command
//! creates or acts in, and positions inside an object a premise holds whole.
use super::*;
use kition_layout_engine::doc::{SpreadRef, StoryId};

impl LayoutDocument {
    pub(super) fn position_premises(&self, command: &str, params: &Value, under: &mut Under) -> Result<(), String> {
        let state = self.active()?;
        let doc = &state.doc;
        if crate::addresses::SPREAD_PLACES.contains(&command) {
            let spread = kition_layout_engine::cmd::spread_param(&self.session, params, "spread").map_err(crate::engine_error)?;
            // What a creation rests on is the spread's identity and place, not its content;
            // deleting a spread's guides consumes them.
            let whole = command == "guide.deleteAll";
            match spread {
                SpreadRef::Doc(si) => {
                    // A `within` window beside `covers` is not checked at SDK 34dff06: gap.move
                    // holds the counted spreads' places by premise only.
                    if command != "gap.move" {
                        under.within.push(Within { parent: json!("layout"), field: "spreads".into(), from: 0, to: si.saturating_add(1), time: None });
                    }
                    for (index, spread) in doc.spreads.iter().enumerate().take(si.saturating_add(1)) {
                        under.premises.push(if whole && index == si {
                            Premise::on(spread.id.0)
                        } else {
                            Premise::on(spread.id.0).fields(Vec::<String>::new()).positions(["spreads"])
                        });
                    }
                    if let Some(spread) = doc.spreads.get(si).filter(|_| command != "gap.move") {
                        under.objects.push(json!(spread.id.0));
                    }
                }
                SpreadRef::Parent(pi) => {
                    if let Some(parent) = doc.parents.get(pi) {
                        under.premises.push(if whole { Premise::on(parent.id.0) } else { Premise::on(parent.id.0).fields(["prefix", "name"]) });
                    }
                }
            }
            // A page counted within that spread.
            if matches!(command, "guide.add" | "layout.addGuides")
                && let Some(pi) = params.get("page").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok())
                && let Some(spread) = doc.spread(spread)
            {
                under.within.push(Within { parent: json!(spread.id.0), field: "pages".into(), from: 0, to: pi.saturating_add(1), time: None });
                for page in spread.pages.iter().take(pi.saturating_add(1)) {
                    under.premises.push(Premise::on(page.id.0).fields(Vec::<String>::new()).positions(["pages"]));
                }
            }
        }
        if command == "object.align" {
            self.align_premises(params, under)?;
        }
        self.world_premises(command, params, under)?;
        // A position inside one object: that object, held whole.
        let holder = match command {
            "state.show" | "state.rename" | "path.deleteAnchor" | "path.convertAnchor" | "path.moveAnchors" => {
                params.get("id").and_then(Value::as_u64).or_else(|| state.selection.items.first().map(|id| id.0))
            }
            "anchored.options" | "anchored.release" | "xref.setFormat" | "text.select" => {
                params.get("story").and_then(Value::as_u64).or_else(|| state.selection.text.map(|text| text.story.0))
            }
            _ => None,
        };
        if let Some(holder) = holder.filter(|holder| state.doc.item(ItemId(*holder)).is_some() || doc.story(StoryId(*holder)).is_some()) {
            under.premises.push(Premise::on(holder));
        }
        Ok(())
    }

    /// Handlers that map spread coordinates into an item (a cut point, an anchor, a feather or
    /// gradient vector, a caret point) or an item's path out to the spread (combining paths,
    /// anchoring, a caption's position, measured totals) read every ancestor's transform: hold
    /// each ancestor's transform and place, as a non-copy move does (C8-layout review 3).
    fn world_premises(&self, command: &str, params: &Value, under: &mut Under) -> Result<(), String> {
        let state = self.active()?;
        let doc = &state.doc;
        let vector = |a: &str, b: &str| params.get(a).is_some_and(|v| !v.is_null()) && params.get(b).is_some_and(|v| !v.is_null());
        let reads = match command {
            "path.split"
            | "path.erase"
            | "path.smooth"
            | "path.addAnchor"
            | "path.appendAnchor"
            | "path.moveAnchors"
            | "object.makeCompoundPath"
            | "object.pathfinder"
            | "anchored.insert"
            | "object.caption"
            | "edit.pasteInto"
            | "transform.set"
            | "text.placeCaret"
            | "table.dropCells" => true,
            "path.convertAnchor" => params.get("to").is_some_and(|v| !v.is_null()),
            "object.gradientFeather" => vector("from", "to"),
            "object.gradient" => vector("from", "to"),
            _ => false,
        };
        if !reads {
            return Ok(());
        }
        let mut items: Vec<ItemId> = Vec::new();
        for key in ["id", "frame"] {
            items.extend(params.get(key).and_then(Value::as_u64).map(ItemId));
        }
        match params.get("ids").and_then(Value::as_array) {
            Some(ids) => items.extend(ids.iter().filter_map(Value::as_u64).map(ItemId)),
            None if items.is_empty() => items.extend(state.selection.items.iter().copied()),
            None => {}
        }
        for id in items {
            let Some(mut loc) = doc.find(id) else { continue };
            while loc.path.len() > 1 {
                loc.path.pop();
                if let Some(parent) = doc.item_at(&loc) {
                    under.premises.push(Premise::on(parent.id.0).fields(Vec::<String>::new()).positions(["xf", "items", "spread"]));
                }
            }
        }
        Ok(())
    }

    /// Aligning to the page, its margins or the spread reads the page under the targets' centre
    /// (or every page of the spread): its size, margins and place, as `align_reference` does.
    fn align_premises(&self, params: &Value, under: &mut Under) -> Result<(), String> {
        let state = self.active()?;
        let doc = &state.doc;
        let ids: Vec<ItemId> = match params.get("ids").and_then(Value::as_array) {
            Some(ids) => ids.iter().filter_map(Value::as_u64).map(ItemId).collect(),
            None => state.selection.items.clone(),
        };
        let default = if ids.len() > 1 { "selection" } else { "page" };
        let to = params.get("to").and_then(Value::as_str).unwrap_or(default);
        if !matches!(to, "page" | "margins" | "spread") {
            return Ok(());
        }
        let Some(first) = ids.first().and_then(|id| doc.find(*id)) else { return Ok(()) };
        let Some(spread) = doc.spread(first.spread) else { return Ok(()) };
        let union = ids.iter().filter_map(|id| doc.item(*id).map(kition_layout_engine::doc::Item::bounds)).reduce(|a, b| a.union(b));
        let pages: Vec<&kition_layout_engine::doc::Page> = if to == "spread" {
            spread.pages.iter().collect()
        } else {
            union.and_then(|union| spread.page_at_x(union.center().x)).and_then(|index| spread.pages.get(index)).into_iter().collect()
        };
        for page in pages {
            under.premises.push(Premise::on(page.id.0).fields(["size", "margins", "x", "side"]).positions(["pages"]));
        }
        Ok(())
    }
}
