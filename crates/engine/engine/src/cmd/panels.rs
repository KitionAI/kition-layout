//! What the Layers and Links panels do to the document (plan 2026-11-25, stage 6): an object
//! moved to a place in the stacking order of a layer (dragging its row), a layer duplicated with
//! its objects, and an embedded graphic kept in a file again.

use std::collections::HashMap;
use std::sync::Arc;

use kition_layout_doc::{AssetId, Item, ItemId, Layer, LayerId, SpreadRef};
use kition_layout_geom::Vec2;
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc, has_selection, str_param, targets};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "arrange.reorder",
            "Move in Stacking Order",
            [],
            None,
            "{ids?, layer?: layer id (default: the first object's layer), index: n (0 = the frontmost of that layer's objects on the spread)} — the objects (top-level, on one spread) go to that place among the layer's objects, in their own order, onto that layer",
            has_selection,
            reorder
        ),
        cmd!(
            "layer.duplicate",
            "Duplicate Layer",
            [],
            None,
            "{id} → {id: the new layer} — a copy of the layer (its name with \"copy\", its options) just above it, holding a copy of each of its objects on every spread and parent",
            has_doc,
            duplicate_layer
        ),
        cmd!(
            "linkedFile.unembed",
            "Unembed Link",
            ["Window", "Links"],
            None,
            "{asset, path} — keep the embedded graphic in the file at `path` too and link to it: the file is written there (an editor in a host writes it through the host first; a hosted session checks the file holds the graphic)",
            has_doc,
            unembed
        ),
    ]
}

fn reorder(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "arrange.reorder";
    let ids = targets(s, p)?;
    let index = p.get("index").and_then(Value::as_u64).ok_or_else(|| bad(ID, "`index` is a whole number (0 = frontmost)"))?;
    let index = usize::try_from(index).unwrap_or(usize::MAX);
    let layer = p.get("layer").map(|v| v.as_u64().map(LayerId).ok_or_else(|| bad(ID, "`layer` is a layer id"))).transpose()?;
    s.edit(|d, _| {
        let first = *ids.first().ok_or_else(|| bad(ID, "nothing to move"))?;
        let spread = d.find(first).ok_or(kition_layout_doc::DocError::NoItem(first))?.spread;
        for id in &ids {
            let loc = d.find(*id).ok_or(kition_layout_doc::DocError::NoItem(*id))?;
            if loc.path.len() != 1 || loc.spread != spread {
                return Err(bad(ID, "the objects must be top-level objects on one spread"));
            }
        }
        let target = match layer {
            Some(l) => l,
            None => d.item(first).map(|it| it.layer).ok_or(kition_layout_doc::DocError::NoItem(first))?,
        };
        if d.layer(target).is_none() {
            return Err(bad(ID, format!("no layer {}", target.0)));
        }
        let sp = d.spread_mut(spread).ok_or_else(|| bad(ID, "no such spread"))?;
        // Taken out back to front, so they keep their order among themselves.
        let mut moving: Vec<Arc<Item>> = Vec::with_capacity(ids.len());
        let mut kept = Vec::with_capacity(sp.items.len());
        for it in sp.items.drain(..) {
            if ids.contains(&it.id) { moving.push(it) } else { kept.push(it) }
        }
        sp.items = kept;
        for it in &mut moving {
            Arc::make_mut(it).layer = target;
        }
        // The layer's objects on the spread, back to front (positions in `items`).
        let on_layer: Vec<usize> = sp.items.iter().enumerate().filter(|(_, it)| it.layer == target).map(|(i, _)| i).collect();
        let n = on_layer.len();
        let at = if n == 0 {
            sp.items.len()
        } else if index >= n {
            // Behind them all.
            on_layer.first().copied().unwrap_or(0)
        } else {
            // In front of the object now `index` from the front.
            on_layer.get(n - 1 - index).map_or(sp.items.len(), |i| i + 1)
        };
        let count = moving.len();
        let at = at.min(sp.items.len());
        sp.items.splice(at..at, moving);
        Ok(json!({"moved": count, "layer": target.0}))
    })
}

