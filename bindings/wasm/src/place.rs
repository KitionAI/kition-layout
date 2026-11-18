//! Facts held by audited command handlers. Unlisted commands retain the SDK's full-target guard.
use crate::LayoutDocument;
use kition_kit::{Carried, Carries, Covered, CoveredAsk, CoveredAt, Covers, DerivedValues, KitDocument, Premise, Under, Within};
use kition_layout_engine::doc::{Content, ItemId};
use kition_layout_geom::{Point, Rect};
use serde_json::{Value, json};

/// The derived fact a cell holds for `style.cell.create` ([`LayoutDocument::cell_style_sources`]).
const STYLE_SOURCE: &str = "styleSource";

mod composition;
pub(crate) mod history;
mod members;
mod pages;
mod positions;
mod queries;
mod resources;
mod rests;
mod settings;
mod tables;
mod text;

const BOUNDS: [&str; 4] = ["boundsX0", "boundsY0", "boundsX1", "boundsY1"];

fn property(command: &str, params: &Value) -> Option<Vec<String>> {
    let fields: &[&str] = match command {
        "object.rename" => &["name"],
        "object.fill" => &["fill.swatch", "fill.tint"],
        "object.opacity" => {
            if params.get("blend").is_some_and(|value| !value.is_null()) {
                &["opacity", "blend"]
            } else {
                &["opacity"]
            }
        }
        "object.label" => &["props.label"],
        "object.altText" => &["altText"],
        "object.lock" => &["locked"],
        "object.hide" => &["hidden"],
        "object.setLayer" => &["layer"],
        "object.setFlags" => {
            return Some(
                [("hidden", "hidden"), ("locked", "locked")]
                    .into_iter()
                    .filter(|(param, _)| params.get(param).is_some())
                    .map(|(_, field)| field.to_owned())
                    .collect(),
            );
        }
        "object.attributes" => {
            return Some(
                [
                    ("overprintFill", "fill.overprint"),
                    ("overprintStroke", "stroke.overprint"),
                    ("overprintGap", "stroke.gapOverprint"),
                    ("nonprinting", "nonprinting"),
                ]
                .into_iter()
                .filter(|(param, _)| params.get(param).is_some())
                .map(|(_, field)| field.to_owned())
                .collect(),
            );
        }
        "object.transparencyGroup" => {
            return Some(
                [("isolate", "props.isolate"), ("knockout", "props.knockout")]
                    .into_iter()
                    .filter(|(param, _)| params.get(param).is_some())
                    .map(|(_, field)| field.to_owned())
                    .collect(),
            );
        }
        // C8-layout-read-3: one effect, the corners, the wrap, the stroke or the export options
        // of each target, patched in place (cmd/object.rs set_flag): not its place or paint.
        "object.dropShadow" => &["effects.dropShadow"],
        "object.innerShadow" => &["effects.innerShadow"],
        "object.outerGlow" => &["effects.outerGlow"],
        "object.innerGlow" => &["effects.innerGlow"],
        "object.bevel" => &["effects.bevel"],
        "object.satin" => &["effects.satin"],
        "object.directionalFeather" => &["effects.directionalFeather"],
        "object.feather" => &["effects.feather"],
        "object.cornerOptions" => &["corners"],
        "object.textWrap" => &["wrap"],
        "object.stroke" => &["stroke"],
        "object.exportOptions" => {
            if params.get("altText").is_some() {
                &["props.exportOptions", "altText"]
            } else {
                &["props.exportOptions"]
            }
        }
        "layer.set" => {
            return Some(
                ["name", "visible", "locked", "printable", "color"]
                    .into_iter()
                    .filter(|field| params.get(field).is_some())
                    .map(str::to_owned)
                    .collect(),
            );
        }
        _ => return None,
    };
    Some(fields.iter().map(|field| (*field).to_owned()).collect())
}

/// The SDK keeps only the last premise a call states for one id. Families add premises for the
/// same object independently (a page's content, then its place in another page's counted
/// prefix; a story's text, then its composition), so join them: every field or position either
/// needs, every one when either leaves it open.
fn join_premises(under: &mut Under) {
    fn union(into: &mut Option<Vec<String>>, more: Option<Vec<String>>) {
        match (into.as_mut(), more) {
            (Some(known), Some(more)) => {
                for value in more {
                    if !known.contains(&value) {
                        known.push(value);
                    }
                }
            }
            _ => *into = None,
        }
    }
    let mut joined: Vec<Premise> = Vec::with_capacity(under.premises.len());
    for premise in std::mem::take(&mut under.premises) {
        if let Some(known) =
            joined.iter_mut().find(|known| known.id == premise.id || numeric_id(&known.id).is_some_and(|id| numeric_id(&premise.id) == Some(id)))
        {
            union(&mut known.fields, premise.fields);
            union(&mut known.positions, premise.positions);
            known.identities.extend(premise.identities);
        } else {
            joined.push(premise);
        }
    }
    under.premises = joined;
}

