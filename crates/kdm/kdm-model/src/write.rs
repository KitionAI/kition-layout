//! `Document` -> replica: a full write (a new replica, an import) and an incremental diff (the
//! engine command bridge, later) that writes only what changed. Both run the same passes, a full
//! write being a diff from nothing:
//!
//! 1. every new layer, spread, page, asset, story, hyperlink and data merge placeholder gets its
//!    object (by local id), every new swatch, style, condition, text variable and cross-reference
//!    format its object (named lists are paired with their previous version by content, then by
//!    name), and what `after` no longer holds is deleted with what it owns;
//! 2. every new item gets its object, every item `after` no longer holds is deleted; each story's
//!    inline objects and tables are paired by key (rows and columns of a table by content);
//! 3. the `items` tree is made to hold `after`'s items (top-level items of every spread and parent
//!    spread, then the objects anchored in stories): under each parent whose children changed,
//!    the longest run of children already in place stays and the rest are moved after it in
//!    order; an item whose `Arc` did not change is skipped whole;
//! 4. every field that differs is set, story text is written as a difference
//!    (`kition_kit::session::write_rich_text`, hyperlink sources and placeholders as marks on it),
//!    every list is made to hold `after`'s order, and the settings map gets the keys that changed.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use kition_kit::kdm::{NodeRef, ObjectId, Tx};
use kition_kit::session::{BlobSink, TextRun, write_rich_text};
use kition_layout_doc::{Cell, Content, Document, Item, ItemId, Story, StoryId};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::cells::{cell_id, cell_key};
use crate::fields::{self, Fields, Out, to_map};
use crate::ids::{
    FAMILIES, FAMILY_FIELDS, IdMap, Ids, InlineKey, InlineKind, NAMED, NAMED_LISTS, TableIds, VARIABLES, check_unique, has_own_id, index,
};
use crate::inline::{InlineRef, inline_fields, inline_list, placeholder_objects, same_inline};
use crate::list::{align, sync_list, sync_set};
use crate::refs::{Mapper, SETTINGS_WITH_IDS, map_setting, swatches_out};
use crate::text::{Anchors, NamesOut, story_runs};
use crate::{KdmResult, MAX_DEPTH, kdm_err, root_id};

/// The keys of a serialized `Document` that are objects or fields of their own, not settings.
pub(crate) const MODELLED: &[&str] = &[
    "title",
    "settings",
    "spreads",
    "parents",
    "layers",
    "stories",
    "styles",
    "swatches",
    "sections",
    "assets",
    "hyperlinks",
    "conditions",
    "textVariables",
    "xrefFormats",
    "nextId",
];

/// The `styles` keys that are settings (the rest are the five style lists).
pub(crate) const STYLE_SETTINGS: &[(&str, &str)] =
    &[("compositeFonts", "compositeFonts"), ("mojikumiTables", "mojikumiTables"), ("exportTags", "exportTags")];

/// The `styles` keys naming the default styles, kept together in the `styleDefaults` setting.
pub(crate) const STYLE_DEFAULTS: &[&str] = &["defaultParagraph", "defaultCharacter", "defaultTextFrame", "defaultGraphicFrame"];

fn fields_map(fields: Fields) -> Map<String, Value> {
    fields.into_iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.to_owned(), v)).collect()
}

fn oid(object: &ObjectId) -> Value {
    Value::String(object.as_str().to_owned())
}

fn kit(error: impl std::fmt::Display) -> kition_kit::kdm::KdmError {
    kdm_err(error)
}

/// The required registers a new object of `ty` is created with, before the write sets their real
/// values in the same transaction (a KDM object is created with every required field).
pub(crate) fn placeholder(ty: &str) -> Map<String, Value> {
    let fields: Value = match ty {
        "Layout" => json!({ "title": "" }),
        "Layer" | "Style" => json!({ "name": "" }),
        "Swatch" => json!({ "name": "", "value": {} }),
        "Page" => json!({ "size": {} }),
        "Item" => json!({ "shape": "rectangle", "content": "unassigned", "xf": [1, 0, 0, 1, 0, 0] }),
        "Inline" => json!({ "kind": "footnote" }),
        "Hyperlink" | "Condition" | "TextVariable" | "XrefFormat" => json!({ "name": "" }),
        "Placeholder" => json!({ "source": 0, "field": "", "role": "text" }),
        "Asset" => json!({ "blob": { "hash": kition_kit::session::sha256_hex(b""), "mime": "application/octet-stream", "size": 0 } }),
        _ => json!({}),
    };
    match fields {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// Writes a whole document into a fresh replica's creating transaction. Answers the `Layout`
/// object's id.
pub fn write_document(tx: &mut Tx<'_>, doc: &Document, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<ObjectId> {
    let root = root_id();
    tx.create_with_id("Layout", &root, placeholder("Layout"))?;
    *ids = Ids::default();
    Writer::new(tx, ids, blobs).write(None, doc)?;
    crate::repair::store_addresses(tx)?;
    Ok(root)
}

/// Writes only what differs between `before` (the document `ids` describe) and `after`. Answers
/// whether it wrote anything. Errs, writing nothing usable, if `after` holds an id twice where it
/// must be unique or nests deeper than the bridge goes.
pub fn write_changes(tx: &mut Tx<'_>, before: &Document, after: &Document, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<bool> {
    let wrote = Writer::new(tx, ids, blobs).write(Some(before), after)?;
    Ok(crate::repair::store_addresses(tx)? || wrote)
}

/// The names and objects a document's fields are written with.
pub(crate) fn out_of<'a>(doc: &Document, ids: &'a Ids) -> Out<'a> {
    let names = fields::style_names(&doc.styles);
    let mut styles: [HashMap<String, ObjectId>; FAMILIES] = Default::default();
    for (family, list) in names.iter().enumerate() {
        for (name, object) in list.iter().zip(&ids.styles[family]) {
            styles[family].entry((*name).clone()).or_insert_with(|| object.clone());
        }
    }
    let mut swatches = HashMap::new();
    for (swatch, object) in doc.swatches.iter().zip(&ids.swatches) {
        swatches.entry(swatch.name.clone()).or_insert_with(|| object.clone());
    }
    let mut anchors = HashMap::new();
    for list in ids.inlines.values() {
        for (key, object) in list {
            if key.kind == crate::ids::InlineKind::TextAnchor {
                anchors.entry(key.local).or_insert_with(|| object.clone());
            }
        }
    }
    let page_at = doc.spreads.iter().flat_map(|s| s.pages.iter()).filter_map(|p| ids.pages.object(p.id.0).cloned()).collect();
    let variables = ids.named[VARIABLES].clone();
    Out { ids, names: NamesOut { styles, swatches, variables }, anchors, page_at, ranges: fields::text_ranges(doc, ids) }
}

/// The document's conditions, text variables and cross-reference formats (see [`NAMED`]), each as
/// (serialized value, name), for pairing.
fn named_values(doc: &Document) -> KdmResult<[Vec<(Value, String)>; NAMED_LISTS]> {
    fn list<T: Serialize>(items: &[T], name: impl Fn(&T) -> &String) -> KdmResult<Vec<(Value, String)>> {
        items.iter().map(|x| Ok((serde_json::to_value(x).map_err(kit)?, name(x).clone()))).collect()
    }
    Ok([list(&doc.conditions, |c| &c.name)?, list(&doc.text_variables, |v| &v.name)?, list(&doc.xref_formats, |f| &f.name)?])
}

