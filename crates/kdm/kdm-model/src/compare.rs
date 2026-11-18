//! [`normalize`]: an id-blind form of a [`Document`], for comparing views. Local ids are minted by
//! every read (never replicated), so two views of the same content — a round trip, or two replicas
//! that converged — rarely share them.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use kition_layout_doc::{
    AssetId, CharRun, Content, Document, HyperlinkSource, Item, ItemId, LayerId, PageId, PlaceholderAnchor, SpreadId, Story, StoryId,
};
use serde_json::Value;

use crate::MAX_DEPTH;

#[derive(Default)]
struct Renumber {
    items: HashMap<u64, u64>,
    stories: HashMap<u64, u64>,
    spreads: HashMap<u64, u64>,
    pages: HashMap<u64, u64>,
    layers: HashMap<u64, u64>,
    assets: HashMap<u64, u64>,
    anchors: HashMap<u64, u64>,
}

fn number(map: &mut HashMap<u64, u64>, id: u64) {
    let next = map.len() as u64 + 1;
    map.entry(id).or_insert(next);
}

fn get(map: &HashMap<u64, u64>, id: u64) -> u64 {
    map.get(&id).copied().unwrap_or(0)
}

fn walk_items(items: &[Arc<Item>], depth: usize, f: &mut dyn FnMut(&Item)) {
    if depth > MAX_DEPTH {
        return;
    }
    for item in items {
        f(item);
        if let Content::Group { items } = &item.content {
            walk_items(items, depth + 1, f);
        }
    }
}

/// The stories of `d` in an order every replica shares: by the number of their first frame
/// (frameless ones last), then by text.
fn story_order<'a>(d: &'a Document, items: &HashMap<u64, u64>) -> Vec<&'a Story> {
    let mut stories: Vec<&Story> = d.stories.values().map(|s| &**s).collect();
    let key = |s: &Story| (s.frames.iter().filter_map(|f| items.get(&f.0).copied()).min().unwrap_or(u64::MAX), s.text.clone(), s.frames.len());
    stories.sort_by_cached_key(|s| key(s));
    stories
}

/// Merges adjacent runs with the same format and drops empty runs but the last (the engine
/// treats both the same).
fn normalize_runs(story: &mut Story) {
    let mut runs: Vec<CharRun> = Vec::with_capacity(story.chars.len());
    let n = story.chars.len();
    for (i, run) in std::mem::take(&mut story.chars).into_iter().enumerate() {
        if run.len == 0 && i + 1 < n {
            continue;
        }
        match runs.last_mut() {
            Some(last) if last.format == run.format => last.len += run.len,
            _ => runs.push(run),
        }
    }
    story.chars = runs;
    for para in &mut story.paras {
        para.chars = Default::default();
    }
    story.rev = 0;
}

fn nested(story: &mut Story, depth: usize) {
    normalize_runs(story);
    if depth > 8 {
        return;
    }
    for table in story.tables.values_mut() {
        for cell in &mut Arc::make_mut(table).cells {
            nested(&mut cell.text, depth + 1);
        }
    }
    for note in story.notes.iter_mut().chain(story.endnotes.iter_mut()) {
        nested(&mut Arc::make_mut(note).text, depth + 1);
    }
}

impl Renumber {
    /// Numbers by an order every replica shares: items in tree order; stories by their first
    /// frame, then text (the engine keys stories by local id, which differs between replicas);
    /// anchored objects in that story order; assets by content.
    fn of(d: &Document) -> Self {
        let mut r = Renumber::default();
        for layer in &d.layers {
            number(&mut r.layers, layer.id.0);
        }
        for spread in d.spreads.iter().chain(&d.parents) {
            number(&mut r.spreads, spread.id.0);
            for page in &spread.pages {
                number(&mut r.pages, page.id.0);
            }
            walk_items(&spread.items, 0, &mut |i| number(&mut r.items, i.id.0));
        }
        for story in story_order(d, &r.items) {
            number(&mut r.stories, story.id.0);
            for object in &story.objects {
                number(&mut r.items, object.item.id.0);
                if let Content::Group { items } = &object.item.content {
                    walk_items(items, 1, &mut |i| number(&mut r.items, i.id.0));
                }
            }
            for anchor in &story.anchors {
                number(&mut r.anchors, anchor.id);
            }
        }
        let mut assets: Vec<&kition_layout_doc::Asset> = d.assets.values().map(|a| &**a).collect();
        assets.sort_by(|a, b| (&a.name, &a.mime, a.data.as_slice()).cmp(&(&b.name, &b.mime, b.data.as_slice())));
        for asset in assets {
            number(&mut r.assets, asset.id.0);
        }
        r
    }

