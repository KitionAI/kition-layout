//! The Kition platform kit suite's adapter for Kition Layout (`kition_convergence::KitAdapter`):
//! drives `LayoutDocument` the way a host does (contract 1 `execute`, the contract 2 replica
//! members), shared by the `coediting` and `replica_integrity` test binaries.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_convergence::{Blobs, KitAdapter, Result, Rng, Script};
use kition_kit::KitDocument;
use kition_layout_kdm::kition_kit::kdm::{ActorId, Replica};
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

pub const NAME: &str = "Layout.klayout";
pub const WIDTH: u32 = 612;
pub const HEIGHT: u32 = 792;

/// The swatches a new layout has, for fills that differ between edits.
const SWATCHES: [&str; 6] =
    ["C=100 M=0 Y=0 K=0", "C=0 M=100 Y=0 K=0", "C=0 M=0 Y=100 K=0", "C=0 M=100 Y=100 K=0", "C=100 M=0 Y=100 K=0", "C=100 M=100 Y=0 K=0"];

pub struct LayoutKit;

/// `(64-char hex SHA-256, u64 LE length, bytes) * N`, as `blobs`/`add_blobs` move them.
pub fn decode_blobs(bytes: &[u8]) -> Result<Blobs> {
    let mut out = Blobs::new();
    let mut at = 0;
    while at < bytes.len() {
        let hash = std::str::from_utf8(bytes.get(at..at + 64).ok_or("truncated hash")?).map_err(|e| e.to_string())?.to_owned();
        let len = u64::from_le_bytes(bytes.get(at + 64..at + 72).ok_or("truncated length")?.try_into().map_err(|_| "length")?) as usize;
        let data = bytes.get(at + 72..at + 72 + len).ok_or("truncated data")?;
        out.insert(hash, data.to_vec());
        at += 72 + len;
    }
    Ok(out)
}

/// Reverses [`decode_blobs`], hashes in order.
pub fn encode_blobs(blobs: &Blobs) -> Vec<u8> {
    let mut out = Vec::new();
    for (hash, data) in blobs {
        out.extend_from_slice(hash.as_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// The first path at which `a` and `b` differ, with both values there.
fn first_difference(a: &Value, b: &Value) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter().find_map(|key| {
                let (l, r) = (x.get(key).unwrap_or(&Value::Null), y.get(key).unwrap_or(&Value::Null));
                first_difference(l, r).map(|inner| format!("/{key}{inner}"))
            })
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            x.iter().zip(y).enumerate().find_map(|(index, (l, r))| first_difference(l, r).map(|inner| format!("/{index}{inner}")))
        }
        _ if a == b => None,
        _ => Some(format!(": {a} != {b}")),
    }
}

pub fn exec(doc: &mut LayoutDocument, command: &str, params: Value) -> Result<Value> {
    doc.execute(command, params).map_err(|error| format!("{command}: {error}"))
}

/// Runs a command; a refusal because of the layout's state is no edit (`Ok(false)`).
fn attempt(doc: &mut LayoutDocument, command: &str, params: Value) -> Result<bool> {
    Ok(doc.execute(command, params).is_ok())
}

/// Every object of every spread as `file.inspect` reports it, depth first, back to front.
pub fn objects(doc: &LayoutDocument) -> Vec<Value> {
    fn walk(item: &Value, out: &mut Vec<Value>) {
        out.push(item.clone());
        for child in item["children"].as_array().into_iter().flatten() {
            walk(child, out);
        }
    }
    let inspected = doc.inspect().unwrap_or(Value::Null);
    let mut out = Vec::new();
    for spread in inspected["spreads"].as_array().into_iter().flatten() {
        for item in spread["items"].as_array().into_iter().flatten() {
            walk(item, &mut out);
        }
    }
    out
}

fn is_group(item: &Value) -> bool {
    item.get("children").is_some()
}

/// Objects that are not groups, in document order: the same on every replica that has the same
/// state, since the view follows the replica.
pub fn leaves(doc: &LayoutDocument) -> Vec<u64> {
    objects(doc).iter().filter(|item| !is_group(item) && !item["locked"].as_bool().unwrap_or(false)).filter_map(|item| item["id"].as_u64()).collect()
}

/// Objects at the top of their spread (not inside a group).
fn tops(doc: &LayoutDocument) -> Vec<u64> {
    let inspected = doc.inspect().unwrap_or(Value::Null);
    inspected["spreads"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|spread| spread["items"].as_array().into_iter().flatten())
        .filter_map(|item| item["id"].as_u64())
        .collect()
}