/// The document's settings: every key of its serialization that is not modelled, the style
/// settings, with local ids as object ids and swatch names as references.
pub(crate) fn settings(doc: &Document, out: &Out<'_>) -> KdmResult<Map<String, Value>> {
    let mut shell = doc.clone();
    shell.spreads = Vec::new();
    shell.parents = Vec::new();
    shell.stories = Default::default();
    shell.assets = Default::default();
    shell.layers = Vec::new();
    shell.swatches = Vec::new();
    shell.sections = Vec::new();
    let mut map = to_map(&shell)?;
    for key in MODELLED {
        map.remove(*key);
    }
    let styles = to_map(&*doc.styles)?;
    for (from, to) in STYLE_SETTINGS {
        if let Some(v) = styles.get(*from) {
            map.insert((*to).to_owned(), v.clone());
        }
    }
    let defaults: Map<String, Value> = STYLE_DEFAULTS.iter().filter_map(|k| styles.get(*k).map(|v| ((*k).to_owned(), v.clone()))).collect();
    map.insert("styleDefaults".into(), Value::Object(defaults));
    // The placeholders are objects of their own; the sources and the merge options stay here.
    if let Some(Value::Object(merge)) = map.get_mut("dataMerge") {
        merge.remove("placeholders");
        if merge.is_empty() {
            map.remove("dataMerge");
        }
    }
    let item = |v: &Value| v.as_u64().and_then(|n| out.ids.items.object(ItemId(n))).map(oid);
    let story = |v: &Value| v.as_u64().and_then(|n| out.ids.stories.object(StoryId(n))).map(oid);
    let page = |v: &Value| v.as_u64().and_then(|n| usize::try_from(n).ok()).and_then(|n| out.page_at.get(n)).map(oid);
    let m = Mapper { item: &item, story: &story, page: &page };
    for key in SETTINGS_WITH_IDS {
        if let Some(v) = map.get_mut(*key) {
            map_setting(key, v, &m);
            if v.is_null() {
                map.remove(*key);
            }
        }
    }
    for v in map.values_mut() {
        swatches_out(v, &out.names.swatches);
    }
    Ok(map)
}

/// The document setup (`DocSettings`), its primary story as that story's object.
pub(crate) fn setup(doc: &Document, out: &Out<'_>) -> KdmResult<Value> {
    let mut map = to_map(&doc.settings)?;
    if let Some(v) = map.get("primaryStory").cloned() {
        match v.as_u64().and_then(|n| out.ids.stories.object(StoryId(n))) {
            Some(object) => {
                map.insert("primaryStory".into(), oid(object));
            }
            None => {
                map.remove("primaryStory");
            }
        }
    }
    let mut v = Value::Object(map);
    swatches_out(&mut v, &out.names.swatches);
    Ok(v)
}

/// A named list's objects in `after`'s order, and each one's partner in `before`.
struct Plan {
    objects: Vec<ObjectId>,
    partner: Vec<Option<usize>>,
}

/// The top-level items of `doc` in tree order, each with its spread (`None`: anchored in a story).
fn top_level(doc: &Document) -> Vec<(&Item, Option<u64>)> {
    let mut out = Vec::new();
    for spread in doc.spreads.iter().chain(&doc.parents) {
        out.extend(spread.items.iter().map(|i| (&**i, Some(spread.id.0))));
    }
    for story in doc.stories.values() {
        out.extend(story.objects.iter().map(|o| (&o.item, None)));
    }
    out
}

fn children(item: &Item) -> Vec<(&Item, Option<u64>)> {
    match &item.content {
        Content::Group { items } => items.iter().map(|i| (&**i, None)).collect(),
        _ => Vec::new(),
    }
}

struct Writer<'w, 'tx> {
    tx: &'w mut Tx<'tx>,
    ids: &'w mut Ids,
    blobs: &'w mut dyn BlobSink,
    /// Objects this write created: written in full, whatever `before` held.
    fresh: HashSet<ObjectId>,
    retired_items: HashSet<ObjectId>,
    wrote: bool,
}

impl<'w, 'tx> Writer<'w, 'tx> {
    fn new(tx: &'w mut Tx<'tx>, ids: &'w mut Ids, blobs: &'w mut dyn BlobSink) -> Self {
        Self { tx, ids, blobs, fresh: HashSet::new(), retired_items: HashSet::new(), wrote: false }
    }

    fn create(&mut self, ty: &str) -> KdmResult<ObjectId> {
        let object = self.tx.create(ty, placeholder(ty))?;
        self.fresh.insert(object.clone());
        self.wrote = true;
        Ok(object)
    }

    /// [`Writer::create`] for an object the engine numbers, storing its engine id (`number`), so
    /// every later read gives it the same id (design note section 2.5).
    fn create_numbered(&mut self, ty: &str, number: u64) -> KdmResult<ObjectId> {
        let object = self.create(ty)?;
        self.tx.set(&object, "number", json!(number))?;
        Ok(object)
    }