    fn item(&self, item: &mut Item, depth: usize) {
        item.id = ItemId(get(&self.items, item.id.0));
        item.layer = LayerId(get(&self.layers, item.layer.0));
        item.overrides = item.overrides.map(|o| ItemId(get(&self.items, o.0)));
        if let Some(caption) = &mut item.live_caption {
            caption.source = ItemId(get(&self.items, caption.source.0));
        }
        if let Some(media) = &mut item.media {
            media.poster = media.poster.map(|p| AssetId(get(&self.assets, p.0)));
        }
        match &mut item.content {
            Content::Text(frame) => frame.story = StoryId(get(&self.stories, frame.story.0)),
            Content::Graphic(g) => g.asset = AssetId(get(&self.assets, g.asset.0)),
            Content::Group { items } if depth <= MAX_DEPTH => {
                for child in items {
                    self.item(Arc::make_mut(child), depth + 1);
                }
            }
            _ => {}
        }
    }

    fn story(&self, story: &mut Story) {
        nested(story, 0);
        story.id = StoryId(get(&self.stories, story.id.0));
        story.frames = story.frames.iter().map(|f| ItemId(get(&self.items, f.0))).collect();
        // Tables renumbered by the paragraph anchoring them, then by content (their numbers are
        // local).
        let mut order: Vec<(usize, String, u64)> = story
            .tables
            .iter()
            .map(|(k, t)| {
                let at = story.paras.iter().position(|p| p.table == Some(*k)).unwrap_or(usize::MAX);
                (at, serde_json::to_string(&**t).unwrap_or_default(), *k)
            })
            .collect();
        order.sort();
        let numbers: HashMap<u64, u64> = order.iter().enumerate().map(|(i, (_, _, k))| (*k, i as u64 + 1)).collect();
        let tables: BTreeMap<u64, _> = std::mem::take(&mut story.tables)
            .into_iter()
            .map(|(k, mut t)| {
                let n = numbers.get(&k).copied().unwrap_or(0);
                Arc::make_mut(&mut t).id = n;
                (n, t)
            })
            .collect();
        story.tables = tables;
        for para in &mut story.paras {
            para.table = para.table.map(|t| numbers.get(&t).copied().unwrap_or(0));
        }
        for (i, note) in story.notes.iter_mut().enumerate() {
            Arc::make_mut(note).id = i as u64 + 1;
        }
        for (i, note) in story.endnotes.iter_mut().enumerate() {
            Arc::make_mut(note).id = i as u64 + 1;
        }
        for anchor in &mut story.anchors {
            let a = Arc::make_mut(anchor);
            a.id = get(&self.anchors, a.id);
        }
        for xref in &mut story.xrefs {
            let x = Arc::make_mut(xref);
            x.target = get(&self.anchors, x.target);
        }
        for (i, note) in story.editorial.iter_mut().enumerate() {
            Arc::make_mut(note).id = i as u64 + 1;
        }
        for object in &mut story.objects {
            self.item(&mut Arc::make_mut(object).item, 0);
        }
        story.link = story.link.map(|(s, rev)| (StoryId(get(&self.stories, s.0)), rev));
    }
}