fn duplicate_layer(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "layer.duplicate";
    let id = p.get("id").and_then(Value::as_u64).map(LayerId).ok_or_else(|| bad(ID, "missing `id` (a layer id)"))?;
    s.edit(|d, _| {
        let at = d.layers.iter().position(|l| l.id == id).ok_or_else(|| bad(ID, format!("no layer {}", id.0)))?;
        let source = d.layers.get(at).cloned().ok_or_else(|| bad(ID, "no such layer"))?;
        let new_id = LayerId(d.alloc());
        let name = kition_layout_doc::Styles::unique_name(|n| d.layers.iter().any(|l| l.name == n), &format!("{} copy", source.name));
        d.layers.insert(at, Layer { id: new_id, name, ..source });
        // Every spread's and parent's objects on the layer, copied in their order (threads between
        // the copies kept), onto the new layer.
        let src = d.clone();
        let refs: Vec<SpreadRef> = src.spread_refs().collect();
        let mut copied = 0;
        for r in refs {
            let Some(sp) = src.spread(r) else { continue };
            let ids: Vec<ItemId> = sp.items.iter().filter(|it| it.layer == id).map(|it| it.id).collect();
            if ids.is_empty() {
                continue;
            }
            let new = super::object::duplicate_from(d, &src, &ids, r, Vec2::ZERO)?;
            let made: HashMap<ItemId, ()> = new.iter().map(|i| (*i, ())).collect();
            if let Some(spread) = d.spread_mut(r) {
                for it in &mut spread.items {
                    if made.contains_key(&it.id) {
                        Arc::make_mut(it).layer = new_id;
                    }
                }
            }
            copied += new.len();
        }
        Ok(json!({"id": new_id.0, "objects": copied}))
    })
}

fn unembed(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "linkedFile.unembed";
    let aid = p.get("asset").and_then(Value::as_u64).map(AssetId).ok_or_else(|| bad(ID, "missing asset"))?;
    let path = str_param(p, "path").filter(|path| !path.trim().is_empty()).ok_or_else(|| bad(ID, "missing path"))?.to_string();
    let data = {
        let a = s.doc()?.doc.assets.get(&aid).cloned().ok_or_else(|| bad(ID, "no such graphic"))?;
        if a.link.is_some() {
            return Err(bad(ID, "the graphic is linked already"));
        }
        a.data.clone()
    };
    keep_in_file(&path, &data)?;
    let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
    s.edit(|d, _| {
        let a = d.assets.get_mut(&aid).ok_or_else(|| bad(ID, "no such graphic"))?;
        let a = Arc::make_mut(a);
        a.link = Some(path.clone());
        a.name = name.clone();
        Ok(json!({"asset": aid.0, "path": path, "name": name}))
    })
}

/// The file at `path` holds `data`: written there outside a host, read back to check in a hosted
/// session (its host wrote it).
#[cfg(not(target_arch = "wasm32"))]
fn keep_in_file(path: &str, data: &[u8]) -> Result<()> {
    if crate::disk::hosted() {
        let there = crate::disk::read_lent(path).map_err(|why| bad("linkedFile.unembed", format!("write the file through the host first: {why}")))?;
        if there != data {
            return Err(bad("linkedFile.unembed", format!("{path} holds another file")));
        }
        return Ok(());
    }
    crate::disk::write(path, data).map_err(|e| bad("linkedFile.unembed", format!("{path}: {e}")))
}