    fn write(mut self, before: Option<&Document>, after: &Document) -> KdmResult<bool> {
        check_unique(after)?;
        let root = root_id();
        let ids_before = self.ids.clone();
        let out_before = before.map(|b| out_of(b, &ids_before));

        // 1. Objects by local id, and named lists.
        let keys = |d: Option<&Document>, f: &dyn Fn(&Document) -> Vec<u64>| d.map(f).unwrap_or_default();
        let layer_keys = |d: &Document| d.layers.iter().map(|l| l.id.0).collect::<Vec<_>>();
        let spread_keys = |d: &Document| d.spreads.iter().chain(&d.parents).map(|s| s.id.0).collect::<Vec<_>>();
        let page_keys = |d: &Document| d.spreads.iter().chain(&d.parents).flat_map(|s| s.pages.iter().map(|p| p.id.0)).collect::<Vec<_>>();
        let asset_keys = |d: &Document| d.assets.keys().map(|a| a.0).collect::<Vec<_>>();
        let story_keys = |d: &Document| d.stories.keys().copied().collect::<Vec<_>>();
        let layers = self.plan_keyed("Layer", |ids| &mut ids.layers, |k| k, &keys(before, &layer_keys), &layer_keys(after))?;
        self.plan_keyed("Spread", |ids| &mut ids.spreads, |k| k, &keys(before, &spread_keys), &spread_keys(after))?;
        self.plan_keyed("Page", |ids| &mut ids.pages, |k| k, &keys(before, &page_keys), &page_keys(after))?;
        let assets = self.plan_keyed("Asset", |ids| &mut ids.assets, |k| k, &keys(before, &asset_keys), &asset_keys(after))?;
        let stories = self.plan_keyed("Story", |ids| &mut ids.stories, |k| k.0, &before.map(story_keys).unwrap_or_default(), &story_keys(after))?;
        let link_keys = |d: &Document| d.hyperlinks.iter().map(|h| h.id).collect::<Vec<_>>();
        let placeholder_keys = |d: &Document| d.data_merge.placeholders.iter().map(|p| p.id).collect::<Vec<_>>();
        let hyperlinks = self.plan_keyed("Hyperlink", |ids| &mut ids.hyperlinks, |k| k, &keys(before, &link_keys), &link_keys(after))?;
        let placeholders =
            self.plan_keyed("Placeholder", |ids| &mut ids.placeholders, |k| k, &keys(before, &placeholder_keys), &placeholder_keys(after))?;
        let named_before = before.map(named_values).transpose()?;
        let named_after = named_values(after)?;
        let mut named_plans = Vec::with_capacity(NAMED_LISTS);
        for (k, (_, ty)) in NAMED.iter().enumerate() {
            let b = named_before.as_ref().map_or(&[][..], |n| &n[k][..]);
            let plan = self.plan(ty, b, &ids_before.named[k].clone(), &named_after[k], |(_, name)| name)?;
            self.ids.named[k] = plan.objects.clone();
            named_plans.push(plan);
        }

        let swatches =
            self.plan("Swatch", before.map_or(&[][..], |b| &b.swatches[..]), &ids_before.swatches.clone(), &after.swatches, |s| &s.name)?;
        self.ids.swatches = swatches.objects.clone();
        let mut style_plans = Vec::with_capacity(FAMILIES);
        {
            let before_styles = before.map(|b| style_values(&b.styles)).transpose()?;
            let after_styles = style_values(&after.styles)?;
            for family in 0..FAMILIES {
                let b = before_styles.as_ref().map_or(&[][..], |s| &s[family][..]);
                let plan = self.plan("Style", b, &ids_before.styles[family].clone(), &after_styles[family], |(_, name)| name)?;
                self.ids.styles[family] = plan.objects.clone();
                style_plans.push(plan);
            }
        }

        // 2. Items, inline objects, tables.
        let before_index = before.map(index).unwrap_or_default();
        let after_index = index(after);
        self.item_objects(&before_index, &after_index)?;
        for story in after.stories.values() {
            let old = before.and_then(|b| b.stories.get(&story.id)).filter(|_| !self.fresh_story(story.id));
            self.plan_story(old.map(|s| &**s), story)?;
        }
        if let Some(b) = before {
            for (id, story) in &b.stories {
                if !after.stories.contains_key(id) {
                    self.forget_story(story)?;
                }
            }
        }
        let ids_after = self.ids.clone();
        let out_after = out_of(after, &ids_after);

        // 3. The tree, and item registers.
        let before_spread: HashMap<ItemId, u64> =
            before.map(|b| top_level(b).into_iter().filter_map(|(i, s)| s.map(|s| (i.id, s))).collect()).unwrap_or_default();
        let old_top = before.map(top_level).unwrap_or_default();
        let new_top = top_level(after);
        let unchanged_top = before.is_some_and(|b| {
            b.spreads.len() == after.spreads.len()
                && b.parents.len() == after.parents.len()
                && b.spreads.iter().zip(&after.spreads).chain(b.parents.iter().zip(&after.parents)).all(|(x, y)| same_items(&x.items, &y.items))
                && old_top.len() == new_top.len()
                && old_top.iter().zip(&new_top).all(|((a, s), (b, t))| std::ptr::eq(*a, *b) && s == t)
        });
        if !unchanged_top {
            let ctx = TreeCtx { before_index: &before_index, before_spread: &before_spread, out_before: out_before.as_ref(), out_after: &out_after };
            let old: Vec<ItemId> = old_top.iter().map(|(i, _)| i.id).collect();
            self.reconcile(None, &old, &new_top, &ctx, 0)?;
        }
        // Ungroup must move surviving children before deleting their old parent. The tree is
        // pre-order; reverse it so a removed subtree is deleted children before parent.
        if !self.retired_items.is_empty() {
            for entry in self.tx.tree(&root, "items")?.into_iter().rev() {
                if self.retired_items.remove(&entry.object) {
                    self.tx.delete(&entry.object)?;
                }
            }
        }
        for object in self.retired_items.drain() {
            if self.tx.exists(&object) {
                self.tx.delete(&object)?;
            }
        }

        // 4. Fields, lists, settings.
        self.set(&root, "title", json!(after.title), before.map(|b| json!(b.title)))?;
        let setup_after = setup(after, &out_after)?;
        let setup_before = match (before, &out_before) {
            (Some(b), Some(o)) => Some(setup(b, o)?),
            _ => None,
        };
        self.set(&root, "setup", setup_after, setup_before)?;

        // Layers.
        let before_layers: HashMap<u64, &kition_layout_doc::Layer> =
            before.map(|b| b.layers.iter().map(|l| (l.id.0, l)).collect()).unwrap_or_default();
        for (layer, object) in after.layers.iter().zip(&layers) {
            let old = before_layers.get(&layer.id.0).copied().filter(|_| !self.fresh.contains(object));
            if old != Some(layer) {
                self.set_fields(object, old.map(fields::layer_fields).transpose()?, fields::layer_fields(layer)?)?;
            }
        }
        self.order(&root, "layers", &layers)?;

        // Spreads, pages, guides.
        let before_spreads: HashMap<u64, &kition_layout_doc::Spread> =
            before.map(|b| b.spreads.iter().chain(&b.parents).map(|s| (s.id.0, &**s)).collect()).unwrap_or_default();
        let mut spread_lists: [Vec<ObjectId>; 2] = Default::default();
        for (which, list) in [&after.spreads, &after.parents].into_iter().enumerate() {
            for spread in list {
                let object = self.ids.spreads.object(spread.id.0).cloned().ok_or_else(|| kit("a spread without its object"))?;
                let old = before_spreads.get(&spread.id.0).copied().filter(|_| !self.fresh.contains(&object));
                if old.is_some_and(|o| std::ptr::eq(o, &**spread)) {
                    spread_lists[which].push(object);
                    continue;
                }
                self.write_spread(&object, old, spread, out_before.as_ref(), &out_after)?;
                spread_lists[which].push(object);
            }
        }
        let [spread_objects, parent_objects] = spread_lists;
        self.order(&root, "spreads", &spread_objects)?;
        self.order(&root, "parents", &parent_objects)?;

        // Assets.
        let before_assets = before.map(|b| &b.assets);
        for ((id, asset), object) in after.assets.iter().zip(&assets) {
            let old = before_assets.and_then(|a| a.get(id)).filter(|_| !self.fresh.contains(object));
            if old.is_some_and(|o| fields::same_asset(o, asset)) {
                continue;
            }
            let old_fields = match old {
                Some(o) => Some(fields::asset_fields(o, self.blobs)?),
                None => None,
            };
            let new_fields = fields::asset_fields(asset, self.blobs)?;
            self.set_fields(object, old_fields, new_fields)?;
        }
        self.order_set(&root, "assets", &assets)?;

        // Swatches and styles.
        for (j, swatch) in after.swatches.iter().enumerate() {
            let Some(object) = swatches.objects.get(j).cloned() else { continue };
            let old = swatches
                .partner
                .get(j)
                .copied()
                .flatten()
                .and_then(|i| before.and_then(|b| b.swatches.get(i)))
                .filter(|_| !self.fresh.contains(&object));
            if old == Some(swatch) {
                continue;
            }
            let old_fields = match (old, &out_before) {
                (Some(o), Some(ob)) => Some(fields::swatch_fields(o, ob)?),
                _ => None,
            };
            self.set_fields(&object, old_fields, fields::swatch_fields(swatch, &out_after)?)?;
        }
        self.order(&root, "swatches", &swatches.objects)?;
        {
            let before_styles = before.map(|b| style_values(&b.styles)).transpose()?;
            let after_styles = style_values(&after.styles)?;
            for (family, plan) in style_plans.iter().enumerate() {
                for (j, (value, _)) in after_styles[family].iter().enumerate() {
                    let Some(object) = plan.objects.get(j).cloned() else { continue };
                    let old = plan
                        .partner
                        .get(j)
                        .copied()
                        .flatten()
                        .and_then(|i| before_styles.as_ref().and_then(|s| s[family].get(i)))
                        .filter(|_| !self.fresh.contains(&object));
                    let old_fields = match (old, &out_before) {
                        (Some((v, _)), Some(ob)) => Some(fields::style_fields(family, v, ob)?),
                        _ => None,
                    };
                    let new_fields = fields::style_fields(family, value, &out_after)?;
                    if old_fields.as_ref() == Some(&new_fields) {
                        continue;
                    }
                    self.write_style(&object, old_fields, new_fields)?;
                }
                self.order(&root, FAMILY_FIELDS[family], &plan.objects)?;
            }
        }

        // Stories: one whose text, hyperlink sources, placeholders and variables are as they were
        // is skipped.
        let marks_kept = |id: &StoryId| {
            out_before.as_ref().is_some_and(|ob| ob.ranges.get(id) == out_after.ranges.get(id) && ob.names.variables == out_after.names.variables)
        };
        for (story, object) in after.stories.values().zip(&stories) {
            let old = before.and_then(|b| b.stories.get(&story.id)).filter(|_| !self.fresh.contains(object));
            if old.is_some_and(|o| std::sync::Arc::ptr_eq(o, story)) && marks_kept(&story.id) {
                continue;
            }
            self.write_story(object, old.map(|s| &**s), story, &ids_before, out_before.as_ref(), &out_after)?;
        }
        self.order_set(&root, "stories", &stories)?;

        // Hyperlinks and data merge placeholders.
        let before_links: HashMap<u64, &kition_layout_doc::Hyperlink> =
            before.map(|b| b.hyperlinks.iter().map(|h| (h.id, h)).collect()).unwrap_or_default();
        for (link, object) in after.hyperlinks.iter().zip(&hyperlinks) {
            let old = before_links.get(&link.id).filter(|_| !self.fresh.contains(object));
            let old_fields = match (old, &out_before) {
                (Some(o), Some(ob)) => Some(fields::hyperlink_fields(o, ob)),
                _ => None,
            };
            self.set_fields(object, old_fields, fields::hyperlink_fields(link, &out_after))?;
        }
        self.order(&root, "hyperlinks", &hyperlinks)?;
        let before_placeholders: HashMap<u64, &kition_layout_doc::Placeholder> =
            before.map(|b| b.data_merge.placeholders.iter().map(|p| (p.id, p)).collect()).unwrap_or_default();
        for (p, object) in after.data_merge.placeholders.iter().zip(&placeholders) {
            let old = before_placeholders.get(&p.id).filter(|_| !self.fresh.contains(object));
            let old_fields = match (old, &out_before) {
                (Some(o), Some(ob)) => Some(fields::placeholder_fields(o, ob)),
                _ => None,
            };
            self.set_fields(object, old_fields, fields::placeholder_fields(p, &out_after))?;
        }
        self.order(&root, "placeholders", &placeholders)?;

        // Conditions, text variables, cross-reference formats.
        for (k, plan) in named_plans.iter().enumerate() {
            for (j, (value, _)) in named_after[k].iter().enumerate() {
                let Some(object) = plan.objects.get(j).cloned() else { continue };
                let old = plan
                    .partner
                    .get(j)
                    .copied()
                    .flatten()
                    .and_then(|i| named_before.as_ref().and_then(|n| n[k].get(i)))
                    .filter(|_| !self.fresh.contains(&object));
                self.set_fields(&object, old.map(|(v, _)| fields::named_fields(v)), fields::named_fields(value))?;
            }
            self.order(&root, NAMED[k].0, &plan.objects)?;
        }

        // Sections.
        let mut sections = Vec::with_capacity(after.sections.len());
        let old_sections = ids_before.sections.clone();
        for (i, section) in after.sections.iter().enumerate() {
            let new_fields = fields::section_fields(section, &out_after)?;
            let existing = old_sections.get(i).cloned().flatten();
            let Some(new_fields) = new_fields else {
                if let Some(object) = existing {
                    self.delete_deep(&object)?;
                }
                sections.push(None);
                continue;
            };
            let old_fields = match (before.and_then(|b| b.sections.get(i)), &out_before) {
                (Some(s), Some(ob)) if existing.is_some() => fields::section_fields(s, ob)?,
                _ => None,
            };
            let object = match existing {
                Some(object) => {
                    self.set_fields(&object, old_fields, new_fields)?;
                    object
                }
                None => {
                    self.wrote = true;
                    self.tx.create("Section", fields_map(new_fields))?
                }
            };
            sections.push(Some(object));
        }
        for object in old_sections.iter().skip(after.sections.len()).flatten() {
            self.delete_deep(object)?;
        }
        let listed: Vec<ObjectId> = sections.iter().flatten().cloned().collect();
        self.order(&root, "sections", &listed)?;
        self.ids.sections = sections;

        // Settings.
        let settings_after = settings(after, &out_after)?;
        let settings_before = match (before, &out_before) {
            (Some(b), Some(o)) => settings(b, o)?,
            _ => Map::new(),
        };
        if settings_after != settings_before {
            for (key, v) in &settings_after {
                if settings_before.get(key) != Some(v) {
                    self.tx.map_set(&root, "settings", key, v.clone())?;
                    self.wrote = true;
                }
            }
            for key in settings_before.keys() {
                if !settings_after.contains_key(key) {
                    self.tx.map_delete(&root, "settings", key)?;
                    self.wrote = true;
                }
            }
        }

        // The ids the view gave objects that do not store them (a state read without the
        // session's repair, `KitModel::load`: objects storing no number, or one another keeps):
        // an edit stores them, so they last.
        if self.wrote {
            for (object, number) in std::mem::take(&mut self.ids.unstored) {
                if self.tx.exists(&object) {
                    self.tx.set(&object, "number", json!(number))?;
                }
            }
        }
        Ok(self.wrote)
    }