impl LayoutDocument {
    pub(crate) fn under_call(&self, command: &str, params: &Value) -> Result<Option<Under>, String> {
        let mut answer = self.under_call_parts(command, params)?;
        if let Some(under) = answer.as_mut() {
            join_premises(under);
        }
        Ok(answer)
    }

    fn under_call_parts(&self, command: &str, params: &Value) -> Result<Option<Under>, String> {
        let params = crate::addresses::resolve(command, params, self.session.kdm().map(|kdm| kdm.replica()))?;
        let state = self.active()?;
        let mut under = Under::default();
        self.page_premises(command, &params, &mut under)?;
        self.position_premises(command, &params, &mut under)?;
        self.text_premises(command, &params, &mut under)?;
        self.resource_premises(command, &params, &mut under)?;
        self.settings_premises(command, &params, &mut under)?;
        self.rest_premises(command, &params, &mut under)?;
        if let Some(members) = members::Members::of(self, command, &params)? {
            self.member_premises(&members, &mut under)?;
        }
        if command == "find.changeObjects" {
            self.query_premises(&params, &mut under)?;
            return Ok(Some(under));
        }
        if command == "transform.move"
            && let Some(destination) = kition_layout_engine::cmd::move_destination(&params)
        {
            use kition_layout_engine::doc::SpreadRef;
            let spread = state.doc.spread(destination).ok_or("no destination spread")?;
            let (field, index) = match destination {
                SpreadRef::Doc(index) => ("spreads", index),
                SpreadRef::Parent(index) => ("parents", index),
            };
            under.objects.push(json!(spread.id.0));
            under.premises.push(Premise::on(spread.id.0).fields(Vec::<String>::new()).positions([field]));
            under.within.push(Within { parent: json!("layout"), field: field.to_owned(), from: index, to: index.saturating_add(1), time: None });
        }
        if self.table_premises(command, &params, &mut under)? {
            return Ok(Some(under));
        }
        if command == "gap.move" {
            let spread = kition_layout_engine::cmd::spread_param(&self.session, &params, "spread").map_err(crate::engine_error)?;
            let point = gap_point(&params)?;
            if let Some((_, _, _, before, after)) = kition_layout_engine::cmd::find_gap(&state.doc, spread, point) {
                under.objects = before.into_iter().chain(after).map(|id| json!(id.0)).collect();
                for id in &under.objects {
                    under.premises.push(
                        Premise::on(id.clone())
                            .fields(["~boundsX0", "~boundsY0", "~boundsX1", "~boundsY1", "path", "content", "graphic.xf", "hidden", "locked"])
                            .positions(["xf", "items", "spread"]),
                    );
                    if let Some(item) = id.as_u64().and_then(|id| state.doc.item(ItemId(id))) {
                        under.premises.push(Premise::on(item.layer.0).fields(["visible", "locked"]).positions(Vec::<String>::new()));
                    }
                }
            }
            under.covers = Some(Covers {
                fields: [
                    "~boundsX0",
                    "~boundsY0",
                    "~boundsX1",
                    "~boundsY1",
                    "~placeSpread",
                    "hidden",
                    "locked",
                    "layer",
                    "spread",
                    "visible",
                    "size",
                    "x",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            });
            return Ok(Some(under));
        }
        let commands = self.commands();
        let Some(spec) = commands.iter().find(|spec| spec.id == command) else { return Ok(None) };
        let targets: Vec<Value> = spec
            .targets
            .and_then(|path| params.get(path))
            .map(|value| value.as_array().cloned().unwrap_or_else(|| vec![value.clone()]))
            .unwrap_or_else(|| {
                if spec.reads.contains(&kition_kit::EditorState::Selection) {
                    state.selection.items.iter().map(|id| json!(id.0)).collect()
                } else {
                    vec![]
                }
            });
        if let Some(fields) = property(command, &params) {
            for id in targets {
                under.premises.push(Premise::on(id).fields(fields.clone()).positions(Vec::<String>::new()));
            }
        } else if command == "transform.move" && !params.get("copy").and_then(Value::as_bool).unwrap_or(false) {
            for id in targets {
                under.premises.push(Premise::on(id.clone()).fields(["locked"]).positions(["xf", "items", "spread"]));
                if let Some(loc) = id.as_u64().and_then(|id| state.doc.find(ItemId(id))) {
                    let mut loc = loc;
                    while loc.path.len() > 1 {
                        loc.path.pop();
                        if let Some(parent) = state.doc.item_at(&loc) {
                            let id = json!(parent.id.0);
                            if !under.premises.iter().any(|premise| premise.id == id) {
                                under.premises.push(Premise::on(id).fields(Vec::<String>::new()).positions(["xf", "items", "spread"]));
                            }
                        }
                    }
                }
            }
        } else if command == "object.group" {
            for id in targets {
                under.premises.push(Premise::on(id).fields(["layer"]).positions(["xf", "items", "spread"]));
            }
        } else if command == "layer.move" {
            if let Some(id) = params.get("id") {
                under.premises.push(Premise::on(id.clone()).fields(Vec::<String>::new()).positions(["layers"]));
            }
            let to = params
                .get("to")
                .and_then(Value::as_u64)
                .and_then(|to| usize::try_from(to).ok())
                .unwrap_or(0)
                .min(state.doc.layers.len().saturating_sub(1));
            // The handler removes the source before applying `to`; map the destination gap
            // back to the current stored list, including the source when it precedes the gap.
            let source = state.doc.layers.iter().position(|layer| Some(layer.id.0) == params.get("id").and_then(Value::as_u64));
            let gap = to + usize::from(source.is_some_and(|source| source < to));
            under.within.push(Within { parent: json!("layout"), field: "layers".to_owned(), from: gap, to: gap, time: None });
        }
        Ok((!under.premises.is_empty() || !under.within.is_empty()).then_some(under))
    }

    pub(crate) fn covered_call(&self, command: &str, params: &Value, asks: &[CoveredAsk]) -> Result<Option<CoveredAt>, String> {
        if command == "find.changeObjects" {
            return self.query_covered(params, asks).map(Some);
        }
        let resolved = crate::addresses::resolve(command, params, self.session.kdm().map(|kdm| kdm.replica()))?;
        if let Some(members) = members::Members::of(self, command, &resolved)? {
            return self.member_covered(&members, asks).map(Some);
        }
        if let Some(window) = text::window(command, &resolved) {
            return self.text_covered(&window, asks).map(Some);
        }
        if command != "gap.move" {
            return Ok(None);
        }
        let state = self.active()?;
        let sr = kition_layout_engine::cmd::spread_param(&self.session, params, "spread").map_err(crate::engine_error)?;
        let point = gap_point(params)?;
        let spread = state.doc.spread(sr).ok_or("no spread")?;
        let page = spread.page_at_x(point.x).and_then(|index| spread.pages.get(index)).ok_or("no page")?.bounds();
        let mut items: std::collections::BTreeMap<u64, (Rect, bool, bool, u64)> =
            spread.items.iter().map(|item| (item.id.0, (item.bounds(), item.hidden, item.locked, item.layer.0))).collect();
        let mut layers: std::collections::BTreeMap<u64, (bool, bool)> =
            state.doc.layers.iter().map(|layer| (layer.id.0, (layer.visible, layer.locked))).collect();
        let mut uncertain = false;
        // Apply every substitution before picking. An unsupported historical structural fact
        // answers unknown; it must never be guessed from the current hierarchy.
        for ask in asks {
            let id = numeric_id(&ask.id);
            let page_spread = ask.before.get("~placeSpread").and_then(Value::as_u64).or_else(|| {
                state
                    .doc
                    .spreads
                    .iter()
                    .chain(&state.doc.parents)
                    .find(|s| Some(s.id.0) == id || s.pages.iter().any(|p| Some(p.id.0) == id))
                    .map(|s| s.id.0)
            });
            if page_spread.is_some_and(|owner| owner != spread.id.0) {
                continue;
            }
            if !ask.unknown.is_empty() {
                uncertain = true;
            }
            if ask.deleted
                && historical_bounds(Rect::ZERO, &ask.before, true)
                    .is_some_and(|bounds| !(bounds.y0 <= point.y && point.y <= bounds.y1) && !(bounds.x0 <= point.x && point.x <= bounds.x1))
            {
                continue;
            }
            if let Some(id) = id {
                if ask.created {
                    if items.remove(&id).is_none() && layers.remove(&id).is_none() && state.doc.item(ItemId(id)).is_none() {
                        uncertain = true;
                    }
                    continue;
                }
                if let Some(layer) = layers.get_mut(&id) {
                    if let Some(value) = ask.before.get("visible").and_then(Value::as_bool) {
                        layer.0 = value;
                    }
                    if let Some(value) = ask.before.get("locked").and_then(Value::as_bool) {
                        layer.1 = value;
                    }
                    if ask.deleted {
                        uncertain = true;
                    }
                    continue;
                }
                if let Some(item) = items.get_mut(&id) {
                    if let Some(bounds) = historical_bounds(item.0, &ask.before, false) {
                        item.0 = bounds;
                    } else if ask.before.contains_key("xf") || ask.before.contains_key("path") {
                        uncertain = true;
                    }
                    if let Some(value) = ask.before.get("hidden").and_then(Value::as_bool) {
                        item.1 = value;
                    }
                    if let Some(value) = ask.before.get("locked").and_then(Value::as_bool) {
                        item.2 = value;
                    }
                    if ask.before.contains_key("layer") || ask.before.contains_key("spread") || ask.before.contains_key("items") {
                        uncertain = true;
                    }
                    continue;
                }
                // The host may ask only the changed child. Its owning group's historical
                // bounds cannot be reconstructed from that child's bounds alone.
                if state.doc.item(ItemId(id)).is_some() && !ask.deleted {
                    uncertain = true;
                    continue;
                }
            }
            if ask.deleted || !ask.before.is_empty() {
                uncertain = true;
            }
        }
        if uncertain {
            return Ok(Some(CoveredAt {
                under: vec![],
                covered: asks.iter().map(|ask| Covered { id: ask.id.clone(), covered: None, ..Default::default() }).collect(),
            }));
        }
        let visible: Vec<_> = items
            .iter()
            .filter(|(_, (_, hidden, locked, layer))| !hidden && !locked && layers.get(layer).is_none_or(|(visible, locked)| *visible && !locked))
            .map(|(id, (bounds, _, _, _))| (ItemId(*id), *bounds))
            .collect();
        let under: Vec<_> = kition_layout_engine::cmd::gap_at(page, point, &visible)
            .map(|(_, _, _, before, after)| before.into_iter().chain(after).map(|id| json!(id.0)).collect())
            .unwrap_or_default();
        let covered = asks
            .iter()
            .map(|ask| {
                let id = numeric_id(&ask.id);
                let affected = under.iter().any(|target| target == &ask.id || numeric_id(target) == id)
                    || under.iter().filter_map(numeric_id).any(|target| items.get(&target).is_some_and(|(_, _, _, layer)| Some(*layer) == id));
                Covered { id: ask.id.clone(), covered: Some(affected), ..Default::default() }
            })
            .collect();
        Ok(Some(CoveredAt { under, covered }))
    }

    pub(crate) fn derived_values(&self, ids: &[Value], names: &[String]) -> Result<Vec<DerivedValues>, String> {
        if !names.is_empty()
            && !names.iter().any(|name| {
                BOUNDS.contains(&name.trim_start_matches('~'))
                    || queries::QUERY_FIELDS.contains(&name.trim_start_matches('~'))
                    || matches!(
                        name.trim_start_matches('~'),
                        "placeSpread"
                            | "compositionFrames"
                            | "pageItems"
                            | "layerItems"
                            | "guideEditable"
                            | "layerCount"
                            | "layerIds"
                            | "usedLayerIds"
                            | "firstLayer"
                            | "lastLayer"
                            | "inkAllToProcess"
                            | STYLE_SOURCE
                    )
            })
        {
            return Ok(vec![]);
        }
        let state = self.active()?;
        let mut values = Vec::new();
        let all;
        if ids.is_empty() {
            // A full read: the state an observation or an outside change shows.
            history::remember(&state.doc);
        }
        let given_all = ids.is_empty();
        let ids = if given_all {
            all = std::iter::once(json!(ROOT))
                .chain(state.doc.all_items().into_iter().map(|id| json!(id.0)))
                .chain(state.doc.stories.keys().map(|id| json!(id.0)))
                .chain(state.doc.layers.iter().map(|layer| json!(layer.id.0)))
                .chain(
                    state
                        .doc
                        .spreads
                        .iter()
                        .chain(&state.doc.parents)
                        .flat_map(|s| std::iter::once(json!(s.id.0)).chain(s.pages.iter().map(|p| json!(p.id.0)))),
                )
                .collect::<Vec<_>>();
            all.as_slice()
        } else {
            ids
        };
        // Cells are told by their own tool ids, which no branch below names.
        let mut cells = std::collections::HashSet::new();
        if names.is_empty() || names.iter().any(|name| name.trim_start_matches('~') == STYLE_SOURCE) {
            for source in self.cell_style_sources(if given_all { None } else { Some(ids) })? {
                cells.insert(source.id.to_string());
                values.push(source);
            }
        }
        for given in ids {
            if cells.contains(&given.to_string()) {
                continue;
            }
            if given.as_str() == Some(ROOT) {
                values.push(DerivedValues { id: given.clone(), values: self.root_derived(names)? });
                continue;
            }
            if let Some(story) = numeric_id(given).and_then(|id| state.doc.story(kition_layout_engine::doc::StoryId(id))) {
                if names.is_empty() || names.iter().any(|name| name.trim_start_matches('~') == "compositionFrames") {
                    values.push(DerivedValues {
                        id: given.clone(),
                        values: [("compositionFrames".to_owned(), composition::frame_facts(&state.doc, story.id))].into_iter().collect(),
                    });
                }
                continue;
            }
            let wants = |wanted: &str| names.is_empty() || names.iter().any(|name| name.trim_start_matches('~') == wanted);
            // What deleting this layer would delete with it, by the engine's own resolver.
            if let Some(layer) = numeric_id(given).and_then(|id| state.doc.layer(kition_layout_engine::doc::LayerId(id))) {
                let mut found = serde_json::Map::new();
                if wants("layerItems") {
                    let items = state.doc.layer_items(layer.id).into_iter().map(|id| id.0).collect::<Vec<_>>();
                    found.insert("layerItems".to_owned(), json!(items));
                }
                if wants("guideEditable") {
                    found.insert("guideEditable".to_owned(), json!(!layer.locked && layer.visible && layer.show_guides));
                }
                values.push(DerivedValues { id: given.clone(), values: found.into_iter().collect() });
                continue;
            }
            if let Some((index, spread)) = state
                .doc
                .spreads
                .iter()
                .enumerate()
                .find(|(_, s)| Some(s.id.0) == numeric_id(given) || s.pages.iter().any(|p| Some(p.id.0) == numeric_id(given)))
            {
                let mut found = serde_json::Map::new();
                if wants("placeSpread") {
                    found.insert("placeSpread".to_owned(), json!(spread.id.0));
                }
                // What deleting this page would delete with it, by the engine's own resolver.
                if wants("pageItems")
                    && let Some(page) = spread.pages.iter().position(|p| Some(p.id.0) == numeric_id(given))
                {
                    found.insert("pageItems".to_owned(), json!(state.doc.page_items(index, page).into_iter().map(|id| id.0).collect::<Vec<_>>()));
                }
                values.push(DerivedValues { id: given.clone(), values: found.into_iter().collect() });
                continue;
            }
            if wants("placeSpread")
                && let Some(spread) =
                    state.doc.parents.iter().find(|s| Some(s.id.0) == numeric_id(given) || s.pages.iter().any(|p| Some(p.id.0) == numeric_id(given)))
            {
                values.push(DerivedValues { id: given.clone(), values: [("placeSpread".to_owned(), json!(spread.id.0))].into_iter().collect() });
                continue;
            }
            let params = crate::addresses::resolve("object.rename", &json!({"id":given}), self.session.kdm().map(|kdm| kdm.replica()))?;
            if let Some(item) = params["id"].as_u64().and_then(|id| state.doc.item(ItemId(id))) {
                let b = item.bounds();
                let mut fields: std::collections::BTreeMap<String, Value> =
                    BOUNDS.into_iter().zip([b.x0, b.y0, b.x1, b.y1]).map(|(key, value)| (key.to_owned(), json!(value))).collect();
                if let Value::Object(query) = kition_layout_engine::cmd::object_query_facts(item) {
                    fields.extend(query.into_iter().filter(|(key, _)| queries::QUERY_FIELDS.contains(&key.as_str())));
                }
                values.push(DerivedValues {
                    id: given.clone(),
                    values: fields
                        .into_iter()
                        .filter(|(key, _)| names.is_empty() || names.iter().any(|name| name.trim_start_matches('~') == key))
                        .collect(),
                });
            }
        }
        Ok(values)
    }

    /// Root-wide facts, separate from the properties of objects in a root-owned collection.
    /// The SDK re-reads a derived value after a write on this server only for the objects its
    /// change set names, and always for the root; a wrap object moved names no story, so
    /// composition premises are stated on the root too (`~compositionFrames.<story>`). The items
    /// on a layer are held as a place of `layer.delete` instead (`members::Members::Layer`).
    fn root_derived(&self, names: &[String]) -> Result<serde_json::Map<String, Value>, String> {
        let doc = &self.active()?.doc;
        let wants = |wanted: &str| names.is_empty() || names.iter().any(|name| name.trim_start_matches('~') == wanted);
        let mut values = serde_json::Map::new();
        if wants("layerCount") {
            values.insert("layerCount".to_owned(), json!(doc.layers.len()));
        }
        if wants("layerIds") {
            let ids = doc.layers.iter().map(|layer| layer.id.0).collect::<std::collections::BTreeSet<_>>();
            values.insert("layerIds".to_owned(), json!(ids));
        }
        if wants("usedLayerIds") {
            let used = doc
                .spreads
                .iter()
                .chain(&doc.parents)
                .flat_map(|spread| spread.items.iter().map(|item| item.layer.0))
                .collect::<std::collections::BTreeSet<_>>();
            values.insert("usedLayerIds".to_owned(), json!(used));
        }
        if wants("firstLayer") {
            values.insert("firstLayer".to_owned(), json!(doc.layers.first().map(|layer| layer.id.0)));
        }
        if wants("lastLayer") {
            values.insert("lastLayer".to_owned(), json!(doc.layers.last().map(|layer| layer.id.0)));
        }
        if wants("inkAllToProcess") {
            values.insert("inkAllToProcess".to_owned(), json!(doc.inks.all_to_process));
        }
        if wants("compositionFrames") {
            let stories: serde_json::Map<String, Value> =
                doc.stories.keys().map(|id| (id.0.to_string(), json!(digest(&composition::frame_facts(doc, *id))))).collect();
            values.insert("compositionFrames".to_owned(), Value::Object(stories));
        }
        Ok(values)
    }

    pub(crate) fn carried_values(&self, ids: &[Value], changes: &[Value]) -> Result<Vec<Carried>, String> {
        let state = self.active()?;
        let mut result = Vec::new();
        for id in ids {
            if !changes.iter().any(|change| {
                &change["id"] == id
                    && change["fields"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|field| field.as_str().is_some_and(|field| field == "xf" || field.starts_with("xf.")))
            }) {
                continue;
            }
            let params = crate::addresses::resolve("object.rename", &json!({"id":id}), self.session.kdm().map(|kdm| kdm.replica()))?;
            if let Some(item) = params["id"].as_u64().and_then(|id| state.doc.item(ItemId(id))) {
                let mut count = 0u64;
                let mut pending = match &item.content {
                    Content::Group { items } => items.iter().map(|item| item.as_ref()).collect::<Vec<_>>(),
                    _ => vec![],
                };
                while let Some(item) = pending.pop() {
                    count = count.saturating_add(1);
                    if let Content::Group { items } = &item.content {
                        pending.extend(items.iter().map(|item| item.as_ref()));
                    }
                }
                if count > 0 {
                    result.push(Carried { id: id.clone(), carries: Carries { count, what: "world location".to_owned() } });
                }
            }
        }
        Ok(result)
    }
}

/// The object model root's object id, as change sets and premises name it.
const ROOT: &str = "layout";

/// A short stable digest of a value, for derived values compared rather than read.
fn digest(value: &Value) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.to_string().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn numeric_id(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}
fn gap_point(params: &Value) -> Result<Point, String> {
    let values = params.get("at").and_then(Value::as_array).ok_or("gap.move needs at")?;
    Ok(Point::new(values.first().and_then(Value::as_f64).ok_or("at needs x")?, values.get(1).and_then(Value::as_f64).ok_or("at needs y")?))
}
fn historical_bounds(current: Rect, before: &serde_json::Map<String, Value>, complete: bool) -> Option<Rect> {
    let mut values = [current.x0, current.y0, current.x1, current.y1];
    let mut count = 0;
    for (slot, key) in values.iter_mut().zip(BOUNDS) {
        if let Some(value) = before.get(&format!("~{key}")) {
            *slot = value.as_f64()?;
            count += 1;
        }
    }
    if count == 0 || complete && count != 4 {
        return None;
    }
    let [x0, y0, x1, y1] = values;
    Some(Rect::new(x0, y0, x1, y1))
}
