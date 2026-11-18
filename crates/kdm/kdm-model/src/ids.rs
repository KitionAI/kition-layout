//! Engine id bookkeeping (design note section 2.5). The engine's `u64` ids (items, stories,
//! spreads, pages, layers, assets, tables, footnotes, endnotes, text anchors, editorial notes) are
//! what agents and diagnostics name objects by (KHP A46, A57), so each object stores its own in its
//! read-only `number` field and every read gives it that id ([`Minter`]). The writer sets the
//! number when it creates the object; an object that stores none (one a script made through the
//! object model, or one of a file written before the numbers were stored) gets one from the
//! session's repair; two objects storing one number (made concurrently on two replicas) are told
//! apart by every read alike, and the next write stores the new number ([`Ids::unstored`]).
//! [`Ids`] maps the ids a view holds to their objects; [`Known`] keeps the document's id counter
//! above every id a view handed out.
//!
//! Styles and swatches have no id in the engine (they are named): their objects are kept in the
//! view's list order and paired between two versions of a list by content, then by name
//! ([`crate::list::align`]).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use kition_kit::kdm::{NodeRef, ObjectId};
use kition_layout_doc::{Document, Item, ItemId, Story, StoryId};

use crate::{KdmResult, MAX_DEPTH, kdm_err};

/// The five style families, in the order of [`FAMILY_FIELDS`].
pub const FAMILIES: usize = 5;
/// The `Layout` list of each style family: paragraph, character, object, table, cell.
pub const FAMILY_FIELDS: [&str; FAMILIES] = ["paragraphStyles", "characterStyles", "objectStyles", "tableStyles", "cellStyles"];
/// Scoped lookup keys: the SDK name guard matches fields, while styles share one ref type.
pub const FAMILY_NAMES: [&str; FAMILIES] = ["paragraphName", "characterName", "objectName", "tableName", "cellName"];
pub(crate) const PARAGRAPH: usize = 0;
pub(crate) const CHARACTER: usize = 1;
pub(crate) const OBJECT: usize = 2;
pub(crate) const TABLE: usize = 3;
pub(crate) const CELL: usize = 4;

/// What kind of inline object a placeholder character anchors, and the engine's id for it (its
/// own id where it has one, else its occurrence among the story's inlines of that kind).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct InlineKey {
    pub kind: InlineKind,
    pub local: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum InlineKind {
    AnchoredObject,
    Footnote,
    Endnote,
    CrossReference,
    TextAnchor,
    IndexEntry,
    EditorialNote,
}

impl InlineKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            InlineKind::AnchoredObject => "anchoredObject",
            InlineKind::Footnote => "footnote",
            InlineKind::Endnote => "endnote",
            InlineKind::CrossReference => "crossReference",
            InlineKind::TextAnchor => "textAnchor",
            InlineKind::IndexEntry => "indexEntry",
            InlineKind::EditorialNote => "editorialNote",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "anchoredObject" => InlineKind::AnchoredObject,
            "footnote" => InlineKind::Footnote,
            "endnote" => InlineKind::Endnote,
            "crossReference" => InlineKind::CrossReference,
            "textAnchor" => InlineKind::TextAnchor,
            "indexEntry" => InlineKind::IndexEntry,
            "editorialNote" => InlineKind::EditorialNote,
            _ => return None,
        })
    }

    /// The engine's placeholder character for this kind.
    pub(crate) fn mark(self) -> char {
        match self {
            InlineKind::AnchoredObject => kition_layout_doc::OBJECT_MARK,
            InlineKind::Footnote => kition_layout_doc::FOOTNOTE_REF,
            InlineKind::Endnote => kition_layout_doc::ENDNOTE_REF,
            InlineKind::CrossReference => kition_layout_doc::XREF_MARK,
            InlineKind::TextAnchor => kition_layout_doc::ANCHOR_MARK,
            InlineKind::IndexEntry => kition_layout_doc::INDEX_MARK,
            InlineKind::EditorialNote => kition_layout_doc::NOTE_MARK,
        }
    }

    pub(crate) fn of_char(c: char) -> Option<Self> {
        [
            InlineKind::AnchoredObject,
            InlineKind::Footnote,
            InlineKind::Endnote,
            InlineKind::CrossReference,
            InlineKind::TextAnchor,
            InlineKind::IndexEntry,
            InlineKind::EditorialNote,
        ]
        .into_iter()
        .find(|k| k.mark() == c)
    }
}