    fn fresh_story(&self, id: StoryId) -> bool {
        self.ids.stories.object(id).is_none_or(|o| self.fresh.contains(o))
    }

    fn set(&mut self, object: &ObjectId, field: &str, v: Value, old: Option<Value>) -> KdmResult<()> {
        let differs = match &old {
            Some(old) => *old != v,
            None => !v.is_null(),
        };
        if differs {
            self.tx.set(object, field, v)?;
            self.wrote = true;
        }
        Ok(())
    }

    fn order(&mut self, owner: &ObjectId, field: &str, objects: &[ObjectId]) -> KdmResult<()> {
        self.wrote |= sync_list(self.tx, owner, field, objects)? > 0;
        Ok(())
    }

    /// For lists whose order means nothing (stories, assets, a story's tables): see [`sync_set`].
    fn order_set(&mut self, owner: &ObjectId, field: &str, objects: &[ObjectId]) -> KdmResult<()> {
        self.wrote |= sync_set(self.tx, owner, field, objects)? > 0;
        Ok(())
    }

    /// Sets each field of `object` whose value differs from `before` (all of them, `null`s aside,
    /// when there is no `before`).
    fn set_fields(&mut self, object: &ObjectId, before: Option<Fields>, after: Fields) -> KdmResult<()> {
        for (i, (field, v)) in after.into_iter().enumerate() {
            let old = before.as_ref().and_then(|b| b.get(i)).map(|(_, v)| v);
            let differs = match old {
                Some(old) => *old != v,
                None => !v.is_null(),
            };
            if differs {
                self.tx.set(object, field, v)?;
                self.wrote = true;
            }
        }
        Ok(())
    }

