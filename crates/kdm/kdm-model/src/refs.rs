//! References stored as data (design note sections 2.2 and 2.5).
//!
//! - **Swatches** are named in the engine (fills, strokes, rules, character colours, tints). In a
//!   stored JSON value, a string under a colour key ([`is_swatch_key`]) that names a swatch is
//!   written as `{"ref": <swatch object id>, "name": <name>}`: it follows a rename by another
//!   person, and reads back as the name it had if the swatch is gone.
//! - **Local ids** inside settings (articles, the TOC, index and endnote stories, the pages PDF
//!   bookmarks go to) are written as the object ids they name; a reference to nothing is not
//!   written (a bookmark whose page is gone is dropped with the bookmarks under it).

use std::collections::HashMap;

use kition_kit::kdm::ObjectId;
use serde_json::{Map, Value, json};

use crate::MAX_DEPTH;

/// Keys whose string values name a swatch.
pub(crate) fn is_swatch_key(key: &str) -> bool {
    matches!(key, "fill" | "stroke" | "color" | "swatch" | "gap" | "base") || key.ends_with("Color")
}

/// Rewrites swatch names under colour keys inside `v` as `{ref, name}` (see the module docs).
pub(crate) fn swatches_out(v: &mut Value, swatches: &HashMap<String, ObjectId>) {
    if swatches.is_empty() {
        return;
    }
    out_rec(v, swatches, 0);
}

fn out_rec(v: &mut Value, swatches: &HashMap<String, ObjectId>, depth: usize) {
    if depth > MAX_DEPTH * 4 {
        return;
    }
    match v {
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if is_swatch_key(key)
                    && let Value::String(name) = item
                {
                    if let Some(object) = swatches.get(name.as_str()) {
                        *item = json!({ "ref": object.as_str(), "name": name });
                    }
                } else {
                    out_rec(item, swatches, depth + 1);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| out_rec(item, swatches, depth + 1)),
        _ => {}
    }
}

/// One swatch reference value `{ref, name}`, by key: `Some(name)` for a swatch named `name`.
pub(crate) fn swatch_ref(v: &Value) -> Option<(&str, &str)> {
    let map = v.as_object()?;
    if map.len() != 2 {
        return None;
    }
    Some((map.get("ref")?.as_str()?, map.get("name")?.as_str()?))
}

/// Rewrites every `{ref, name}` inside `v` back to the swatch's current name (`names`), or the
/// name it had when the swatch is gone.
pub(crate) fn swatches_in(v: &mut Value, names: &HashMap<ObjectId, String>) {
    in_rec(v, names, 0);
}

fn in_rec(v: &mut Value, names: &HashMap<ObjectId, String>, depth: usize) {
    if depth > MAX_DEPTH * 4 {
        return;
    }
    if let Some((object, name)) = swatch_ref(v) {
        let name = names.get(&ObjectId::from_string(object)).cloned().unwrap_or_else(|| name.to_owned());
        *v = Value::String(name);
        return;
    }
    match v {
        Value::Object(map) => map.values_mut().for_each(|item| in_rec(item, names, depth + 1)),
        Value::Array(items) => items.iter_mut().for_each(|item| in_rec(item, names, depth + 1)),
        _ => {}
    }
}

/// Clears the `ref` of every `{ref, name}` inside `v` whose swatch `live` says is gone, keeping
/// the name (validation rule V7). Answers whether it changed anything.
pub(crate) fn drop_gone_swatches(v: &mut Value, live: &dyn Fn(&str) -> bool) -> bool {
    fn rec(v: &mut Value, live: &dyn Fn(&str) -> bool, depth: usize) -> bool {
        if depth > MAX_DEPTH * 4 {
            return false;
        }
        if let Some((object, name)) = swatch_ref(v) {
            if live(object) {
                return false;
            }
            *v = Value::String(name.to_owned());
            return true;
        }
        match v {
            Value::Object(map) => map.values_mut().fold(false, |any, item| rec(item, live, depth + 1) | any),
            Value::Array(items) => items.iter_mut().fold(false, |any, item| rec(item, live, depth + 1) | any),
            _ => false,
        }
    }
    rec(v, live, 0)
}

/// How local ids translate: item and story ids, and absolute page indexes, to object ids (writing)
/// or back (reading); `None` drops the reference.
pub(crate) struct Mapper<'a> {
    pub item: &'a dyn Fn(&Value) -> Option<Value>,
    pub story: &'a dyn Fn(&Value) -> Option<Value>,
    pub page: &'a dyn Fn(&Value) -> Option<Value>,
}