fn groups(doc: &LayoutDocument) -> Vec<u64> {
    objects(doc).iter().filter(|item| is_group(item)).filter_map(|item| item["id"].as_u64()).collect()
}

pub fn pages(doc: &LayoutDocument) -> u64 {
    doc.inspect().map(|inspected| inspected["pageCount"].as_u64().unwrap_or(0)).unwrap_or(0)
}

/// The probe: the first object that is not a text frame, so text and geometry scenarios touch
/// different objects.
fn probe(doc: &LayoutDocument) -> Result<u64> {
    objects(doc)
        .iter()
        .find(|item| !is_group(item) && item.get("story").is_none())
        .and_then(|item| item["id"].as_u64())
        .ok_or_else(|| "the layout has no object".to_owned())
}

/// The first story (the headline's), whose text the text scenarios type into.
pub fn story(doc: &LayoutDocument) -> Option<u64> {
    let inspected = doc.inspect().ok()?;
    inspected["stories"].as_array()?.first()?["id"].as_u64()
}

/// The first story's text, paragraphs joined by `\n`.
pub fn story_text(doc: &mut LayoutDocument) -> Result<Option<String>> {
    let Some(id) = story(doc) else { return Ok(None) };
    let read = exec(doc, "story.get", json!({ "story": id }))?;
    Ok(read["text"].as_str().map(|text| text.replace('\r', "\n")))
}

/// The byte offset (`story.replaceRange` counts UTF-8 bytes) of character `at` of the story's
/// text.
fn position(text: &str, at: usize) -> usize {
    text.chars().take(at).map(char::len_utf8).sum()
}

fn replace(doc: &mut LayoutDocument, at: usize, len: usize, text: &str) -> Result<bool> {
    let Some(id) = story(doc) else { return Ok(false) };
    let current = story_text(doc)?.unwrap_or_default();
    let (start, end) = (position(&current, at), position(&current, at + len));
    exec(doc, "story.replaceRange", json!({ "story": id, "start": start, "end": end, "text": text }))?;
    Ok(true)
}

/// A small PNG whose pixel derives from `seed`.
fn png(seed: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let [a, b, c, _] = seed.to_le_bytes();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([a, b, c, 255])).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if index <= chunk.len() { char::from(ALPHABET[(n >> shift) as usize & 63]) } else { '=' });
        }
    }
    out
}

/// A rectangle frame at (`x`, `y`) on the first spread; answers its id.
pub fn frame(doc: &mut LayoutDocument, x: u64, y: u64) -> Result<u64> {
    let (x, y) = (20 + x % 520, 300 + y % 400);
    let made = exec(doc, "frame.add", json!({ "rect": [x, y, x + 40, y + 30], "content": "unassigned" }))?;
    made["id"].as_u64().ok_or_else(|| format!("frame.add answered {made}"))
}

/// The layout every scenario starts from: three pages; on the first, a headline text frame of
/// two paragraphs, two pairs of grouped rectangles, and a loose one with a fill.
pub fn starting_layout() -> Result<LayoutDocument> {
    let mut doc = LayoutDocument::create(NAME, WIDTH, HEIGHT)?;
    exec(&mut doc, "frame.add", json!({ "rect": [72, 72, 540, 200], "content": "text", "text": "Spring concert" }))?;
    let id = story(&doc).ok_or("no story")?;
    exec(&mut doc, "story.setText", json!({ "story": id, "text": "Spring concert\rIn the park at noon" }))?;
    let loose = frame(&mut doc, 0, 0)?;
    exec(&mut doc, "object.fill", json!({ "ids": [loose], "swatch": SWATCHES[0] }))?;
    let a = [frame(&mut doc, 60, 0)?, frame(&mut doc, 120, 0)?];
    let b = [frame(&mut doc, 60, 60)?, frame(&mut doc, 120, 60)?];
    for pair in [a, b] {
        exec(&mut doc, "arrange.group", json!({ "ids": pair }))?;
    }
    exec(&mut doc, "layout.pages.insert", json!({ "count": 2 }))?;
    exec(&mut doc, "select.set", json!({ "ids": [] }))?;
    Ok(doc)
}

impl KitAdapter for LayoutKit {
    type Doc = LayoutDocument;

    fn name(&self) -> &str {
        "kit.layout"
    }

    fn schema(&self) -> &str {
        include_str!("../../../../crates/kdm/kdm-model/model/schema.json")
    }

    fn create(&self) -> Result<LayoutDocument> {
        starting_layout()
    }

    fn join(&self, snapshot: &[u8], blobs: &Blobs) -> Result<LayoutDocument> {
        LayoutDocument::open_with_replica(NAME, &[], snapshot, &encode_blobs(blobs))
    }