/// The id-blind JSON of `d`: every local id renumbered by position (items in tree order, then the
/// objects anchored in stories; stories, assets and tables in key order; footnotes, endnotes and
/// editorial notes by position), every reference rewritten to match, and what the bridge does not
/// store left out (the id counter, assets' disk paths, pages' view rotation, stories' revision
/// counter), character runs merged where adjacent runs have one format. Two documents with the
/// same content normalize to the same value whatever ids either holds.
pub fn normalize(d: &Document) -> Value {
    let r = Renumber::of(d);
    let mut d = d.clone();
    d.next_id = 0;
    d.font_scope = 0;
    d.layers.iter_mut().for_each(|l| l.id = LayerId(get(&r.layers, l.id.0)));
    for spread in d.spreads.iter_mut().chain(d.parents.iter_mut()) {
        let s = Arc::make_mut(spread);
        s.id = SpreadId(get(&r.spreads, s.id.0));
        if let Some(info) = &mut s.parent {
            info.based_on = info.based_on.map(|b| SpreadId(get(&r.spreads, b.0)));
        }
        for page in &mut s.pages {
            page.id = PageId(get(&r.pages, page.id.0));
            page.parent = page.parent.map(|p| SpreadId(get(&r.spreads, p.0)));
            page.overridden = page.overridden.iter().map(|o| ItemId(get(&r.items, o.0))).collect();
            page.view_rotation = 0;
            for guide in &mut page.guides {
                guide.layer = guide.layer.map(|l| LayerId(get(&r.layers, l.0)));
            }
        }
        for item in &mut s.items {
            r.item(Arc::make_mut(item), 0);
        }
    }
    d.stories = std::mem::take(&mut d.stories)
        .into_values()
        .map(|mut story| {
            r.story(Arc::make_mut(&mut story));
            (story.id, story)
        })
        .collect();
    d.assets = std::mem::take(&mut d.assets)
        .into_values()
        .map(|mut asset| {
            let a = Arc::make_mut(&mut asset);
            a.id = AssetId(get(&r.assets, a.id.0));
            a.link = None;
            (a.id, asset)
        })
        .collect();
    d.settings.primary_story = d.settings.primary_story.map(|s| StoryId(get(&r.stories, s.0)));
    d.endnote_story = d.endnote_story.map(|s| StoryId(get(&r.stories, s.0)));
    for article in &mut d.articles {
        article.items = article.items.iter().map(|i| ItemId(get(&r.items, i.0))).collect();
    }
    for link in &mut d.hyperlinks {
        match &mut link.source {
            HyperlinkSource::Item { id } => *id = ItemId(get(&r.items, id.0)),
            HyperlinkSource::Text { story, .. } => *story = StoryId(get(&r.stories, story.0)),
        }
    }
    for p in &mut d.data_merge.placeholders {
        match &mut p.anchor {
            PlaceholderAnchor::Item { id } => *id = ItemId(get(&r.items, id.0)),
            PlaceholderAnchor::Text { story, .. } => *story = StoryId(get(&r.stories, story.0)),
        }
    }
    if let Some(toc) = &mut d.toc {
        toc.story = StoryId(get(&r.stories, toc.story.0));
    }
    if let Some(index) = &mut d.index {
        index.story = StoryId(get(&r.stories, index.story.0));
    }
    let mut v = serde_json::to_value(&d).unwrap_or(Value::Null);
    whole_numbers(&mut v, 0);
    v
}

/// Rewrites every double holding a whole number within ±2^53 as that integer, as a KDM replica
/// keeps numbers, and drops `null` object entries.
fn whole_numbers(v: &mut Value, depth: usize) {
    const SAFE: f64 = 9_007_199_254_740_992.0;
    if depth > MAX_DEPTH * 4 {
        return;
    }
    match v {
        Value::Number(n) if !n.is_i64() && !n.is_u64() => {
            if let Some(f) = n.as_f64().filter(|f| f.is_finite() && f.fract() == 0.0 && f.abs() <= SAFE) {
                #[allow(clippy::cast_possible_truncation)]
                let i = f as i64;
                *v = Value::from(i);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|x| whole_numbers(x, depth + 1)),
        Value::Object(map) => {
            // `null` and a missing key read the same (serde reads both as `None`).
            map.retain(|_, x| !x.is_null());
            map.values_mut().for_each(|x| whole_numbers(x, depth + 1));
        }
        _ => {}
    }
}
