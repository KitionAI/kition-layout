//! Deterministic repair of a draft's merge and a restore (inside their one change and undo step),
//! and after an import, an undo or a redo (design note section 2.6, rules V1–V10, V12 and V13;
//! [`kition_kit::session::KitModel::validate`], [`crate::repair::repair_in`]). Every rule depends
//! only on the state, never on the order the changes came in or their actor. A repair never
//! inserts content (characters, pages, frames); it only drops, clamps or clears.
//!
//! - V1: list entries naming no live object are dropped (a page's `overridden` items too).
//! - V2: a top-level item on no live spread that no `Inline` names is deleted with its subtree
//!   (items the tree no longer reaches go too: they are invisible).
//! - V3: an object listed twice in one list, or in two lists of one kind, keeps its first entry
//!   by (list owner id, position).
//! - V4: a frame in two stories' `frames` stays in the story whose id sorts first.
//! - V5: a story with no live frame, that no setting names, is deleted.
//! - V6 (an `Inline` or `Table` no character marks) reads the text marks: see [`crate::repair`].
//! - V7: a reference to a deleted object: an item's object style, a table's or cell's style →
//!   the family's first style; an item's layer, a guide's layer → the first layer; a page's
//!   parent, a spread's `basedOn`, a style's `basedOn`/`next` → none; a swatch `{ref, name}` →
//!   its name. An asset stays as it is (reported by `layout.missing-image`).
//! - V8: a duplicate item `key` is cleared on the item whose id sorts later.
//! - V9: an empty group is deleted.
//! - V10: a section starting at a deleted page is deleted.
//! - V12: an item's `overrides` naming an item that is not on a parent spread is cleared.
//! - V13: a hyperlink starting from a deleted item or going to a deleted page, and a data merge
//!   placeholder filling a deleted item, are deleted (one whose source text is gone: see
//!   [`crate::repair`]).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use kition_kit::kdm::{ObjectId, Tx};
use serde_json::Value;

use crate::fields::as_object;
use crate::ids::FAMILY_FIELDS;
use crate::refs::drop_gone_swatches;
use crate::{KdmResult, root_id};

/// Removes from `owner.field` the entries naming no live object (V1) and those `seen` already
/// holds (V3), adding the rest to `seen`.
fn clean(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, seen: &mut HashSet<ObjectId>) -> KdmResult<bool> {
    let items = tx.list_items(owner, field)?;
    let mut drop = Vec::new();
    for (index, object) in items.iter().enumerate() {
        if !tx.exists(object) || !seen.insert(object.clone()) {
            drop.push(index);
        }
    }
    for index in drop.iter().rev() {
        tx.list_remove(owner, field, *index)?;
    }
    Ok(!drop.is_empty())
}

/// `owners` sorted by id: the order V3 and V4 keep first entries in.
fn sorted(mut owners: Vec<ObjectId>) -> Vec<ObjectId> {
    owners.sort();
    owners
}

fn live_ref(tx: &Tx<'_>, v: &Value) -> Option<ObjectId> {
    as_object(v).filter(|o| tx.exists(o))
}

/// Batch writes must leave content discriminators consistent before merge repair runs.
pub(crate) fn check_batch(tx: &Tx<'_>) -> KdmResult<()> {
    let entries = tx.tree(&root_id(), "items")?;
    let mut parents = HashSet::new();
    for parent in crate::ids::tree_parents(&entries).into_iter().flatten() {
        if let Some(entry) = entries.get(parent) {
            parents.insert(entry.object.clone());
        }
    }
    let mut frames = HashSet::new();
    for story in tx.objects_of_type("Story") {
        for frame in tx.list_items(&story, "frames")? {
            if tx.exists(&frame) {
                if tx.get(&frame, "content")? != "text" {
                    return Err(crate::kdm_err("a story's live frame must have text content; use a content conversion command"));
                }
                frames.insert(frame);
            }
        }
    }
    for item in tx.objects_of_type("Item") {
        let kind = tx.get(&item, "content")?;
        if (kind == "group") != parents.contains(&item) {
            return Err(crate::kdm_err("group content must have tree children, and only a group can own children"));
        }
        if kind == "text" && !frames.contains(&item) {
            return Err(crate::kdm_err("text content must belong to a story"));
        }
    }
    Ok(())
}