/// On the web the editor writes the file through its host before it links the graphic there.
#[cfg(target_arch = "wasm32")]
fn keep_in_file(_path: &str, _data: &[u8]) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s
    }

    fn order(s: &Session) -> Vec<(u64, u64)> {
        s.doc().unwrap().doc.spreads[0].items.iter().map(|it| (it.id.0, it.layer.0)).collect()
    }

    #[test]
    fn an_object_moves_to_a_place_among_its_layers_objects() {
        let mut s = session();
        let ids: Vec<u64> = (0..3)
            .map(|i| {
                s.execute("frame.add", &json!({"rect": [10.0 + 20.0 * i as f64, 10, 30.0 + 20.0 * i as f64, 30]})).unwrap()["id"].as_u64().unwrap()
            })
            .collect();
        let (a, b, c) = (ids[0], ids[1], ids[2]);
        // Back to front: a, b, c. The backmost to the front.
        s.execute("arrange.reorder", &json!({"ids": [a], "index": 0})).unwrap();
        assert_eq!(order(&s).iter().map(|x| x.0).collect::<Vec<_>>(), vec![b, c, a]);
        // Between c and b: one from the front.
        s.execute("arrange.reorder", &json!({"ids": [a], "index": 1})).unwrap();
        assert_eq!(order(&s).iter().map(|x| x.0).collect::<Vec<_>>(), vec![b, a, c]);
        // Past the end: behind them all.
        s.execute("arrange.reorder", &json!({"ids": [c], "index": 99})).unwrap();
        assert_eq!(order(&s).iter().map(|x| x.0).collect::<Vec<_>>(), vec![c, b, a]);
        // To another layer.
        let layer = s.execute("layer.new", &json!({"name": "Top"})).unwrap()["id"].as_u64().unwrap();
        s.execute("arrange.reorder", &json!({"ids": [b], "layer": layer, "index": 0})).unwrap();
        assert!(order(&s).contains(&(b, layer)));
        // One undo step; nonsense refused without a panic.
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(!order(&s).contains(&(b, layer)));
        assert!(s.execute("arrange.reorder", &json!({"ids": [b], "index": -1})).is_err());
        assert!(s.execute("arrange.reorder", &json!({"ids": [b], "layer": 99999, "index": 0})).is_err());
        assert!(s.execute("arrange.reorder", &json!({"ids": [123456], "index": 0})).is_err());
    }

    #[test]
    fn a_layer_duplicates_with_its_objects() {
        let mut s = session();
        let layer = s.doc().unwrap().doc.layers[0].id.0;
        s.execute("frame.add", &json!({"rect": [10, 10, 100, 100], "content": "text", "text": "Hello"})).unwrap();
        s.execute("frame.add", &json!({"rect": [110, 10, 200, 100]})).unwrap();
        let r = s.execute("layer.duplicate", &json!({"id": layer})).unwrap();
        let copy = r["id"].as_u64().unwrap();
        let d = &s.doc().unwrap().doc;
        assert_eq!(d.layers.len(), 2);
        assert_eq!(d.layers[0].id.0, copy, "just above the layer");
        assert!(d.layers[0].name.ends_with("copy"), "{}", d.layers[0].name);
        let on_copy: Vec<_> = d.spreads[0].items.iter().filter(|it| it.layer.0 == copy).collect();
        assert_eq!(on_copy.len(), 2);
        assert_eq!(d.stories.len(), 2, "the text frame's copy has its own story");
        assert!(s.execute("layer.duplicate", &json!({"id": 424242})).is_err());
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().doc.layers.len(), 1, "one undo step");
    }

    #[test]
    fn an_embedded_graphic_is_kept_in_a_file_and_linked() {
        let mut s = session();
        let png = kition_layout_render::Rendered { width: 2, height: 1, pixels: vec![255, 0, 0, 255, 0, 255, 0, 255] }.to_png();
        let placed = s.execute("file.place", &json!({"base64": super::super::base64_encode(&png), "name": "dot.png"})).unwrap();
        let asset = placed["asset"].as_u64().unwrap();
        let dir = std::env::temp_dir().join(format!("kition-unembed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dot.png");
        let path_text = path.to_string_lossy().to_string();
        s.execute("linkedFile.unembed", &json!({"asset": asset, "path": path_text})).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), png);
        let a = s.doc().unwrap().doc.assets.get(&AssetId(asset)).cloned().unwrap();
        assert_eq!(a.link.as_deref(), Some(path_text.as_str()));
        assert!(s.execute("linkedFile.unembed", &json!({"asset": asset, "path": path_text})).is_err(), "linked already");
        assert!(s.execute("linkedFile.unembed", &json!({"asset": 999999, "path": path_text})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