    /// A style's registers, and its `attrs` map key by key.
    fn write_style(&mut self, object: &ObjectId, before: Option<Fields>, after: Fields) -> KdmResult<()> {
        let mut registers = Vec::new();
        let mut old_registers = before.as_ref().map(|_| Vec::new());
        let mut attrs_after = Map::new();
        let mut attrs_before = Map::new();
        for (field, v) in after {
            if field == "attrs" {
                if let Value::Object(m) = v {
                    attrs_after = m;
                }
            } else {
                registers.push((field, v));
            }
        }
        if let (Some(before), Some(old)) = (before, old_registers.as_mut()) {
            for (field, v) in before {
                if field == "attrs" {
                    if let Value::Object(m) = v {
                        attrs_before = m;
                    }
                } else {
                    old.push((field, v));
                }
            }
        }
        self.set_fields(object, old_registers, registers)?;
        for (key, v) in &attrs_after {
            if attrs_before.get(key) != Some(v) {
                self.tx.map_set(object, "attrs", key, v.clone())?;
                self.wrote = true;
            }
        }
        for key in attrs_before.keys() {
            if !attrs_after.contains_key(key) {
                self.tx.map_delete(object, "attrs", key)?;
                self.wrote = true;
            }
        }
        Ok(())
    }

    /// Deletes `object` and the objects it owns (a story's inline objects and tables, a table's
    /// rows, columns and cells, a spread's pages and guides).
    fn delete_deep(&mut self, object: &ObjectId) -> KdmResult<()> {
        if !self.tx.exists(object) {
            return Ok(());
        }
        let owned: &[&str] = match self.tx.object_type(object)?.as_str() {
            "Story" => &["inlines", "tables"],
            "Table" => &["rows", "columns"],
            "Spread" => &["pages", "guides"],
            _ => &[],
        };
        if self.tx.object_type(object)? == "Table" {
            for (_, cell) in self.tx.map_entries(object, "cells")? {
                if let Some(cell) = fields::as_object(&cell)
                    && self.tx.exists(&cell)
                {
                    self.tx.delete(&cell)?;
                }
            }
        }
        for field in owned {
            for child in self.tx.list_items(object, field)? {
                self.delete_deep(&child)?;
            }
        }
        self.wrote = true;
        self.tx.delete(object)
    }

    /// Pairs `after` with `before` (whose objects `before_objects` are, in order) by content,
    /// then by name; deletes the objects of unpaired `before` elements and creates (empty) ones
    /// for unpaired `after` elements.
    fn plan<T: Serialize + PartialEq>(
        &mut self,
        ty: &str,
        before: &[T],
        before_objects: &[ObjectId],
        after: &[T],
        name: impl Fn(&T) -> &String,
    ) -> KdmResult<Plan> {
        let before = if before.len() == before_objects.len() {
            before
        } else {
            for object in before_objects {
                self.delete_deep(object)?;
            }
            &[]
        };
        let partner: Vec<Option<usize>> = if before == after {
            (0..after.len()).map(Some).collect()
        } else {
            let b: Vec<(Value, &String)> = before.iter().map(|x| Ok((serde_json::to_value(x).map_err(kit)?, name(x)))).collect::<KdmResult<_>>()?;
            let a: Vec<(Value, &String)> = after.iter().map(|x| Ok((serde_json::to_value(x).map_err(kit)?, name(x)))).collect::<KdmResult<_>>()?;
            align(&b, &a)
        };
        let kept: HashSet<usize> = partner.iter().flatten().copied().collect();
        if before.len() == before_objects.len() {
            for (i, object) in before_objects.iter().enumerate() {
                if !kept.contains(&i) {
                    self.delete_deep(object)?;
                }
            }
        }
        let mut objects = Vec::with_capacity(after.len());
        for p in &partner {
            match p.and_then(|i| before_objects.get(i)) {
                Some(object) => objects.push(object.clone()),
                None => objects.push(self.create(ty)?),
            }
        }
        Ok(Plan { objects, partner })
    }

    /// Objects by local id: deletes those of `before` keys `after` lacks, creates (empty) ones for
    /// new keys. Answers `after`'s objects in order.
    fn plan_keyed<K: Copy + Eq + Hash>(
        &mut self,
        ty: &str,
        map: impl Fn(&mut Ids) -> &mut IdMap<K>,
        number: impl Fn(K) -> u64,
        before: &[K],
        after: &[K],
    ) -> KdmResult<Vec<ObjectId>> {
        let after_set: HashSet<K> = after.iter().copied().collect();
        let before_set: HashSet<K> = before.iter().copied().collect();
        for key in before {
            if !after_set.contains(key)
                && let Some(object) = map(self.ids).remove_local(*key)
            {
                self.delete_deep(&object)?;
            }
        }
        let mut objects = Vec::with_capacity(after.len());
        for key in after {
            match map(self.ids).object(*key).cloned() {
                Some(object) if before_set.contains(key) && self.tx.exists(&object) => objects.push(object),
                _ => {
                    let object = self.create_numbered(ty, number(*key))?;
                    map(self.ids).insert(object.clone(), *key);
                    objects.push(object);
                }
            }
        }
        Ok(objects)
    }

    /// Deletes the objects of the items `after` no longer holds and creates (empty) ones for its
    /// new items.
    fn item_objects(&mut self, before: &HashMap<ItemId, &Item>, after: &HashMap<ItemId, &Item>) -> KdmResult<()> {
        let mut gone: Vec<ItemId> = before.keys().filter(|id| !after.contains_key(id)).copied().collect();
        gone.sort_unstable();
        for id in gone {
            if let Some(object) = self.ids.items.remove_local(id) {
                self.ids.tree.remove(&object);
                if self.tx.exists(&object) {
                    self.retired_items.insert(object);
                }
                self.wrote = true;
            }
        }
        let mut new: Vec<ItemId> = after
            .keys()
            .filter(|id| !before.contains_key(id) || self.ids.items.object(**id).is_none_or(|o| !self.ids.tree.contains_key(o)))
            .copied()
            .collect();
        new.sort_unstable();
        for id in new {
            if let Some(stale) = self.ids.items.remove_local(id)
                && self.tx.exists(&stale)
            {
                self.retired_items.insert(stale);
            }
            let object = self.create_numbered("Item", id.0)?;
            self.ids.items.insert(object, id);
        }
        Ok(())
    }