pub(crate) fn validate(tx: &mut Tx<'_>, _changed: &BTreeSet<ObjectId>) -> KdmResult<bool> {
    let root = root_id();
    if !tx.exists(&root) {
        return Ok(false);
    }
    // One repair can expose another (a deleted item leaves its story frameless): run the rules
    // until nothing changes (a few rounds at most; each is deterministic).
    let mut wrote = false;
    for _ in 0..8 {
        if !round(tx, &root)? {
            break;
        }
        wrote = true;
    }
    Ok(wrote)
}

fn round(tx: &mut Tx<'_>, root: &ObjectId) -> KdmResult<bool> {
    let root = root.clone();
    let mut wrote = false;

    // V1, V3: the Layout's lists. Spreads and parent spreads are one kind, so are the five style
    // families.
    let mut spreads_seen = HashSet::new();
    wrote |= clean(tx, &root, "spreads", &mut spreads_seen)?;
    wrote |= clean(tx, &root, "parents", &mut spreads_seen)?;
    let mut styles_seen = HashSet::new();
    for field in FAMILY_FIELDS {
        wrote |= clean(tx, &root, field, &mut styles_seen)?;
    }
    // These read-only lookup keys must also follow direct batches and list transfers.
    for (family, field) in FAMILY_FIELDS.iter().enumerate() {
        for style in tx.list_items(&root, field)? {
            let name = tx.get(&style, "name")?;
            for (scope, key) in crate::ids::FAMILY_NAMES.iter().enumerate() {
                let value = if scope == family { name.clone() } else { Value::Null };
                if tx.get(&style, key)? != value {
                    tx.set(&style, key, value)?;
                    wrote = true;
                }
            }
        }
    }
    for field in ["layers", "stories", "swatches", "assets", "sections", "hyperlinks", "placeholders", "conditions", "textVariables", "xrefFormats"] {
        wrote |= clean(tx, &root, field, &mut HashSet::new())?;
    }
    let spreads = tx.list_items(&root, "spreads")?;
    let parents = tx.list_items(&root, "parents")?;
    let all_spreads = sorted(spreads.iter().chain(&parents).cloned().collect());
    for parent in &parents {
        let prefix = tx.get(parent, "prefix")?;
        let name = tx.get(parent, "name")?;
        let label = Value::String(format!("{}-{}", prefix.as_str().unwrap_or(""), name.as_str().unwrap_or("")));
        if tx.get(parent, "parentLabel")? != label {
            tx.set(parent, "parentLabel", label)?;
            wrote = true;
        }
    }
    let mut pages_seen = HashSet::new();
    let mut guides_seen = HashSet::new();
    for spread in &all_spreads {
        wrote |= clean(tx, spread, "pages", &mut pages_seen)?;
        wrote |= clean(tx, spread, "guides", &mut guides_seen)?;
    }
    for page in sorted(pages_seen.iter().cloned().collect()) {
        wrote |= clean(tx, &page, "overridden", &mut HashSet::new())?;
    }

    // Stories: V4 (frames), V1/V3 (inlines, tables), then tables' rows and columns.
    let stories = sorted(tx.list_items(&root, "stories")?);
    let mut frames_seen = HashSet::new();
    let mut inlines_seen = HashSet::new();
    let mut tables_seen = HashSet::new();
    for story in &stories {
        wrote |= clean(tx, story, "frames", &mut frames_seen)?;
        wrote |= clean(tx, story, "inlines", &mut inlines_seen)?;
        wrote |= clean(tx, story, "tables", &mut tables_seen)?;
    }
    let mut rows_seen = HashSet::new();
    let mut cols_seen = HashSet::new();
    for table in sorted(tables_seen.iter().cloned().collect()) {
        wrote |= clean(tx, &table, "rows", &mut rows_seen)?;
        wrote |= clean(tx, &table, "columns", &mut cols_seen)?;
        let rows: HashSet<String> = tx.list_items(&table, "rows")?.iter().map(|o| o.as_str().to_owned()).collect();
        let cols: HashSet<String> = tx.list_items(&table, "columns")?.iter().map(|o| o.as_str().to_owned()).collect();
        for (key, cell) in tx.map_entries(&table, "cells")? {
            let (row, col) = key.split_once('/').unwrap_or((key.as_str(), ""));
            let live_cell = live_ref(tx, &cell).is_some();
            if !rows.contains(row) || !cols.contains(col) || !live_cell {
                if let Some(cell) = live_ref(tx, &cell)
                    && (!rows.contains(row) || !cols.contains(col))
                {
                    tx.delete(&cell)?;
                }
                tx.map_delete(&table, "cells", &key)?;
                wrote = true;
            }
        }
    }

    // V5: stories without a live frame that no setting names.
    let named_stories = named_stories(tx, &root)?;
    for story in &stories {
        if !tx.exists(story) || named_stories.contains(story.as_str()) {
            continue;
        }
        if tx.list_items(story, "frames")?.is_empty() {
            delete_story(tx, story)?;
            wrote = true;
        }
    }
    wrote |= clean(tx, &root, "stories", &mut HashSet::new())?;

    // The items tree: V2, V9, and items it no longer reaches.
    let live_spreads: HashSet<ObjectId> = all_spreads.iter().filter(|s| tx.exists(s)).cloned().collect();
    let parent_spreads: HashSet<ObjectId> = parents.iter().filter(|s| tx.exists(s)).cloned().collect();
    let mut anchored: HashSet<ObjectId> = HashSet::new();
    for inline in tx.objects_of_type("Inline") {
        if let Some(item) = live_ref(tx, &tx.get(&inline, "item")?) {
            anchored.insert(item);
        }
    }
    // Nodes naming a deleted object (and what is under them) are not part of the document.
    let entries: Vec<_> = {
        let all = tx.tree(&root, "items")?;
        let parents = crate::ids::tree_parents(&all);
        let mut alive = vec![false; all.len()];
        for (i, e) in all.iter().enumerate() {
            alive[i] = tx.exists(&e.object) && parents[i].is_none_or(|p| alive[p]);
        }
        all.into_iter().zip(alive).filter(|(_, a)| *a).map(|(e, _)| e).collect()
    };
    let mut children: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    let mut parent_of: HashMap<ObjectId, ObjectId> = HashMap::new();
    let mut roots = Vec::new();
    for (e, parent) in entries.iter().zip(crate::ids::tree_parents(&entries)) {
        match parent.map(|p| &entries[p].object) {
            Some(parent) => {
                children.entry(parent.clone()).or_default().push(e.object.clone());
                parent_of.insert(e.object.clone(), parent.clone());
            }
            None => roots.push(e.object.clone()),
        }
    }
    let reached: HashSet<ObjectId> = entries.iter().map(|e| e.object.clone()).collect();
    let mut doomed: BTreeSet<ObjectId> = BTreeSet::new();
    for item in &roots {
        let spread = live_ref(tx, &tx.get(item, "spread")?).filter(|s| live_spreads.contains(s));
        if spread.is_none() && !anchored.contains(item) {
            subtree(item, &children, &mut doomed);
        }
    }
    for item in tx.objects_of_type("Item") {
        if !reached.contains(&item) {
            doomed.insert(item);
        }
    }
    // V9: empty groups (after the deletes above), innermost first.
    let mut changed = true;
    while changed {
        changed = false;
        for item in tx.objects_of_type("Item") {
            if doomed.contains(&item) || tx.get(&item, "content")? != Value::String("group".into()) {
                continue;
            }
            let live_children = children.get(&item).is_some_and(|c| c.iter().any(|c| !doomed.contains(c) && tx.exists(c)));
            if !live_children {
                doomed.insert(item);
                changed = true;
            }
        }
    }
    for item in &doomed {
        if tx.exists(item) {
            tx.delete(item)?;
            wrote = true;
        }
    }

    // V7, V8, V12 on items.
    let first_layer = tx.list_items(&root, "layers")?.first().cloned();
    let first_style: Vec<Option<ObjectId>> = FAMILY_FIELDS.iter().map(|f| tx.list_items(&root, f).ok().and_then(|l| l.first().cloned())).collect();
    let mut keys: BTreeMap<String, Vec<ObjectId>> = BTreeMap::new();
    let top_of = |item: &ObjectId| -> ObjectId {
        let mut at = item.clone();
        let mut hops = 0;
        while let Some(parent) = parent_of.get(&at) {
            at = parent.clone();
            hops += 1;
            if hops > crate::MAX_DEPTH {
                break;
            }
        }
        at
    };
    let mut fixes: Vec<(ObjectId, &'static str, Value)> = Vec::new();
    for item in tx.objects_of_type("Item") {
        let layer = tx.get(&item, "layer")?;
        if !layer.is_null() && live_ref(tx, &layer).is_none() {
            fixes.push((item.clone(), "layer", first_layer.as_ref().map(|l| Value::String(l.as_str().to_owned())).unwrap_or(Value::Null)));
        }
        let style = tx.get(&item, "objectStyle")?;
        if !style.is_null() && live_ref(tx, &style).is_none() {
            fixes.push((item.clone(), "objectStyle", first_style[2].as_ref().map(|s| Value::String(s.as_str().to_owned())).unwrap_or(Value::Null)));
        }
        let overrides = tx.get(&item, "overrides")?;
        if !overrides.is_null() {
            let on_parent = live_ref(tx, &overrides)
                .map(|o| top_of(&o))
                .and_then(|top| live_ref(tx, &tx.get(&top, "spread").unwrap_or(Value::Null)))
                .is_some_and(|s| parent_spreads.contains(&s));
            if !on_parent {
                fixes.push((item.clone(), "overrides", Value::Null));
            }
        }
        if let Value::String(key) = tx.get(&item, "key")? {
            keys.entry(key).or_default().push(item.clone());
        }
        for field in ["fill", "stroke", "effects", "textOptions", "graphic", "props"] {
            let mut v = tx.get(&item, field)?;
            if drop_gone_swatches(&mut v, &|o| tx.exists(&ObjectId::from_string(o))) {
                fixes.push((item.clone(), field, v));
            }
        }
    }
    for (_, mut holders) in keys {
        holders.sort();
        for later in holders.into_iter().skip(1) {
            fixes.push((later, "key", Value::Null));
        }
    }
    // V7 on pages, spreads, guides, styles, tables, cells, swatches, inlines.
    for page in tx.objects_of_type("Page") {
        let parent = tx.get(&page, "parent")?;
        if !parent.is_null() && live_ref(tx, &parent).is_none() {
            fixes.push((page, "parent", Value::Null));
        }
    }
    for spread in tx.objects_of_type("Spread") {
        let based_on = tx.get(&spread, "basedOn")?;
        if !based_on.is_null() && live_ref(tx, &based_on).is_none() {
            fixes.push((spread, "basedOn", Value::Null));
        }
    }
    for guide in tx.objects_of_type("Guide") {
        let layer = tx.get(&guide, "layer")?;
        if !layer.is_null() && live_ref(tx, &layer).is_none() {
            fixes.push((guide, "layer", first_layer.as_ref().map(|l| Value::String(l.as_str().to_owned())).unwrap_or(Value::Null)));
        }
    }
    for style in tx.objects_of_type("Style") {
        for field in ["basedOn", "next"] {
            let v = tx.get(&style, field)?;
            if !v.is_null() && live_ref(tx, &v).is_none() {
                fixes.push((style.clone(), field, Value::Null));
            }
        }
    }
    for (ty, family) in [("Table", 3usize), ("Cell", 4usize)] {
        for object in tx.objects_of_type(ty) {
            let v = tx.get(&object, "style")?;
            if !v.is_null() && live_ref(tx, &v).is_none() {
                fixes.push((object, "style", first_style[family].as_ref().map(|s| Value::String(s.as_str().to_owned())).unwrap_or(Value::Null)));
            }
        }
    }
    for (ty, field) in [("Swatch", "value"), ("Inline", "data"), ("Cell", "props"), ("Table", "options")] {
        for object in tx.objects_of_type(ty) {
            let mut v = tx.get(&object, field)?;
            if drop_gone_swatches(&mut v, &|o| tx.exists(&ObjectId::from_string(o))) {
                fixes.push((object, field, v));
            }
        }
    }
    for (object, field, v) in fixes {
        tx.set(&object, field, v)?;
        wrote = true;
    }
    for style in tx.objects_of_type("Style") {
        for (key, mut v) in tx.map_entries(&style, "attrs")? {
            if drop_gone_swatches(&mut v, &|o| tx.exists(&ObjectId::from_string(o))) {
                tx.map_set(&style, "attrs", &key, v)?;
                wrote = true;
            }
        }
    }

    // V10: sections starting at a deleted page.
    for section in tx.list_items(&root, "sections")? {
        if live_ref(tx, &tx.get(&section, "start")?).is_none() {
            tx.delete(&section)?;
            wrote = true;
        }
    }
    wrote |= clean(tx, &root, "sections", &mut HashSet::new())?;

    // V13: hyperlinks and placeholders whose item or page is gone.
    for (field, refs) in [("hyperlinks", &["item", "page"][..]), ("placeholders", &["item"][..])] {
        for object in tx.list_items(&root, field)? {
            let mut gone = false;
            for key in refs {
                let v = tx.get(&object, key)?;
                gone |= !v.is_null() && live_ref(tx, &v).is_none();
            }
            if gone && tx.exists(&object) {
                tx.delete(&object)?;
                wrote = true;
            }
        }
        wrote |= clean(tx, &root, field, &mut HashSet::new())?;
    }
    Ok(wrote)
}

/// The stories settings and the setup name (TOC, index, endnotes, the primary story).
fn named_stories(tx: &Tx<'_>, root: &ObjectId) -> KdmResult<HashSet<String>> {
    let mut out = HashSet::new();
    if let Some(s) = tx.get(root, "setup")?.get("primaryStory").and_then(Value::as_str) {
        out.insert(s.to_owned());
    }
    if let Some(s) = tx.map_get(root, "settings", "endnoteStory")?.as_str() {
        out.insert(s.to_owned());
    }
    for key in ["toc", "index"] {
        if let Some(s) = tx.map_get(root, "settings", key)?.get("story").and_then(Value::as_str) {
            out.insert(s.to_owned());
        }
    }
    Ok(out)
}

fn subtree(item: &ObjectId, children: &HashMap<ObjectId, Vec<ObjectId>>, out: &mut BTreeSet<ObjectId>) {
    let mut stack = vec![item.clone()];
    while let Some(at) = stack.pop() {
        if out.insert(at.clone())
            && let Some(kids) = children.get(&at)
        {
            stack.extend(kids.iter().cloned());
        }
    }
}

/// Deletes a story and what it owns: its inline objects, tables, rows, columns and cells.
pub(crate) fn delete_story(tx: &mut Tx<'_>, story: &ObjectId) -> KdmResult<()> {
    for inline in tx.list_items(story, "inlines")? {
        if tx.exists(&inline) {
            tx.delete(&inline)?;
        }
    }
    for table in tx.list_items(story, "tables")? {
        delete_table(tx, &table)?;
    }
    if tx.exists(story) {
        tx.delete(story)?;
    }
    Ok(())
}

/// Deletes a table with its rows, columns and cells.
pub(crate) fn delete_table(tx: &mut Tx<'_>, table: &ObjectId) -> KdmResult<()> {
    if !tx.exists(table) {
        return Ok(());
    }
    for (_, cell) in tx.map_entries(table, "cells")? {
        if let Some(cell) = live_ref(tx, &cell) {
            tx.delete(&cell)?;
        }
    }
    for field in ["rows", "columns"] {
        for line in tx.list_items(table, field)? {
            if tx.exists(&line) {
                tx.delete(&line)?;
            }
        }
    }
    tx.delete(table)
}
