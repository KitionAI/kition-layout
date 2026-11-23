//! Replica -> `Document`. A first pass walks every list and the `items` tree (skipping entries
//! whose object is gone, and every object after its first listing) and gives each object its
//! engine id — the number it stores (`number`), else a fresh one above every number stored
//! ([`Minter`]) — so the second pass can translate every reference stored as data while it reads
//! fields. The document's id counter stays above every id the view ([`Known`]) handed out.
//!
//! What the reader does with a state a merge can leave (the validation rules of section 2.6
//! repair them in the replica; the reader never fails on them): an item whose layer is gone reads
//! on the first layer, a style reference to a deleted style reads as the family default, a swatch
//! reference to a deleted swatch as the name it had, a top-level item on no live spread that no
//! inline object anchors is not read, a text frame no story threads reads with story 0, a
//! hyperlink or data merge placeholder whose item or text is gone (or a hyperlink whose page is
//! gone) is not read, nor is a condition, text variable or cross-reference format a script wrote in
//! another shape. A story text that does not end with `\n` is refused (V11).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use kition_kit::kdm::{KdmError, ObjectId, Replica, TreeEntry};
use kition_kit::session::{BlobSource, TextRun};
use kition_layout_doc::{Condition, Content, Document, Item, ItemId, ParentInfo, Spread, SpreadId, StoryId, Styles, XrefFormat};
use serde_json::{Map, Value, json};

use crate::cells::{cell_id, cell_key};
use crate::fields::{self, In, as_object};
use crate::ids::{
    Domain, FAMILIES, FAMILY_FIELDS, Ids, InlineKey, InlineKind, Known, Minter, NAMED, NAMED_LISTS, TableIds, has_own_id, stored_number,
};
use crate::inline::{InlineValue, inline_from, push_inline};
use crate::marks::{HYPERLINK, PLACEHOLDER};
use crate::refs::{Mapper, SETTINGS_WITH_IDS, map_setting, max_id, swatches_in};
use crate::text::decode;
use crate::write::{STYLE_DEFAULTS, STYLE_SETTINGS};
use crate::{KdmResult, MAX_DEPTH, kdm_err, root_id};

/// Reads the whole document with fresh local ids.
pub fn read_document(replica: &Replica, blobs: &dyn BlobSource) -> KdmResult<(Document, Ids)> {
    read_with(replica, blobs, &Known::default())
}

/// Reads the whole document, its id counter above every id `known` remembers.
pub fn read_document_known(replica: &Replica, blobs: &dyn BlobSource, known: &Known) -> KdmResult<(Document, Ids)> {
    read_with(replica, blobs, known)
}

struct Reader<'r> {
    replica: &'r Replica,
    seen: HashSet<ObjectId>,
}

impl Reader<'_> {
    /// The live objects of a list, each at its first listing in the document.
    fn live(&mut self, owner: &ObjectId, field: &str) -> KdmResult<Vec<ObjectId>> {
        let mut out = Vec::new();
        for object in self.replica.list(owner, field)? {
            if self.replica.exists(&object) && self.seen.insert(object.clone()) {
                out.push(object);
            }
        }
        Ok(out)
    }
}

fn string(v: Value) -> String {
    match v {
        Value::String(s) => s,
        _ => String::new(),
    }
}

/// The document's structure as a read finds it (the first pass): every list and the `items` tree,
/// each live object at its first listing, and the objects that get an engine id in reading order.
struct Walk {
    layers: Vec<ObjectId>,
    spreads: Vec<ObjectId>,
    parents: Vec<ObjectId>,
    spread_pages: HashMap<ObjectId, Vec<ObjectId>>,
    stories: Vec<ObjectId>,
    assets: Vec<ObjectId>,
    style_objects: [Vec<ObjectId>; FAMILIES],
    swatches: Vec<ObjectId>,
    sections: Vec<ObjectId>,
    hyperlinks: Vec<ObjectId>,
    placeholders: Vec<ObjectId>,
    named: [Vec<ObjectId>; NAMED_LISTS],
    story_frames: HashMap<ObjectId, Vec<ObjectId>>,
    story_inlines: HashMap<ObjectId, Vec<ObjectId>>,
    story_tables: HashMap<ObjectId, Vec<ObjectId>>,
    /// The numbered inline objects (footnotes, endnotes, text anchors, editorial notes) by kind.
    inline_kinds: HashMap<ObjectId, InlineKind>,
    entries: Vec<TreeEntry>,
    kids: HashMap<usize, Vec<usize>>,
    roots: Vec<usize>,
    /// The tree entries read as items, in reading order.
    item_entries: Vec<usize>,
    /// Every object that gets an engine id, in reading order, with its domain.
    numbered: Vec<(Domain, ObjectId)>,
}