    /// Pairs a story's inline objects and tables with its previous version's, creating and
    /// deleting objects.
    fn plan_story(&mut self, before: Option<&Story>, after: &Story) -> KdmResult<()> {
        if before.is_some_and(|b| std::ptr::eq(b, after)) {
            return Ok(());
        }
        // Inline objects, by key.
        let old: Vec<(InlineKey, ObjectId)> =
            if before.is_some() { self.ids.inlines.get(&after.id).cloned().unwrap_or_default() } else { Vec::new() };
        let mut pool: HashMap<InlineKey, Vec<ObjectId>> = HashMap::new();
        for (key, object) in old.into_iter().rev() {
            pool.entry(key).or_default().push(object);
        }
        let mut list = Vec::new();
        for (key, _) in inline_list(after).into_iter().flatten() {
            let object = match pool.get_mut(&key).and_then(Vec::pop) {
                Some(object) if self.tx.exists(&object) => object,
                _ if has_own_id(key.kind) && key.kind != InlineKind::AnchoredObject => self.create_numbered("Inline", key.local)?,
                _ => self.create("Inline")?,
            };
            list.push((key, object));
        }
        for object in pool.into_values().flatten() {
            self.delete_deep(&object)?;
        }
        self.ids.inlines.insert(after.id, list);

        // Tables, by number; their rows and columns by content.
        let old_tables: Vec<u64> = before.map(|b| b.tables.keys().copied().collect()).unwrap_or_default();
        for table in &old_tables {
            if !after.tables.contains_key(table)
                && let Some(t) = self.ids.tables.remove(&(after.id, *table))
                && let Some(object) = t.object
            {
                self.delete_deep(&object)?;
            }
        }
        for (number, table) in &after.tables {
            let key = (after.id, *number);
            let old_table = before.and_then(|b| b.tables.get(number));
            let mut t = self.ids.tables.get(&key).cloned().filter(|_| old_table.is_some()).unwrap_or_default();
            let object = match t.object.clone() {
                Some(object) if self.tx.exists(&object) => object,
                _ => self.create_numbered("Table", *number)?,
            };
            t.object = Some(object.clone());
            if old_table.is_some_and(|o| std::sync::Arc::ptr_eq(o, table))
                && t.rows.len() == table.rows.len()
                && t.columns.len() == table.columns.len()
            {
                self.ids.tables.insert(key, t);
                continue;
            }
            let (old_rows, old_cols) = match old_table {
                Some(o) => (row_values(o)?, col_values(o)?),
                None => (Vec::new(), Vec::new()),
            };
            let new_rows = row_values(table)?;
            let new_cols = col_values(table)?;
            let rows = self.pair_lines("TableRow", &object, &t.rows, &old_rows, &new_rows, &t.columns, true)?;
            let columns = self.pair_lines("TableColumn", &object, &t.columns, &old_cols, &new_cols, &t.rows, false)?;
            t.rows = rows.0;
            t.columns = columns.0;
            self.ids.tables.insert(key, t);
        }
        Ok(())
    }

    /// Pairs a table's rows (or columns) with the previous ones by content; deletes the unpaired
    /// old lines with their cells (across `other`, the old lines of the other direction).
    #[allow(clippy::too_many_arguments)]
    fn pair_lines(
        &mut self,
        ty: &str,
        table: &ObjectId,
        objects: &[ObjectId],
        before: &[Value],
        after: &[Value],
        other: &[ObjectId],
        rows: bool,
    ) -> KdmResult<(Vec<ObjectId>, Vec<Option<usize>>)> {
        let before = if before.len() == objects.len() { before } else { &[] };
        let b: Vec<(&Value, ())> = before.iter().map(|v| (v, ())).collect();
        let a: Vec<(&Value, ())> = after.iter().map(|v| (v, ())).collect();
        let partner = align(&b, &a);
        let kept: HashSet<usize> = partner.iter().flatten().copied().collect();
        for (i, object) in objects.iter().enumerate() {
            if before.is_empty() || !kept.contains(&i) {
                for line in other {
                    let key = if rows { cell_key(object, line) } else { cell_key(line, object) };
                    let cell = cell_id(table, &key);
                    if self.tx.exists(&cell) {
                        self.tx.delete(&cell)?;
                    }
                    if !self.tx.map_get(table, "cells", &key)?.is_null() {
                        self.tx.map_delete(table, "cells", &key)?;
                    }
                }
                if self.tx.exists(object) {
                    self.tx.delete(object)?;
                }
                self.wrote = true;
            }
        }
        let mut out = Vec::with_capacity(after.len());
        for p in &partner {
            match p.and_then(|i| objects.get(i)).filter(|_| !before.is_empty()) {
                Some(object) => out.push(object.clone()),
                None => out.push(self.create(ty)?),
            }
        }
        Ok((out, partner))
    }

    /// Forgets (and deletes) what a deleted story owned.
    fn forget_story(&mut self, story: &Story) -> KdmResult<()> {
        self.ids.inlines.remove(&story.id);
        for number in story.tables.keys() {
            self.ids.tables.remove(&(story.id, *number));
        }
        Ok(())
    }

    /// Makes the tree under `parent` hold `after` (it held `before`, by id; nothing if `parent`
    /// is new), writing each changed item's registers on the way.
    fn reconcile(
        &mut self,
        parent: Option<&NodeRef>,
        before: &[ItemId],
        after: &[(&Item, Option<u64>)],
        ctx: &TreeCtx<'_, '_>,
        depth: usize,
    ) -> KdmResult<()> {
        if depth > MAX_DEPTH {
            return Err(kdm_err(format!("groups nest deeper than {MAX_DEPTH}")));
        }
        let root = root_id();
        let before_pos: HashMap<ItemId, usize> = before.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        // Only the order among the nodes of one spread (or of one group's children) is read, so a
        // node of the same spread as before keeps its place while every node of its spread before
        // it in `after` kept its place, in order; the others go last (a tree move or create
        // appends), in `after` order. So adding an item to a spread moves no item of another
        // spread: an agent's draft adding one merges with the person adding one (both are inserts,
        // not moves of the same nodes, rule D7).
        let mut last: HashMap<Option<u64>, usize> = HashMap::new();
        let mut broken: HashSet<Option<u64>> = HashSet::new();
        let mut keep = Vec::with_capacity(after.len());
        for (item, spread) in after {
            let fresh = self.ids.items.object(item.id).is_none_or(|o| self.fresh.contains(o));
            // A top-level item that changed spread moves: its old place says nothing about its
            // order among the items of its new spread.
            let same_spread = parent.is_some() || ctx.before_spread.get(&item.id).copied() == *spread;
            let pos = before_pos.get(&item.id).copied().filter(|_| !fresh && same_spread);
            let kept = !broken.contains(spread) && pos.is_some_and(|pos| last.get(spread).is_none_or(|previous| pos > *previous));
            match (kept, pos) {
                (true, Some(pos)) => {
                    last.insert(*spread, pos);
                }
                _ => {
                    broken.insert(*spread);
                }
            }
            keep.push(kept);
        }
        for (k, (item, spread)) in after.iter().enumerate() {
            let object = self.ids.items.object(item.id).cloned().ok_or_else(|| kdm_err(format!("item {} has no replica object", item.id)))?;
            let fresh = self.fresh.contains(&object);
            let old = if fresh { None } else { ctx.before_index.get(&item.id).copied() };
            let old_spread = ctx.before_spread.get(&item.id).copied();
            let same = old.is_some_and(|o| std::ptr::eq(o, *item)) && old_spread == *spread;
            if !same {
                let spread_object = spread.and_then(|s| self.ids.spreads.object(s).cloned());
                let new_fields = fields::item_fields(item, spread_object.as_ref(), ctx.out_after)?;
                let old_fields = match (old, ctx.out_before) {
                    (Some(o), Some(ob)) => {
                        let old_spread_object = old_spread.and_then(|s| ob.ids.spreads.object(s).cloned());
                        Some(fields::item_fields(o, old_spread_object.as_ref(), ob)?)
                    }
                    _ => None,
                };
                self.set_fields(&object, old_fields, new_fields)?;
            }
            if !keep.get(k).copied().unwrap_or(false) {
                match self.ids.tree.get(&object).cloned() {
                    Some(node) if !fresh => self.tx.tree_move(&root, "items", &node, parent)?,
                    _ => {
                        let node = self.tx.tree_create(&root, "items", parent, &object)?;
                        self.ids.tree.insert(object.clone(), node);
                    }
                }
                self.wrote = true;
            }
            let kids = children(item);
            if !matches!(item.content, Content::Group { .. }) || old.is_some_and(|o| std::ptr::eq(o, *item)) {
                continue;
            }
            let old_kids: Vec<ItemId> = old.map(|o| children(o).into_iter().map(|(i, _)| i.id).collect()).unwrap_or_default();
            let node = self.ids.tree.get(&object).cloned().ok_or_else(|| kdm_err(format!("item {} has no tree node", item.id)))?;
            self.reconcile(Some(&node), &old_kids, &kids, ctx, depth + 1)?;
        }
        Ok(())
    }