/// A table's objects: the table, its rows and its columns in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TableIds {
    pub object: Option<ObjectId>,
    pub rows: Vec<ObjectId>,
    pub columns: Vec<ObjectId>,
}

/// Object ids to local ids and back.
#[derive(Clone, Debug)]
pub struct IdMap<L> {
    to_local: HashMap<ObjectId, L>,
    to_object: HashMap<L, ObjectId>,
}

impl<L> Default for IdMap<L> {
    fn default() -> Self {
        Self { to_local: HashMap::new(), to_object: HashMap::new() }
    }
}

impl<L: Copy + Eq + std::hash::Hash> IdMap<L> {
    /// Maps `object` to `local`, replacing either's earlier mapping.
    pub fn insert(&mut self, object: ObjectId, local: L) {
        if let Some(old) = self.to_local.remove(&object) {
            self.to_object.remove(&old);
        }
        if let Some(old) = self.to_object.remove(&local) {
            self.to_local.remove(&old);
        }
        self.to_object.insert(local, object.clone());
        self.to_local.insert(object, local);
    }

    pub fn local(&self, object: &ObjectId) -> Option<L> {
        self.to_local.get(object).copied()
    }

    pub fn object(&self, local: L) -> Option<&ObjectId> {
        self.to_object.get(&local)
    }

    /// Forgets `local`; answers its object.
    pub fn remove_local(&mut self, local: L) -> Option<ObjectId> {
        let object = self.to_object.remove(&local)?;
        self.to_local.remove(&object);
        Some(object)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ObjectId, L)> {
        self.to_local.iter().map(|(o, l)| (o, *l))
    }

    pub fn len(&self) -> usize {
        self.to_local.len()
    }

    pub fn is_empty(&self) -> bool {
        self.to_local.is_empty()
    }
}

/// The objects behind a view's local ids.
#[derive(Clone, Debug, Default)]
pub struct Ids {
    pub items: IdMap<ItemId>,
    /// Each item object's tree node, so a later write can move it or put children under it.
    pub(crate) tree: HashMap<ObjectId, NodeRef>,
    pub stories: IdMap<StoryId>,
    /// Spreads and parent spreads.
    pub spreads: IdMap<u64>,
    pub pages: IdMap<u64>,
    pub layers: IdMap<u64>,
    pub assets: IdMap<u64>,
    /// Parallel to each style family's list (see [`FAMILY_FIELDS`]).
    pub(crate) styles: [Vec<ObjectId>; FAMILIES],
    /// Parallel to the document's swatches.
    pub(crate) swatches: Vec<ObjectId>,
    /// Parallel to the document's sections.
    pub(crate) sections: Vec<Option<ObjectId>>,
    /// Per spread: its guides' objects, in page order then each page's guide order.
    pub(crate) guides: HashMap<u64, Vec<ObjectId>>,
    /// Per story: its inline objects in text order.
    pub(crate) inlines: HashMap<StoryId, Vec<(InlineKey, ObjectId)>>,
    /// Per story and table id.
    pub(crate) tables: HashMap<(StoryId, u64), TableIds>,
    /// The objects the last read gave another id than the number they store (or that store none),
    /// with that id: the next write stores it.
    pub(crate) unstored: Vec<(ObjectId, u64)>,
}

impl Ids {
    /// The object of item `id`.
    pub fn item(&self, id: ItemId) -> Option<&ObjectId> {
        self.items.object(id)
    }

    /// The object of story `id`.
    pub fn story(&self, id: StoryId) -> Option<&ObjectId> {
        self.stories.object(id)
    }

    /// The object of the style at `index` of family `family` (see [`FAMILY_FIELDS`]).
    pub fn style(&self, family: usize, index: usize) -> Option<&ObjectId> {
        self.styles.get(family).and_then(|list| list.get(index))
    }

    /// The object of the swatch at `index` of the document's swatches.
    pub fn swatch(&self, index: usize) -> Option<&ObjectId> {
        self.swatches.get(index)
    }

    /// The object of table `table` of story `story`.
    pub fn table(&self, story: StoryId, table: u64) -> Option<&ObjectId> {
        self.tables.get(&(story, table)).and_then(|t| t.object.as_ref())
    }
}