fn walk<'r>(replica: &'r Replica, root: &ObjectId) -> KdmResult<(Reader<'r>, Walk)> {
    let mut r = Reader { replica, seen: HashSet::new() };
    let mut numbered = Vec::new();
    let layers = r.live(root, "layers")?;
    numbered.extend(layers.iter().map(|o| (Domain::Layer, o.clone())));
    let spreads = r.live(root, "spreads")?;
    let parents = r.live(root, "parents")?;
    let mut spread_pages: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    for object in spreads.iter().chain(&parents) {
        numbered.push((Domain::Spread, object.clone()));
        let pages = r.live(object, "pages")?;
        numbered.extend(pages.iter().map(|o| (Domain::Page, o.clone())));
        spread_pages.insert(object.clone(), pages);
    }
    let stories = r.live(root, "stories")?;
    numbered.extend(stories.iter().map(|o| (Domain::Story, o.clone())));
    let assets = r.live(root, "assets")?;
    numbered.extend(assets.iter().map(|o| (Domain::Asset, o.clone())));
    let mut style_objects: [Vec<ObjectId>; FAMILIES] = Default::default();
    for (family, field) in FAMILY_FIELDS.iter().enumerate() {
        style_objects[family] = r.live(root, field)?;
    }
    let swatches = r.live(root, "swatches")?;
    let sections = r.live(root, "sections")?;
    let hyperlinks = r.live(root, "hyperlinks")?;
    numbered.extend(hyperlinks.iter().map(|o| (Domain::Hyperlink, o.clone())));
    let placeholders = r.live(root, "placeholders")?;
    numbered.extend(placeholders.iter().map(|o| (Domain::Placeholder, o.clone())));
    let mut named: [Vec<ObjectId>; NAMED_LISTS] = Default::default();
    for (k, (field, _)) in NAMED.iter().enumerate() {
        named[k] = r.live(root, field)?;
    }

    // Frames, inline objects and tables of every story.
    let mut story_frames: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    let mut framed = HashSet::new();
    let mut story_inlines: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    let mut story_tables: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    let mut inline_kinds: HashMap<ObjectId, InlineKind> = HashMap::new();
    for story in &stories {
        let frames: Vec<ObjectId> = replica.list(story, "frames")?.into_iter().filter(|f| replica.exists(f) && framed.insert(f.clone())).collect();
        story_frames.insert(story.clone(), frames);
        let inlines = r.live(story, "inlines")?;
        for inline in &inlines {
            if let Some(kind) = InlineKind::from_name(&string(replica.get(inline, "kind")?))
                && has_own_id(kind)
                && kind != InlineKind::AnchoredObject
            {
                numbered.push((Domain::Inline(story.clone(), kind), inline.clone()));
                inline_kinds.insert(inline.clone(), kind);
            }
        }
        story_inlines.insert(story.clone(), inlines);
        let tables = r.live(story, "tables")?;
        numbered.extend(tables.iter().map(|o| (Domain::Table(story.clone()), o.clone())));
        story_tables.insert(story.clone(), tables);
    }

    // The items tree.
    let entries = replica.tree(root, "items")?;
    let mut kids: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (i, parent) in crate::ids::tree_parents(&entries).into_iter().enumerate() {
        match parent {
            Some(p) => kids.entry(p).or_default().push(i),
            None => roots.push(i),
        }
    }
    let mut item_entries = Vec::new();
    let mut stack: Vec<(usize, usize)> = roots.iter().rev().map(|i| (*i, 0)).collect();
    while let Some((i, depth)) = stack.pop() {
        let object = &entries[i].object;
        if depth > MAX_DEPTH || !replica.exists(object) || !r.seen.insert(object.clone()) {
            continue;
        }
        item_entries.push(i);
        numbered.push((Domain::Item, object.clone()));
        if let Some(children) = kids.get(&i) {
            stack.extend(children.iter().rev().map(|c| (*c, depth + 1)));
        }
    }
    let walk = Walk {
        layers,
        spreads,
        parents,
        spread_pages,
        stories,
        assets,
        style_objects,
        swatches,
        sections,
        hyperlinks,
        placeholders,
        named,
        story_frames,
        story_inlines,
        story_tables,
        inline_kinds,
        entries,
        kids,
        roots,
        item_entries,
        numbered,
    };
    Ok((r, walk))
}