    fn write_spread(
        &mut self,
        object: &ObjectId,
        old: Option<&kition_layout_doc::Spread>,
        spread: &kition_layout_doc::Spread,
        out_before: Option<&Out<'_>>,
        out_after: &Out<'_>,
    ) -> KdmResult<()> {
        let registers = |s: &kition_layout_doc::Spread, out: &Out<'_>| -> Fields {
            let info = s.parent.as_ref();
            vec![
                ("name", info.map(|p| json!(p.name)).unwrap_or(Value::Null)),
                ("prefix", info.map(|p| json!(p.prefix)).unwrap_or(Value::Null)),
                ("parentLabel", info.map(|p| json!(p.label())).unwrap_or(Value::Null)),
                ("basedOn", info.and_then(|p| p.based_on).and_then(|b| out.ids.spreads.object(b.0)).map(oid).unwrap_or(Value::Null)),
                ("allowShuffle", json!(s.allow_shuffle)),
            ]
        };
        let old_registers = match (old, out_before) {
            (Some(o), Some(ob)) => Some(registers(o, ob)),
            _ => None,
        };
        self.set_fields(object, old_registers, registers(spread, out_after))?;
        let before_pages: HashMap<u64, &kition_layout_doc::Page> = old.map(|o| o.pages.iter().map(|p| (p.id.0, p)).collect()).unwrap_or_default();
        let mut pages = Vec::with_capacity(spread.pages.len());
        let mut guide_objects = Vec::new();
        let old_guides = self.ids.guides.get(&spread.id.0).cloned().unwrap_or_default();
        let old_flat: Vec<(u64, &kition_layout_doc::Guide)> =
            old.map(|o| o.pages.iter().flat_map(|p| p.guides.iter().map(move |g| (p.id.0, g))).collect()).unwrap_or_default();
        let mut g = 0usize;
        for page in &spread.pages {
            let page_object = self.ids.pages.object(page.id.0).cloned().ok_or_else(|| kit("a page without its object"))?;
            let old_page = before_pages.get(&page.id.0).copied().filter(|_| !self.fresh.contains(&page_object));
            let new_fields = fields::page_fields(page, out_after)?;
            let old_fields = match (old_page, out_before) {
                (Some(p), Some(ob)) => Some(fields::page_fields(p, ob)?),
                _ => None,
            };
            self.set_fields(&page_object, old_fields, new_fields)?;
            let overridden = fields::page_overridden(page, out_after);
            let unchanged = old_page.is_some_and(|p| p.overridden == page.overridden) && out_before.is_some();
            if !unchanged {
                self.order_set(&page_object, "overridden", &overridden)?;
            }
            for guide in &page.guides {
                let existing = old_guides.get(g).cloned().filter(|o| self.tx.exists(o));
                let new_fields = fields::guide_fields(guide, &page_object, out_after)?;
                let object = match existing {
                    Some(object) => {
                        let old_fields = match (old_flat.get(g), out_before) {
                            (Some((page_id, old_guide)), Some(ob)) => {
                                let old_page_object = ob.ids.pages.object(*page_id).cloned().unwrap_or_else(|| page_object.clone());
                                Some(fields::guide_fields(old_guide, &old_page_object, ob)?)
                            }
                            _ => None,
                        };
                        self.set_fields(&object, old_fields, new_fields)?;
                        object
                    }
                    None => {
                        let object = self.tx.create("Guide", fields_map(new_fields))?;
                        self.wrote = true;
                        object
                    }
                };
                guide_objects.push(object);
                g += 1;
            }
            pages.push(page_object);
        }
        for object in old_guides.iter().skip(g) {
            self.delete_deep(object)?;
        }
        self.order(object, "pages", &pages)?;
        self.order(object, "guides", &guide_objects)?;
        self.ids.guides.insert(spread.id.0, guide_objects);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn write_story(
        &mut self,
        object: &ObjectId,
        old: Option<&Story>,
        story: &Story,
        ids_before: &Ids,
        out_before: Option<&Out<'_>>,
        out_after: &Out<'_>,
    ) -> KdmResult<()> {
        // Options.
        let new_options = fields::story_options(story, out_after)?;
        let old_options = match (old, out_before) {
            (Some(o), Some(ob)) => Some(fields::story_options(o, ob)?),
            _ => None,
        };
        self.set(object, "options", new_options, old_options)?;
        // Frames.
        let frames: Vec<ObjectId> = story.frames.iter().filter_map(|f| self.ids.items.object(*f).cloned()).collect();
        self.order(object, "frames", &frames)?;
        // Inline objects.
        let after_list = inline_list(story);
        let after_objects = self.ids.inlines.get(&story.id).cloned().unwrap_or_default();
        let before_list = old.map(inline_list).unwrap_or_default();
        let before_objects = if old.is_some() { ids_before.inlines.get(&story.id).cloned().unwrap_or_default() } else { Vec::new() };
        let before_placed = placeholder_objects(&before_list, &before_objects);
        let before_by_object: HashMap<&ObjectId, InlineRef<'_>> =
            before_list.iter().zip(before_placed.iter()).filter_map(|(e, o)| Some((o.as_ref()?, e.as_ref()?.1))).collect();
        let placed = placeholder_objects(&after_list, &after_objects);
        for (entry, inline_object) in after_list.iter().zip(&placed) {
            let (Some((_, r)), Some(inline_object)) = (entry, inline_object) else { continue };
            let old_r = before_by_object.get(inline_object).filter(|_| !self.fresh.contains(inline_object));
            if old_r.is_some_and(|o| same_inline(o, r)) {
                continue;
            }
            let (new_fields, new_runs) = inline_fields(r, out_after)?;
            let (old_fields, old_runs) = match (old_r, out_before) {
                (Some(o), Some(ob)) => {
                    let (f, runs) = inline_fields(o, ob)?;
                    (Some(f), runs)
                }
                _ => (None, Vec::new()),
            };
            self.set_fields(inline_object, old_fields, new_fields)?;
            self.text(inline_object, &old_runs, &new_runs)?;
        }
        let inline_order: Vec<ObjectId> = after_objects.iter().map(|(_, o)| o.clone()).collect();
        self.order(object, "inlines", &inline_order)?;
        // Tables.
        let mut table_order = Vec::new();
        for (number, table) in &story.tables {
            let Some(t) = self.ids.tables.get(&(story.id, *number)).cloned() else { continue };
            let Some(table_object) = t.object.clone() else { continue };
            table_order.push(table_object.clone());
            let old_table = old.and_then(|o| o.tables.get(number)).filter(|_| !self.fresh.contains(&table_object));
            if old_table.is_some_and(|o| std::sync::Arc::ptr_eq(o, table)) {
                continue;
            }
            let old_t = ids_before.tables.get(&(story.id, *number)).cloned().unwrap_or_default();
            self.write_table(&table_object, old_table.map(|o| &**o), table, &old_t, &t, out_before, out_after)?;
        }
        self.order_set(object, "tables", &table_order)?;
        // Text.
        let new_table = |n: u64| out_after.ids.table(story.id, n).map(oid);
        let ranges = |out: &Out<'_>, id: StoryId| out.ranges.get(&id).cloned().unwrap_or_default();
        let new_ranges = ranges(out_after, story.id);
        let new_runs = story_runs(story, &out_after.names, &Anchors { table: &new_table, inlines: Some(&placed), ranges: &new_ranges })?;
        let old_runs = match (old, out_before) {
            (Some(o), Some(ob)) => {
                let old_table = |n: u64| ob.ids.table(o.id, n).map(oid);
                let old_ranges = ranges(ob, o.id);
                story_runs(o, &ob.names, &Anchors { table: &old_table, inlines: Some(&before_placed), ranges: &old_ranges })?
            }
            _ => Vec::new(),
        };
        self.text(object, &old_runs, &new_runs)
    }