    fn version(&self, doc: &mut LayoutDocument) -> Result<Vec<u8>> {
        doc.version()
    }

    fn export_changes(&self, doc: &mut LayoutDocument, since: &[u8]) -> Result<Vec<u8>> {
        doc.export_changes(since)
    }

    fn import_changes(&self, doc: &mut LayoutDocument, bytes: &[u8]) -> Result<()> {
        doc.import_changes(bytes).map(|_| ())
    }

    fn snapshot(&self, doc: &mut LayoutDocument) -> Result<Vec<u8>> {
        doc.snapshot()
    }

    fn state(&self, doc: &mut LayoutDocument) -> Result<Value> {
        let snapshot = doc.snapshot()?;
        let replica = Replica::load(kition_layout_kdm::schema(), &snapshot, ActorId::random()).map_err(|e| e.to_string())?;
        replica.state_json().map_err(|e| e.to_string())
    }

    fn undo(&self, doc: &mut LayoutDocument) -> Result<bool> {
        doc.undo().map(|(done, _)| done)
    }

    fn redo(&self, doc: &mut LayoutDocument) -> Result<bool> {
        doc.redo().map(|(done, _)| done)
    }

    fn random_edit(&self, doc: &mut LayoutDocument, rng: &mut Rng) -> Result<Option<String>> {
        let leaves = leaves(doc);
        let target = rng.pick(&leaves).copied();
        let value = rng.next_u64() % 10_000;
        let (label, done) = match (rng.below(16), target) {
            (0 | 1, _) => ("frame", frame(doc, value, value / 7).is_ok()),
            (2, Some(id)) => ("fill", attempt(doc, "object.fill", json!({ "ids": [id], "swatch": SWATCHES[value as usize % SWATCHES.len()] }))?),
            (3, Some(id)) if leaves.len() > 3 => ("delete", attempt(doc, "edit.delete", json!({ "ids": [id] }))?),
            (4 | 5, Some(id)) => {
                ("move", attempt(doc, "transform.move", json!({ "ids": [id], "dx": (value % 21) as f64 - 10.0, "dy": (value % 13) as f64 - 6.0 }))?)
            }
            (6, Some(id)) => {
                exec(doc, "select.set", json!({ "ids": [id] }))?;
                ("duplicate", attempt(doc, "edit.duplicate", json!({}))?)
            }
            (7, _) => {
                let tops = tops(doc);
                let Some(&id) = rng.pick(&tops) else { return Ok(None) };
                exec(doc, "select.set", json!({ "ids": [id] }))?;
                let order = ["arrange.bringToFront", "arrange.sendToBack", "arrange.bringForward", "arrange.sendBackward"];
                ("arrange", attempt(doc, order[rng.below(order.len())], json!({}))?)
            }
            (8, _) => {
                let tops = tops(doc);
                let (Some(&a), Some(&b)) = (rng.pick(&tops), rng.pick(&tops)) else { return Ok(None) };
                if a == b {
                    return Ok(None);
                }
                ("group", attempt(doc, "arrange.group", json!({ "ids": [a, b] }))?)
            }
            (9, _) => {
                let groups = groups(doc);
                let Some(&group) = rng.pick(&groups) else { return Ok(None) };
                ("ungroup", attempt(doc, "arrange.ungroup", json!({ "ids": [group] }))?)
            }
            (10, _) => {
                let length = story_text(doc)?.map_or(0, |text| text.chars().count());
                let typed = format!("t{value}{}", ['\u{4E2D}', '\u{6587}', '\u{1F600}'][value as usize % 3]);
                ("type", replace(doc, rng.below(length + 1), 0, &typed)?)
            }
            (11, _) => {
                let length = story_text(doc)?.map_or(0, |text| text.chars().count());
                if length < 2 {
                    return Ok(None);
                }
                let at = rng.below(length - 1);
                ("erase", replace(doc, at, 1 + rng.below(2.min(length - at)), "")?)
            }
            (12, _) => ("add page", attempt(doc, "layout.pages.insert", json!({ "count": 1 }))?),
            (13, _) => {
                let count = pages(doc);
                if count < 2 {
                    return Ok(None);
                }
                ("move page", attempt(doc, "layout.pages.move", json!({ "from": value % count, "to": (value / 3) % count }))?)
            }
            (14, _) => ("place image", self.script(doc, &Script::InsertBlob { seed: value as u32 })?),
            (15, Some(id)) => ("rename", attempt(doc, "object.rename", json!({ "id": id, "name": format!("Object {value}") }))?),
            _ => return Ok(None),
        };
        Ok(done.then(|| label.to_owned()))
    }