/// Every numbered object's engine id ([`Minter`]).
struct Numbering {
    ids: HashMap<ObjectId, u64>,
    /// The counter's floor: every counted id handed out or stored.
    counter: u64,
    /// The objects whose id is not the number they store, with that id, and whether they store
    /// a number at all (one another object of their domain keeps).
    unstored: Vec<(ObjectId, u64, bool)>,
}

fn number(replica: &Replica, walk: &Walk) -> KdmResult<Numbering> {
    let mut order = Vec::with_capacity(walk.numbered.len());
    for (domain, object) in &walk.numbered {
        order.push((domain.clone(), object.clone(), stored_number(&replica.get(object, "number")?)));
    }
    let mut minter = Minter::new(&order);
    let mut ids = HashMap::with_capacity(order.len());
    let mut unstored = Vec::new();
    for (domain, object, stored) in order {
        let id = minter.mint(&domain, &object);
        if stored != Some(id) {
            unstored.push((object.clone(), id, stored.is_some()));
        }
        ids.insert(object, id);
    }
    Ok(Numbering { ids, counter: minter.max(), unstored })
}

/// The numbers the session's repair stores: those of the objects that store none (made by a script
/// through the object model, or read from a file written before the numbers were stored), as every
/// read gives them, a function of the replica's state alone. Two objects that store one number
/// (made concurrently on two replicas) are not repaired here: every read tells them apart alike,
/// and the replica's next edit stores the new number, so a merge leaves no change to exchange.
pub(crate) fn number_repairs(replica: &Replica) -> KdmResult<Vec<(ObjectId, u64)>> {
    let root = root_id();
    if !replica.exists(&root) {
        return Ok(Vec::new());
    }
    let (_, walk) = walk(replica, &root)?;
    Ok(number(replica, &walk)?.unstored.into_iter().filter(|(_, _, numbered)| !numbered).map(|(object, id, _)| (object, id)).collect())
}