/// What a view has handed out: the document's id counter never goes below an id it gave, so a
/// deleted object's id is not given again. See [`Known::remember`].
#[derive(Clone, Debug, Default)]
pub struct Known {
    /// The highest id ever handed out or seen.
    pub(crate) max: u64,
}

impl Known {
    /// Remembers the ids `doc` handed out.
    pub fn remember(&mut self, doc: &Document) {
        self.max = self.max.max(doc.next_id);
    }
}

/// Whether the engine gives inline objects of `kind` an id of their own (else they are keyed by
/// occurrence).
pub(crate) fn has_own_id(kind: InlineKind) -> bool {
    matches!(kind, InlineKind::AnchoredObject | InlineKind::Footnote | InlineKind::Endnote | InlineKind::TextAnchor | InlineKind::EditorialNote)
}

/// Where an engine id must be unique: the engine's own id spaces. Layers, spreads (parent spreads
/// included), pages, stories, assets and items are numbered from the document's one counter
/// (`Document::next_id`); a story numbers its tables, and each kind of its inline objects
/// (footnotes, endnotes, text anchors, editorial notes), itself.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Domain {
    /// All objects numbered by the document counter share this collision domain.
    Global,
    Layer,
    Spread,
    Page,
    Story,
    Asset,
    Item,
    /// The tables of the story object.
    Table(ObjectId),
    /// The numbered inline objects of one kind of the story object.
    Inline(ObjectId, InlineKind),
}

impl Domain {
    fn space(&self) -> Self {
        if self.counted() { Self::Global } else { self.clone() }
    }
    /// Whether the document's counter numbers this domain (so `next_id` stays above its numbers).
    fn counted(&self) -> bool {
        !matches!(self, Domain::Table(_) | Domain::Inline(..))
    }
}

/// The number an object stores (its `number` field), if it is a valid engine id.
pub(crate) fn stored_number(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().filter(|n| (1..=kition_layout_doc::MAX_NEXT_ID).contains(n))
}

/// Mints engine ids for the objects of one read, given in reading order with the number each
/// stores (design note section 2.5):
///
/// 1. an object keeps the number it stores, unless another object of its domain stores it too:
///    then the one with the smallest object id keeps it (two replicas that created objects
///    concurrently minted one number);
/// 2. every other object (one that stores no number, or lost its number so) gets a fresh one, in
///    reading order, above every number stored in the counted domains.
///
/// The result depends on the replica's state alone, so every replica holding one state gives each
/// object the same id, whatever order the changes arrived in. A replica written before the numbers
/// were stored has none: its objects number in reading order from 1, as they always did.
pub(crate) struct Minter {
    keep: HashMap<ObjectId, u64>,
    taken: HashSet<(Domain, u64)>,
    next: u64,
}

impl Minter {
    pub(crate) fn new(order: &[(Domain, ObjectId, Option<u64>)]) -> Self {
        let mut holders: HashMap<(Domain, u64), &ObjectId> = HashMap::new();
        let mut ceiling = 0;
        for (domain, object, number) in order {
            let Some(number) = *number else { continue };
            if domain.counted() {
                ceiling = ceiling.max(number);
            }
            holders.entry((domain.space(), number)).and_modify(|holder| *holder = (*holder).min(object)).or_insert(object);
        }
        let keep = holders.iter().map(|((_, number), object)| ((*object).clone(), *number)).collect();
        let taken = holders.into_keys().collect();
        Self { keep, taken, next: ceiling }
    }

    pub(crate) fn mint(&mut self, domain: &Domain, object: &ObjectId) -> u64 {
        if let Some(id) = self.keep.get(object) {
            return *id;
        }
        loop {
            self.next = self.next.saturating_add(1);
            if self.taken.insert((domain.space(), self.next)) {
                return self.next;
            }
        }
    }

    /// The largest id the document's counter must stay above: every counted number handed out or
    /// stored.
    pub(crate) fn max(&self) -> u64 {
        self.next
    }
}

/// Visits every item of `items` and their descendants (as deep as the bridge goes).
pub(crate) fn walk<'a>(items: &'a [Arc<Item>], depth: usize, f: &mut dyn FnMut(&'a Arc<Item>)) {
    if depth > MAX_DEPTH {
        return;
    }
    for item in items {
        f(item);
        if let kition_layout_doc::Content::Group { items } = &item.content {
            walk(items, depth + 1, f);
        }
    }
}