/// The settings keys that hold local ids (see [`map_setting`]).
pub(crate) const SETTINGS_WITH_IDS: &[&str] = &["articles", "bookmarks", "toc", "index", "endnoteStory"];

fn map_field(obj: &mut Map<String, Value>, key: &str, f: &dyn Fn(&Value) -> Option<Value>) {
    if let Some(v) = obj.get(key) {
        match f(v) {
            Some(new) => {
                obj.insert(key.to_owned(), new);
            }
            None => {
                obj.remove(key);
            }
        }
    }
}

/// The bookmarks of `list` (and of their `children`) with their `page` mapped; a bookmark whose
/// page is gone is dropped with the bookmarks under it.
fn map_bookmarks(list: &mut Vec<Value>, m: &Mapper<'_>, depth: usize) {
    if depth > MAX_DEPTH {
        list.clear();
        return;
    }
    list.retain_mut(|bookmark| {
        let Some(obj) = bookmark.as_object_mut() else { return false };
        map_field(obj, "page", m.page);
        if let Some(children) = obj.get_mut("children").and_then(Value::as_array_mut) {
            map_bookmarks(children, m, depth + 1);
        }
        obj.contains_key("page")
    });
}

/// Translates the local ids inside settings entry `key` (the engine's serialized form) by `m`.
/// An entry that ends up naming nothing becomes `null`.
pub(crate) fn map_setting(key: &str, v: &mut Value, m: &Mapper<'_>) {
    match key {
        "articles" => {
            if let Some(list) = v.as_array_mut() {
                for article in list {
                    if let Some(items) = article.get_mut("items").and_then(Value::as_array_mut) {
                        let mapped: Vec<Value> = items.iter().filter_map(|i| (m.item)(i)).collect();
                        *items = mapped;
                    }
                }
            }
        }
        "bookmarks" => {
            if let Some(list) = v.as_array_mut() {
                map_bookmarks(list, m, 0);
            }
        }
        "toc" | "index" => {
            if let Some(obj) = v.as_object_mut() {
                map_field(obj, "story", m.story);
            }
        }
        "endnoteStory" => {
            *v = (m.story)(v).unwrap_or(Value::Null);
        }
        _ => {}
    }
}

/// The largest integer under an `id` key anywhere in `v` (ids the bridge keeps as data, such as
/// data merge source ids), so the view's id counter stays above them.
pub(crate) fn max_id(v: &Value) -> u64 {
    fn rec(v: &Value, depth: usize) -> u64 {
        if depth > MAX_DEPTH * 4 {
            return 0;
        }
        match v {
            Value::Object(map) => {
                map.iter().map(|(k, item)| if k == "id" { item.as_u64().unwrap_or(0) } else { rec(item, depth + 1) }).max().unwrap_or(0)
            }
            Value::Array(items) => items.iter().map(|item| rec(item, depth + 1)).max().unwrap_or(0),
            _ => 0,
        }
    }
    rec(v, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swatch_names_follow_their_object() {
        let red = ObjectId::from_string("red");
        let mut v = json!({ "fill": { "color": "Red", "tint": 1 }, "underlineColor": "Red", "name": "Red", "stops": [{ "color": "Paper" }] });
        swatches_out(&mut v, &HashMap::from([("Red".to_owned(), red.clone())]));
        assert_eq!(v["fill"]["color"], json!({ "ref": "red", "name": "Red" }));
        assert_eq!(v["underlineColor"], json!({ "ref": "red", "name": "Red" }));
        assert_eq!(v["name"], json!("Red"), "not a colour key");
        assert_eq!(v["stops"][0]["color"], json!("Paper"), "names no swatch");
        let mut renamed = v.clone();
        swatches_in(&mut renamed, &HashMap::from([(red.clone(), "Crimson".to_owned())]));
        assert_eq!(renamed["fill"]["color"], json!("Crimson"));
        let mut gone = v.clone();
        swatches_in(&mut gone, &HashMap::new());
        assert_eq!(gone["underlineColor"], json!("Red"), "the name it had");
        assert!(drop_gone_swatches(&mut v, &|_| false));
        assert_eq!(v["fill"]["color"], json!("Red"));
    }
}