pub(crate) fn read_with(replica: &Replica, blobs: &dyn BlobSource, known: &Known) -> KdmResult<(Document, Ids)> {
    let root = root_id();
    if !replica.exists(&root) {
        return Err(KdmError::NotFound(root.to_string()));
    }
    let (mut r, walk) = walk(replica, &root)?;
    let Numbering { ids: locals, counter, unstored } = number(replica, &walk)?;
    let local = |object: &ObjectId| locals.get(object).copied().unwrap_or(0);
    let Walk {
        layers,
        spreads,
        parents,
        spread_pages,
        stories,
        assets,
        style_objects,
        swatches,
        sections,
        hyperlinks,
        placeholders,
        named,
        story_frames,
        story_inlines,
        story_tables,
        inline_kinds,
        entries,
        kids,
        roots,
        item_entries,
        numbered: _,
    } = walk;
    let mut ids = Ids { unstored: unstored.into_iter().map(|(object, id, _)| (object, id)).collect(), ..Ids::default() };
    let mut inn = In::default();

    // Pass 1: local ids.
    for object in &layers {
        let id = local(object);
        ids.layers.insert(object.clone(), id);
        inn.layers.insert(object.clone(), id);
    }
    inn.first_layer = layers.first().and_then(|o| inn.layers.get(o).copied()).unwrap_or(0);
    let mut absolute = 0usize;
    for (is_parent, list) in [(false, &spreads), (true, &parents)] {
        for object in list {
            let id = local(object);
            ids.spreads.insert(object.clone(), id);
            inn.spreads.insert(object.clone(), id);
            for page in spread_pages.get(object).into_iter().flatten() {
                let id = local(page);
                ids.pages.insert(page.clone(), id);
                inn.pages.insert(page.clone(), id);
                if !is_parent {
                    inn.page_index.insert(page.clone(), absolute);
                    absolute += 1;
                }
            }
        }
    }
    for object in &stories {
        let id = local(object);
        ids.stories.insert(object.clone(), StoryId(id));
        inn.stories.insert(object.clone(), id);
    }
    for object in &assets {
        let id = local(object);
        ids.assets.insert(object.clone(), id);
        inn.assets.insert(object.clone(), id);
    }
    for (family, objects) in style_objects.iter().enumerate() {
        for object in objects {
            inn.names.styles[family].insert(object.clone(), string(replica.get(object, "name")?));
        }
    }
    for object in &swatches {
        inn.names.swatches.insert(object.clone(), string(replica.get(object, "name")?));
    }
    // Conditions, text variables, cross-reference formats: a text variable instance names its
    // variable's object, read as its place in the list.
    let named_list = |k: usize| -> (Vec<Value>, Vec<ObjectId>) {
        let mut values = Vec::new();
        let mut objects = Vec::new();
        for object in &named[k] {
            let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
            let value = match k {
                0 => fields::named_from::<Condition>(&get).and_then(|v| serde_json::to_value(v).ok()),
                1 => fields::named_from::<kition_layout_doc::vars::TextVariable>(&get).and_then(|v| serde_json::to_value(v).ok()),
                _ => fields::named_from::<XrefFormat>(&get).and_then(|v| serde_json::to_value(v).ok()),
            };
            if let Some(value) = value {
                values.push(value);
                objects.push(object.clone());
            }
        }
        (values, objects)
    };
    let mut named_values: Vec<Value> = Vec::with_capacity(NAMED_LISTS);
    for (k, slot) in ids.named.iter_mut().enumerate() {
        let (values, objects) = named_list(k);
        if k == crate::ids::VARIABLES {
            inn.names.variables = objects.iter().enumerate().map(|(i, o)| (o.clone(), i)).collect();
        }
        named_values.push(Value::Array(values));
        *slot = objects;
    }
    let mut inline_locals: HashMap<ObjectId, u64> = HashMap::new();
    for (inline, kind) in &inline_kinds {
        let id = local(inline);
        inline_locals.insert(inline.clone(), id);
        if *kind == InlineKind::TextAnchor {
            inn.anchors.insert(inline.clone(), id);
        }
    }
    let table_locals: HashMap<ObjectId, u64> = story_tables.values().flatten().map(|table| (table.clone(), local(table))).collect();
    let mut item_local: HashMap<usize, u64> = HashMap::new();
    for i in item_entries {
        let object = &entries[i].object;
        let id = local(object);
        item_local.insert(i, id);
        ids.items.insert(object.clone(), ItemId(id));
        ids.tree.insert(object.clone(), entries[i].node.clone());
        inn.items.insert(object.clone(), id);
    }
    for (story, frames) in &story_frames {
        let Some(story_id) = inn.stories.get(story).copied() else { continue };
        for frame in frames {
            if let Some(item) = inn.items.get(frame) {
                inn.frame_story.entry(*item).or_insert(story_id);
            }
        }
    }

    // Pass 2: items.
    let build = |i: usize, inn: &In| -> KdmResult<Option<Item>> {
        fn rec(
            i: usize,
            entries: &[TreeEntry],
            kids: &HashMap<usize, Vec<usize>>,
            item_local: &HashMap<usize, u64>,
            replica: &Replica,
            inn: &In,
            depth: usize,
        ) -> KdmResult<Option<Item>> {
            let Some(id) = item_local.get(&i).copied() else { return Ok(None) };
            let object = &entries[i].object;
            let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
            let mut item = fields::item_from(id, &get, inn)?;
            if let Content::Group { items } = &mut item.content
                && depth <= MAX_DEPTH
            {
                for c in kids.get(&i).into_iter().flatten() {
                    if let Some(child) = rec(*c, entries, kids, item_local, replica, inn, depth + 1)? {
                        items.push(Arc::new(child));
                    }
                }
            }
            Ok(Some(item))
        }
        rec(i, &entries, &kids, &item_local, replica, inn, 0)
    };
    let mut on_spread: HashMap<ObjectId, Vec<Arc<Item>>> = HashMap::new();
    let mut anchored: HashMap<ObjectId, Item> = HashMap::new();
    for &i in &roots {
        if !item_local.contains_key(&i) {
            continue;
        }
        let object = entries[i].object.clone();
        let spread = as_object(&replica.get(&object, "spread")?).filter(|s| inn.spreads.contains_key(s));
        let Some(item) = build(i, &inn)? else { continue };
        match spread {
            Some(spread) => on_spread.entry(spread).or_default().push(Arc::new(item)),
            None => {
                anchored.insert(object, item);
            }
        }
    }

    // Spreads and pages.
    let mut read_spreads = |list: &[ObjectId], is_parent: bool| -> KdmResult<Vec<Arc<Spread>>> {
        let mut out = Vec::with_capacity(list.len());
        for object in list {
            let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
            let id = inn.spreads.get(object).copied().unwrap_or(0);
            let mut guides: HashMap<ObjectId, Vec<kition_layout_doc::Guide>> = HashMap::new();
            let mut guide_objects = Vec::new();
            for guide in replica.list(object, "guides")? {
                if !replica.exists(&guide) {
                    continue;
                }
                let gget = |field: &str| replica.get(&guide, field).unwrap_or(Value::Null);
                let (page, g) = fields::guide_from(&gget, &inn)?;
                if let Some(page) = page {
                    guides.entry(page).or_default().push(g);
                    guide_objects.push(guide);
                }
            }
            let mut pages = Vec::new();
            for page in spread_pages.get(object).into_iter().flatten() {
                let pget = |field: &str| replica.get(page, field).unwrap_or(Value::Null);
                let pid = inn.pages.get(page).copied().unwrap_or(0);
                let overridden: Vec<ObjectId> = replica.list(page, "overridden")?.into_iter().filter(|o| replica.exists(o)).collect();
                pages.push(fields::page_from(pid, &pget, &overridden, guides.remove(page).unwrap_or_default(), &inn)?);
            }
            let name = get("name");
            let prefix = get("prefix");
            let parent = if is_parent || !name.is_null() || !prefix.is_null() {
                let based_on = as_object(&get("basedOn")).and_then(|b| inn.spreads.get(&b)).map(|b| SpreadId(*b));
                Some(ParentInfo { prefix: string(prefix), name: string(name), based_on })
            } else {
                None
            };
            let parent = if is_parent || parent.as_ref().is_some_and(|p| !p.name.is_empty() || !p.prefix.is_empty()) { parent } else { None };
            out.push(Arc::new(Spread {
                id: SpreadId(id),
                pages,
                items: on_spread.remove(object).unwrap_or_default(),
                parent,
                allow_shuffle: get("allowShuffle").as_bool().unwrap_or(true),
            }));
            ids.guides.insert(id, guide_objects);
        }
        Ok(out)
    };
    let doc_spreads = read_spreads(&spreads, false)?;
    let doc_parents = read_spreads(&parents, true)?;

    // Stories.
    let mut doc_stories = BTreeMap::new();
    // The hyperlink sources and placeholders in the stories' text, by object.
    let mut text_ranges: HashMap<(&'static str, ObjectId), (StoryId, usize, usize)> = HashMap::new();
    for object in &stories {
        let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
        let sid = inn.stories.get(object).copied().unwrap_or(0);
        let mut story = fields::story_from_options(sid, &get("options"), &inn)?;
        story.frames = story_frames.get(object).into_iter().flatten().filter_map(|f| inn.items.get(f).map(|i| ItemId(*i))).collect();
        // Tables.
        for table in story_tables.get(object).into_iter().flatten() {
            let local = table_locals.get(table).copied().unwrap_or(0);
            let tget = |field: &str| replica.get(table, field).unwrap_or(Value::Null);
            let rows = r.live(table, "rows")?;
            let columns = r.live(table, "columns")?;
            let cell_map: HashMap<String, Value> = replica.map_entries(table, "cells")?.into_iter().collect();
            let line = |o: &ObjectId| replica.get(o, "props").unwrap_or(Value::Null);
            let row_values: Vec<kition_layout_doc::Row> = rows.iter().map(|o| serde_json::from_value(line(o)).unwrap_or_default()).collect();
            let col_values: Vec<kition_layout_doc::Column> =
                columns.iter().map(|o| serde_json::from_value(line(o)).unwrap_or(kition_layout_doc::Column { width: 72.0 })).collect();
            let mut cells = Vec::with_capacity(rows.len() * columns.len());
            for row in &rows {
                for column in &columns {
                    let key = cell_key(row, column);
                    let cell = cell_map.get(&key).and_then(as_object).filter(|c| replica.exists(c)).unwrap_or_else(|| cell_id(table, &key));
                    if !replica.exists(&cell) || !cell_map.contains_key(&key) {
                        cells.push(kition_layout_doc::Cell::default());
                        continue;
                    }
                    let cget = |field: &str| replica.get(&cell, field).unwrap_or(Value::Null);
                    let runs = replica.text_delta(&cell, "text")?;
                    cells.push(fields::cell_from(&cget, &runs, &inn)?);
                }
            }
            story.tables.insert(local, Arc::new(fields::table_from(local, &tget, row_values, col_values, cells, &inn)?));
            ids.tables.insert((StoryId(sid), local), TableIds { object: Some(table.clone()), rows, columns });
        }
        // Text.
        let runs = replica.text_delta(object, "text")?;
        let decoded = decode(&runs, &inn.names, &|v| as_object(v).and_then(|o| table_locals.get(&o).copied()))?;
        story.text = decoded.text;
        story.chars = decoded.chars;
        story.paras = decoded.paras;
        for range in decoded.ranges {
            text_ranges.entry((range.mark, range.object)).or_insert((StoryId(sid), range.start, range.end));
        }
        // Inline objects, in text order.
        let live_inlines: HashSet<&ObjectId> = story_inlines.get(object).into_iter().flatten().collect();
        let mut used = HashSet::new();
        let mut keys = Vec::new();
        let mut counts: HashMap<InlineKind, u64> = HashMap::new();
        for (kind, inline) in &decoded.placeholders {
            let Some(inline) = inline.as_ref().filter(|i| live_inlines.contains(i) && used.insert((*i).clone())) else { continue };
            let iget = |field: &str| replica.get(inline, field).unwrap_or(Value::Null);
            if InlineKind::from_name(&string(iget("kind"))) != Some(*kind) {
                continue;
            }
            let local = inline_locals.get(inline).copied().unwrap_or(0);
            let runs: Vec<TextRun> = replica.text_delta(inline, "text")?;
            let value = inline_from(*kind, local, &iget, &runs, &inn)?;
            let (item, key_local) = match &value {
                InlineValue::Object(item, _) => {
                    let Some(item) = item.as_ref().and_then(|o| anchored.remove(o)) else { continue };
                    let id = item.id.0;
                    (Some(item), id)
                }
                _ if has_own_id(*kind) => (None, local),
                _ => {
                    let n = counts.entry(*kind).or_insert(0);
                    let at = *n;
                    *n += 1;
                    (None, at)
                }
            };
            push_inline(&mut story, value, item)?;
            keys.push((InlineKey { kind: *kind, local: key_local }, inline.clone()));
        }
        ids.inlines.insert(StoryId(sid), keys);
        doc_stories.insert(StoryId(sid), Arc::new(story));
    }

    // Styles and swatches.
    let settings_entries: Map<String, Value> = replica.map_entries(&root, "settings")?.into_iter().filter(|(_, v)| !v.is_null()).collect();
    let mut styles_json = Map::new();
    for (from, to) in STYLE_SETTINGS {
        if let Some(v) = settings_entries.get(*to) {
            styles_json.insert((*from).to_owned(), v.clone());
        }
    }
    if let Some(Value::Object(defaults)) = settings_entries.get("styleDefaults") {
        for key in STYLE_DEFAULTS {
            if let Some(v) = defaults.get(*key) {
                styles_json.insert((*key).to_owned(), v.clone());
            }
        }
    }
    for key in STYLE_DEFAULTS {
        styles_json.entry(*key).or_insert(json!(""));
    }
    let family_keys = ["paragraph", "character", "object", "table", "cell"];
    for (family, objects) in style_objects.iter().enumerate() {
        let mut list = Vec::with_capacity(objects.len());
        for object in objects {
            let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
            let attrs: Map<String, Value> = replica.map_entries(object, "attrs")?.into_iter().collect();
            let style: Value = fields::style_from(family, get("name"), get("basedOn"), get("next"), attrs, &inn)?;
            list.push(style);
        }
        styles_json.insert(family_keys[family].to_owned(), Value::Array(list));
        ids.styles[family] = objects.clone();
    }
    let styles: Styles = serde_json::from_value(Value::Object(styles_json)).map_err(kdm_err)?;
    let mut doc_swatches = Vec::with_capacity(swatches.len());
    for object in &swatches {
        let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
        doc_swatches.push(fields::swatch_from(&get, &inn)?);
    }
    ids.swatches = swatches.clone();

    // Layers, assets, sections.
    let mut doc_layers = Vec::with_capacity(layers.len());
    for object in &layers {
        let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
        doc_layers.push(fields::layer_from(inn.layers.get(object).copied().unwrap_or(0), &get)?);
    }
    let mut doc_assets = BTreeMap::new();
    for object in &assets {
        let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
        let id = inn.assets.get(object).copied().unwrap_or(0);
        doc_assets.insert(kition_layout_doc::AssetId(id), Arc::new(fields::asset_from(id, &get, blobs)?));
    }
    let mut doc_sections = Vec::new();
    let mut section_ids = Vec::new();
    for object in &sections {
        let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
        if let Some(section) = fields::section_from(&get, &inn)? {
            doc_sections.push(section);
            section_ids.push(Some(object.clone()));
        }
    }
    ids.sections = section_ids;
    let mut doc_hyperlinks = Vec::with_capacity(hyperlinks.len());
    for object in &hyperlinks {
        let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
        let text = text_ranges.get(&(HYPERLINK, object.clone()));
        if let Some(link) = fields::hyperlink_from(local(object), &get, text, &inn) {
            ids.hyperlinks.insert(object.clone(), link.id);
            doc_hyperlinks.push(link);
        }
    }
    let mut doc_placeholders = Vec::with_capacity(placeholders.len());
    for object in &placeholders {
        let get = |field: &str| replica.get(object, field).unwrap_or(Value::Null);
        let text = text_ranges.get(&(PLACEHOLDER, object.clone()));
        if let Some(p) = fields::placeholder_from(local(object), &get, text, &inn) {
            ids.placeholders.insert(object.clone(), p.id);
            doc_placeholders.push(p);
        }
    }

    // The document shell: settings, setup, title.
    let mut doc_json = settings_entries;
    for (_, to) in STYLE_SETTINGS {
        doc_json.remove(*to);
    }
    doc_json.remove("styleDefaults");
    let item_in = |v: &Value| inn.item(v);
    let story_in = |v: &Value| inn.story(v);
    let page_in = |v: &Value| as_object(v).and_then(|o| inn.page_index.get(&o)).map(|i| Value::from(*i));
    let mapper = Mapper { item: &item_in, story: &story_in, page: &page_in };
    for key in SETTINGS_WITH_IDS {
        if let Some(v) = doc_json.get_mut(*key) {
            map_setting(key, v, &mapper);
            if v.is_null() {
                doc_json.remove(*key);
            }
        }
    }
    for v in doc_json.values_mut() {
        swatches_in(v, &inn.names.swatches);
    }
    let mut setup = replica.get(&root, "setup")?;
    if let Some(map) = setup.as_object_mut()
        && let Some(primary) = map.get("primaryStory").cloned()
    {
        match inn.story(&primary) {
            Some(v) => {
                map.insert("primaryStory".into(), v);
            }
            None => {
                map.remove("primaryStory");
            }
        }
    }
    swatches_in(&mut setup, &inn.names.swatches);
    let raw_max = doc_json.values().map(max_id).max().unwrap_or(0);
    doc_json.insert("title".into(), Value::String(string(replica.get(&root, "title")?)));
    if !setup.is_null() {
        doc_json.insert("settings".into(), setup);
    }
    for key in ["spreads", "parents", "layers", "swatches", "sections"] {
        doc_json.insert(key.into(), json!([]));
    }
    doc_json.insert("stories".into(), json!({}));
    doc_json.insert("assets".into(), json!({}));
    doc_json.insert("styles".into(), json!({ "paragraph": [], "character": [], "object": [], "defaultParagraph": "", "defaultCharacter": "", "defaultTextFrame": "", "defaultGraphicFrame": "" }));
    doc_json.insert("nextId".into(), json!(0));
    for ((field, _), values) in NAMED.iter().zip(named_values) {
        doc_json.insert((*field).to_owned(), values);
    }
    doc_json.insert("hyperlinks".into(), json!([]));
    let mut doc: Document = serde_json::from_value(Value::Object(doc_json)).map_err(kdm_err)?;
    doc.hyperlinks = doc_hyperlinks;
    doc.data_merge.placeholders = doc_placeholders;
    doc.spreads = doc_spreads;
    doc.parents = doc_parents;
    doc.layers = doc_layers;
    doc.stories = doc_stories;
    doc.styles = Arc::new(styles);
    doc.swatches = doc_swatches;
    doc.sections = doc_sections;
    doc.assets = doc_assets;
    doc.next_id = counter.max(known.max).max(raw_max).min(kition_layout_doc::MAX_NEXT_ID);
    Ok((doc, ids))
}