/// The anchored objects of `story`, in text order.
pub(crate) fn anchored(story: &Story) -> impl Iterator<Item = &Item> {
    story.objects.iter().map(|o| &o.item)
}

/// Every item of `doc` (spreads, parents, and objects anchored in stories, with their group
/// children), by id: the first holder wins. An item two versions share (an `Arc` neither
/// changed) is the same reference in both indexes, so `std::ptr::eq` finds untouched items.
pub(crate) fn index(doc: &Document) -> HashMap<ItemId, &Item> {
    let mut out: HashMap<ItemId, &Item> = HashMap::new();
    for spread in doc.spreads.iter().chain(&doc.parents) {
        walk(&spread.items, 0, &mut |item| {
            out.entry(item.id).or_insert(item);
        });
    }
    for story in doc.stories.values() {
        for item in anchored(story) {
            out.entry(item.id).or_insert(item);
            if let kition_layout_doc::Content::Group { items } = &item.content {
                walk(items, 1, &mut |child| {
                    out.entry(child.id).or_insert(child);
                });
            }
        }
    }
    out
}

/// Errs if `doc` holds an id twice where a replica needs it unique, or nests deeper than the
/// bridge goes: writing it would merge two objects into one, or lose what is too deep.
pub(crate) fn check_unique(doc: &Document) -> KdmResult<()> {
    let mut seen = HashSet::new();
    let mut duplicate = None;
    let mut too_deep = false;
    let mut visit = |item: &Item, depth: usize| {
        if depth > MAX_DEPTH {
            too_deep = true;
        }
        if !seen.insert(item.id) && duplicate.is_none() {
            duplicate = Some(item.id);
        }
    };
    fn rec(items: &[Arc<Item>], depth: usize, visit: &mut dyn FnMut(&Item, usize)) {
        for item in items {
            visit(item, depth);
            if depth <= MAX_DEPTH
                && let kition_layout_doc::Content::Group { items } = &item.content
            {
                rec(items, depth + 1, visit);
            }
        }
    }
    for spread in doc.spreads.iter().chain(&doc.parents) {
        rec(&spread.items, 0, &mut visit);
    }
    for story in doc.stories.values() {
        for item in anchored(story) {
            visit(item, 0);
            if let kition_layout_doc::Content::Group { items } = &item.content {
                rec(items, 1, &mut visit);
            }
        }
    }
    if too_deep {
        return Err(kdm_err(format!("the document nests groups deeper than {MAX_DEPTH}; refusing to write it")));
    }
    if let Some(id) = duplicate {
        return Err(kdm_err(format!("the document holds item {id} more than once; refusing to write it")));
    }
    fn unique<T: std::hash::Hash + Eq + std::fmt::Display>(what: &str, ids: impl Iterator<Item = T>) -> KdmResult<()> {
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(id) {
                return Err(kdm_err(format!("the document holds {what} more than once; refusing to write it")));
            }
        }
        Ok(())
    }
    unique("a spread id", doc.spreads.iter().chain(&doc.parents).map(|s| s.id))?;
    unique("a page id", doc.spreads.iter().chain(&doc.parents).flat_map(|s| s.pages.iter().map(|p| p.id)))?;
    unique("a layer id", doc.layers.iter().map(|l| l.id))?;
    for story in doc.stories.values() {
        let mut frames = HashSet::new();
        for frame in &story.frames {
            if !frames.insert(*frame) {
                return Err(kdm_err(format!("story {} threads frame {frame} twice; refusing to write it", story.id)));
            }
        }
    }
    Ok(())
}

/// The parent entry of each entry of a tree listed in pre-order (`None`: a root): a parent always
/// comes before its children, so a stack of the current ancestors finds it.
pub(crate) fn tree_parents(entries: &[kition_kit::kdm::TreeEntry]) -> Vec<Option<usize>> {
    let mut out = Vec::with_capacity(entries.len());
    let mut stack: Vec<usize> = Vec::new();
    for e in entries {
        let parent = match &e.parent {
            None => {
                stack.clear();
                None
            }
            Some(p) => {
                while stack.last().is_some_and(|top| entries[*top].node != *p) {
                    stack.pop();
                }
                stack.last().copied()
            }
        };
        out.push(parent);
        stack.push(out.len() - 1);
    }
    out
}