    fn script(&self, doc: &mut LayoutDocument, step: &Script) -> Result<bool> {
        match step {
            Script::SetField { slot: 0, value } => {
                // The probe's geometry.
                let id = probe(doc)?;
                exec(doc, "transform.move", json!({ "ids": [id], "dx": 1 + (*value % 40), "dy": 1 + (*value % 30) }))?;
            }
            Script::SetField { value, .. } => {
                // The probe's fill.
                let id = probe(doc)?;
                exec(doc, "object.fill", json!({ "ids": [id], "swatch": SWATCHES[*value as usize % SWATCHES.len()] }))?;
            }
            Script::Create => {
                frame(doc, 300, 200)?;
            }
            Script::Delete => {
                let id = probe(doc)?;
                exec(doc, "edit.delete", json!({ "ids": [id] }))?;
            }
            Script::Duplicate => {
                let id = probe(doc)?;
                exec(doc, "select.set", json!({ "ids": [id] }))?;
                exec(doc, "edit.duplicate", json!({}))?;
            }
            Script::Reorder { which } => {
                // Pages: the first to the end, or the last to the front.
                let last = pages(doc).saturating_sub(1);
                let (from, to) = if *which == 0 { (0, last) } else { (last, 0) };
                exec(doc, "layout.pages.move", json!({ "from": from, "to": to }))?;
            }
            // No command moves an existing object into another group (grouping makes a new
            // group node): the reparent scenario is skipped.
            Script::Reparent { .. } => return Ok(false),
            Script::InsertText { at, text } => return replace(doc, *at, 0, text),
            Script::DeleteText { at, len } => return replace(doc, *at, *len, ""),
            Script::SplitParagraph { at } => return replace(doc, *at, 0, "\r"),
            Script::MergeParagraph { at } => return replace(doc, *at, 1, ""),
            Script::InsertBlob { seed } => {
                exec(doc, "file.place", json!({ "name": format!("picture-{seed}.png"), "base64": base64(&png(*seed)) }))?;
            }
            _ => return Ok(false),
        }
        exec(doc, "select.set", json!({ "ids": [] }))?;
        Ok(true)
    }

    fn text(&self, doc: &mut LayoutDocument) -> Result<Option<String>> {
        story_text(doc)
    }

    /// The view matches a fresh replica of the same state: the same pages and objects (ids
    /// aside) and the same pixels.
    fn check_view(&self, doc: &mut LayoutDocument) -> Result<()> {
        let snapshot = doc.snapshot()?;
        let blobs = self.blobs(doc)?;
        let fresh = self.join(&snapshot, &blobs)?;
        // Stories are listed by local id, which each replica mints as it reads: compare them as
        // a set.
        let view = |doc: &LayoutDocument| {
            let inspected = doc.inspect().unwrap_or(Value::Null);
            let mut stories: Vec<Value> = inspected["stories"].as_array().into_iter().flatten().map(strip_ids).collect();
            stories.sort_by_key(Value::to_string);
            json!({ "spreads": strip_ids(&inspected["spreads"]), "stories": stories })
        };
        if view(doc) != view(&fresh) {
            return Err(format!("the view differs from a fresh replica's: {}", first_difference(&view(doc), &view(&fresh)).unwrap_or_default()));
        }
        let (w, h) = (doc.width(), doc.height());
        if doc.render(0.0, 0.0, 0.25, w / 4, h / 4)? != fresh.render(0.0, 0.0, 0.25, w / 4, h / 4)? {
            return Err("the view renders differently from a fresh replica".into());
        }
        Ok(())
    }

    fn blobs(&self, doc: &mut LayoutDocument) -> Result<Blobs> {
        decode_blobs(&doc.blobs()?)
    }

    fn add_blobs(&self, doc: &mut LayoutDocument, blobs: &Blobs) -> Result<()> {
        doc.add_blobs(&encode_blobs(blobs))
    }

    fn export_file(&self, doc: &mut LayoutDocument) -> Result<Option<Vec<u8>>> {
        doc.save(None).map(Some)
    }
}

/// `value` without ids (local ids may differ between replicas) or what refers to them.
pub fn strip_ids(value: &Value) -> Value {
    const IDS: [&str; 5] = ["id", "story", "frames", "layer", "asset"];
    match value {
        Value::Object(map) => {
            Value::Object(map.iter().filter(|(key, _)| !IDS.contains(&key.as_str())).map(|(k, v)| (k.clone(), strip_ids(v))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(strip_ids).collect()),
        other => other.clone(),
    }
}