    fn text(&mut self, object: &ObjectId, old: &[TextRun], new: &[TextRun]) -> KdmResult<()> {
        if old == new {
            return Ok(());
        }
        write_rich_text(self.tx, object, "text", old, new).map_err(kit)?;
        self.wrote = true;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn write_table(
        &mut self,
        object: &ObjectId,
        old: Option<&kition_layout_doc::Table>,
        table: &kition_layout_doc::Table,
        old_t: &TableIds,
        t: &TableIds,
        out_before: Option<&Out<'_>>,
        out_after: &Out<'_>,
    ) -> KdmResult<()> {
        let new_fields = fields::table_fields(table, out_after)?;
        let old_fields = match (old, out_before) {
            (Some(o), Some(ob)) => Some(fields::table_fields(o, ob)?),
            _ => None,
        };
        self.set_fields(object, old_fields, new_fields)?;
        // Rows and columns: their own props, then the cells.
        let old_row_index: HashMap<&ObjectId, usize> = old_t.rows.iter().enumerate().map(|(i, o)| (o, i)).collect();
        let old_col_index: HashMap<&ObjectId, usize> = old_t.columns.iter().enumerate().map(|(i, o)| (o, i)).collect();
        for (r, row) in table.rows.iter().enumerate() {
            let Some(row_object) = t.rows.get(r) else { continue };
            let old_row = old.zip(old_row_index.get(row_object)).and_then(|(o, i)| o.rows.get(*i));
            let new_v = fields::line_props(row)?;
            self.set(row_object, "props", new_v, old_row.map(fields::line_props).transpose()?)?;
        }
        for (c, column) in table.columns.iter().enumerate() {
            let Some(col_object) = t.columns.get(c) else { continue };
            let old_col = old.zip(old_col_index.get(col_object)).and_then(|(o, i)| o.columns.get(*i));
            let new_v = fields::line_props(column)?;
            self.set(col_object, "props", new_v, old_col.map(fields::line_props).transpose()?)?;
        }
        self.order(object, "rows", &t.rows)?;
        self.order(object, "columns", &t.columns)?;
        let nc = table.columns.len();
        for (r, row_object) in t.rows.iter().enumerate() {
            for (c, col_object) in t.columns.iter().enumerate() {
                let Some(cell) = table.cells.get(r * nc + c) else { continue };
                let old_cell: Option<&Cell> = match (old, old_row_index.get(row_object), old_col_index.get(col_object)) {
                    (Some(o), Some(br), Some(bc)) => o.cells.get(br * o.columns.len() + bc),
                    _ => None,
                };
                let key = cell_key(row_object, col_object);
                let id = cell_id(object, &key);
                let exists = self.tx.exists(&id);
                if exists && old_cell.is_some_and(|o| o == cell) {
                    continue;
                }
                // Every cell has its object, empty ones too, created with its row or column: two
                // people typing into one empty cell then type into one text (section 2.2).
                if !exists {
                    self.tx.create_with_id("Cell", &id, Map::new())?;
                    self.wrote = true;
                }
                if fields::as_object(&self.tx.map_get(object, "cells", &key)?) != Some(id.clone()) {
                    self.tx.map_set(object, "cells", &key, oid(&id))?;
                    self.wrote = true;
                }
                let new = fields::cell_fields(cell, out_after)?;
                let old = match (old_cell, out_before) {
                    (Some(o), Some(ob)) if exists => fields::cell_fields(o, ob)?,
                    _ => None,
                };
                let empty = || -> Fields { vec![("span", Value::Null), ("style", Value::Null), ("props", Value::Null)] };
                let (old_fields, old_runs) = old.unwrap_or_else(|| (empty(), Vec::new()));
                let (new_fields, new_runs) = new.unwrap_or_else(|| (empty(), Vec::new()));
                self.set_fields(&id, Some(old_fields), new_fields)?;
                self.text(&id, &old_runs, &new_runs)?;
            }
        }
        Ok(())
    }
}

struct TreeCtx<'a, 'o> {
    before_index: &'a HashMap<ItemId, &'a Item>,
    before_spread: &'a HashMap<ItemId, u64>,
    out_before: Option<&'a Out<'o>>,
    out_after: &'a Out<'o>,
}

fn same_items(a: &[std::sync::Arc<Item>], b: &[std::sync::Arc<Item>]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| std::sync::Arc::ptr_eq(x, y))
}

/// A table's rows as values for pairing: each row with its cells.
fn row_values(t: &kition_layout_doc::Table) -> KdmResult<Vec<Value>> {
    let nc = t.columns.len();
    (0..t.rows.len())
        .map(|r| {
            let cells: Vec<&Cell> = (0..nc).filter_map(|c| t.cells.get(r * nc + c)).collect();
            Ok(json!([serde_json::to_value(&t.rows[r]).map_err(kit)?, serde_json::to_value(cells).map_err(kit)?]))
        })
        .collect()
}

/// A table's columns as values for pairing: each column with its cells.
fn col_values(t: &kition_layout_doc::Table) -> KdmResult<Vec<Value>> {
    let nc = t.columns.len();
    (0..nc)
        .map(|c| {
            let cells: Vec<&Cell> = (0..t.rows.len()).filter_map(|r| t.cells.get(r * nc + c)).collect();
            Ok(json!([serde_json::to_value(&t.columns[c]).map_err(kit)?, serde_json::to_value(cells).map_err(kit)?]))
        })
        .collect()
}

/// Each style family's styles as (serialized value, name), for pairing.
fn style_values(styles: &kition_layout_doc::Styles) -> KdmResult<[Vec<(Value, String)>; FAMILIES]> {
    fn list<T: Serialize>(items: &[T], name: impl Fn(&T) -> &String) -> KdmResult<Vec<(Value, String)>> {
        items.iter().map(|s| Ok((serde_json::to_value(s).map_err(kit)?, name(s).clone()))).collect()
    }
    Ok([
        list(&styles.paragraph, |s| &s.name)?,
        list(&styles.character, |s| &s.name)?,
        list(&styles.object, |s| &s.name)?,
        list(&styles.table, |s| &s.name)?,
        list(&styles.cell, |s| &s.name)?,
    ])
}
