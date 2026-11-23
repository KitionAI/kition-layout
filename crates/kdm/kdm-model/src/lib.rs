//! Bridges the Layout engine's [`Document`] ([`kition_layout_doc`]) to a `kit.layout` KDM replica.
//!
//! [`LayoutModel`] implements [`KitModel`]: the engine's materialised view of a replica, the same
//! [`Document`] the rest of the engine already knows. [`write_document`] and [`write_changes`]
//! are the entry points of the engine command bridge (DE3): writing a whole document in one
//! creating transaction, or only what a command changed, found by comparing the documents before
//! and after it (`Arc` sharing skips untouched spreads, items and stories).
//!
//! What lives where (`model/schema.json`):
//! - every item, of every spread and parent spread and every object anchored in a story, is one
//!   `Item` object named by one node of the `Layout.items` **tree**: a top-level node carries its
//!   `spread` (anchored objects have none), group children are tree children. A move to another
//!   spread, a group, an ungroup and a reorder are each one tree move. Coupled values sit in one
//!   `json` register each ([`COUPLED_VALUES`], the M3 record);
//! - spreads and parent spreads, pages, guides, layers, stories, the five style families,
//!   swatches, assets (bytes as blobs), sections, hyperlinks, data merge placeholders, conditions,
//!   text variables and cross-reference formats are objects in lists of the `Layout`;
//! - a story's text is one `text` field with rich-text marks ([`marks`]): 61 character marks, the
//!   paragraph style and 13 grouped paragraph marks on each paragraph's terminating `\n`,
//!   `inline`/`table` anchors, `variable` on text variable instances, and `hyperlink` and
//!   `placeholder` over the text a hyperlink starts from or a placeholder fills. Threading is the
//!   story's ordered `frames` list;
//! - a table's rows and columns are objects; its cells are objects under derived ids ([`cells`]);
//! - every other key of the document (about 20 long-tail collections and options) is one entry
//!   of the `Layout.settings` map.
//!
//! **Ids.** Every object the engine numbers stores its engine id in its read-only `number` field
//! (written when the bridge creates it), so every read gives it the same id: agents and
//! diagnostics name objects by these ids (KHP A46, A57). [`read_document`] reads them (an object
//! not numbered yet, or storing a number another object keeps, gets a fresh one, the same on every
//! replica) and [`Ids`] maps them to object ids. Every id stored as data is written as the object
//! id it names; swatch names as `{"ref", "name"}`.
//!
//! **Repair.** [`KitModel::validate`] applies the rules V1–V10, V12 and V13 of section 2.6 in a
//! transaction; V6 needs the replica's text marks, which a transaction cannot read, so
//! [`repair::read_repairs`] finds those and [`repair::apply`] writes them; V11 (a story text not
//! ending with `\n`) is refused at read. Then [`repair::store_numbers`] numbers the objects that
//! store none. Two objects storing one number (created concurrently) are told apart by every read:
//! the one with the smaller object id keeps it, the other gets a fresh one, which the next write
//! stores.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod cells;
pub mod compare;
mod fields;
pub mod file;
mod ids;
mod inline;
mod list;
pub mod marks;
mod read;
mod refs;
pub mod repair;
pub mod session;
mod text;
mod validate;
mod write;

use std::collections::BTreeSet;
use std::sync::Arc;

use kition_kit::kdm::{ActorId, DocId, KdmError, ObjectId, Origin, Replica, Schema, Tx};
use kition_kit::session::{BlobSink, BlobSource, Damage, KitError, KitModel};
use kition_layout_doc::Document;

pub use file::{FileBase, SavedFile, open_file, save_file};
pub use ids::{FAMILY_FIELDS, FAMILY_NAMES, IdMap, Ids, Known};
pub use read::{read_document, read_document_known};
pub use session::LayoutSession;
pub use write::{write_changes, write_document};

/// Re-exported so downstream crates (the engine command bridge) share this crate's exact pin.
pub use kition_kit;

type Result<T> = kition_kit::session::Result<T>;
pub(crate) type KdmResult<T> = std::result::Result<T, KdmError>;

pub(crate) const SCHEMA: &str = include_str!("../model/schema.json");

/// How deep groups may nest for the bridge (writes refuse deeper trees, reads stop there).
pub(crate) const MAX_DEPTH: usize = 200;

/// The M3 review record (design note section 2.3): each register that holds values which only
/// make sense together, and what they are. A split would let a merge combine two writers'
/// halves into a state nobody chose.
pub const COUPLED_VALUES: &[(&str, &str)] = &[
    ("Item.xf", "the item's transform: six numbers, rotation included"),
    ("Item.path", "a path's points and handles"),
    ("Item.graphic", "an image's placement in its frame: asset, size, inner transform, fitting, crop"),
    ("Item.fill", "fill colour, tint, gradient"),
    ("Item.stroke", "stroke weight, type, colour, gap colour, ends, alignment"),
    ("Item.corners", "corner shape and radius per corner"),
    ("Item.textOptions", "text frame options: insets, columns, column width, gutter, vertical justification, first baseline, auto-size"),
    ("Item.wrap", "text wrap: type, offsets, contour"),
    ("Page.size", "page width and height"),
    ("Page.margins", "the four margins"),
    ("Page.columns", "column count, gutter, custom positions"),
    ("Guide.line", "guide orientation, position, extent, page"),
    ("Swatch.value", "a swatch's colour model and values"),
    ("Cell.span", "a cell's row and column span"),
    ("Inline.data", "an inline's anchor data (cross-reference format and target, footnote story options)"),
    ("Style.attrs[cluster]", "each attribute cluster of a style, one map key each (for paragraph styles the 13 paragraph groups)"),
    ("Layout.setup", "the Document Setup dialog: page size defaults, facing pages, bleed, slug, units"),
];

/// The `Layout` object's id: one per replica, minted by [`write_document`] with
/// [`Tx::create_with_id`], so every reader finds it without an index.
pub(crate) fn root_id() -> ObjectId {
    ObjectId::from_string("layout")
}

fn kit_err(error: impl std::fmt::Display) -> KitError {
    KitError::new(error.to_string())
}

pub(crate) fn kdm_err(error: impl std::fmt::Display) -> KdmError {
    KdmError::Engine(error.to_string())
}

/// A file's document, as [`KitModel::import`] reads it: IDML (by content or `name`'s extension),
/// else a `.klayout` file (its replica's state; the engine's own snapshot is not a `.klayout`).
/// The bytes are all there is: an IDML package's links stay links, never read by the path they
/// name (KHP A29, A42).
pub fn decode(name: &str, bytes: &[u8]) -> Result<Document> {
    if kition_layout_idml::is_idml(bytes) || name.to_ascii_lowercase().ends_with(".idml") {
        return kition_layout_idml::import_idml_with(bytes, &|_| None).map_err(kit_err);
    }
    let (session, _) = open_file(bytes)?;
    Ok((**session.document()).clone())
}

/// Whether `bytes` are a `.klayout` file (by its stored `mimetype`, without unzipping).
pub fn is_klayout(bytes: &[u8]) -> bool {
    kition_layout_format::container::is_container(bytes)
}

/// The engine's view of a `kit.layout` replica: the same [`Document`] the rest of the engine
/// knows, plus the id maps [`write_changes`] needs to find each object again.
pub struct LayoutModel {
    document: Arc<Document>,
    ids: Ids,
    known: Known,
}

impl LayoutModel {
    /// A view built by the caller itself: `document` as written by [`write_document`] or
    /// [`write_changes`], with the ids they filled.
    pub fn from_parts(document: Arc<Document>, ids: Ids) -> Self {
        let mut known = Known::default();
        known.remember(&document);
        Self { document, ids, known }
    }

    /// Replaces the view after the caller wrote `document` to the replica itself, with the ids
    /// that write left.
    pub fn adopt(&mut self, document: Arc<Document>, ids: Ids) {
        self.known.remember(&document);
        self.document = document;
        self.ids = ids;
    }

    /// The view: the document as of the last load, apply or adopt.
    pub fn document(&self) -> &Arc<Document> {
        &self.document
    }

    /// The id maps, current as of the last load, apply or adopt.
    pub fn ids(&self) -> &Ids {
        &self.ids
    }

    fn reload(&mut self, replica: &Replica, blobs: &dyn BlobSource) -> Result<()> {
        let (mut document, ids) = read::read_with(replica, blobs, &self.known).map_err(kit_err)?;
        self.known.remember(&document);
        share_unchanged(&self.document, &mut document);
        self.document = Arc::new(document);
        self.ids = ids;
        Ok(())
    }
}

impl std::fmt::Debug for LayoutModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayoutModel")
            .field("spreads", &self.document.spreads.len())
            .field("stories", &self.document.stories.len())
            .finish_non_exhaustive()
    }
}

/// The schema, parsed once per call (a fixed, tested file).
pub fn schema() -> Arc<Schema> {
    #[allow(clippy::expect_used)]
    Arc::new(Schema::parse(SCHEMA).expect("model/schema.json is a fixed, tested file"))
}

/// Refuses raw batch content conversions whose merge repair would remove source content.
pub fn check_batch(tx: &Tx<'_>) -> Result<()> {
    validate::check_batch(tx).map_err(kit_err)
}

impl KitModel for LayoutModel {
    fn schema() -> Arc<Schema> {
        schema()
    }

    fn load(replica: &Replica, blobs: &dyn BlobSource) -> Result<Self> {
        let (document, ids) = read::read_with(replica, blobs, &Known::default()).map_err(kit_err)?;
        Ok(Self::from_parts(Arc::new(document), ids))
    }

    /// Reads the view again, keeping every surviving object's local id, and patches the previous
    /// view with what changed: every spread, item, story, asset and the styles that read back
    /// equal keep the previous view's `Arc` (E11), so the engine's composition and render caches
    /// stay warm and an unchanged story is not composed again. The read itself is whole (about
    /// 8 ms for a 150,000-character story in a debug build, `tests/perf.rs`).
    fn apply(&mut self, replica: &Replica, changed: &BTreeSet<ObjectId>, blobs: &dyn BlobSource) -> Result<Damage> {
        if changed.is_empty() {
            return Ok(Damage::None);
        }
        self.reload(replica, blobs)?;
        Ok(Damage::Full)
    }

    fn validate(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> Result<bool> {
        let changed = validate::validate(tx, changed).map_err(kit_err)?;
        Ok(repair::store_addresses(tx).map_err(kit_err)? || changed)
    }

    fn import(name: &str, bytes: &[u8], blobs: &mut dyn BlobSink, actor: ActorId) -> Result<Replica> {
        let document = decode(name, bytes)?;
        let mut ids = Ids::default();
        let (replica, ()) = Replica::create_with(schema(), DocId::new(), actor, &Origin::system("import"), |tx| {
            write_document(tx, &document, &mut ids, blobs)?;
            Ok(())
        })
        .map_err(kit_err)?;
        Ok(replica)
    }

    /// `.idml` when `name` says so, otherwise a `.klayout` file holding `replica`'s content (no
    /// history, rule M16b) and the blobs its assets reference. Seal the replica first
    /// ([`LayoutSession::seal`]); a replica that is not sealed saves the content of a sealed copy,
    /// whose seal stays out of this replica.
    fn export(&self, replica: &Replica, name: &str, blobs: &dyn BlobSource) -> Result<Vec<u8>> {
        if name.to_ascii_lowercase().ends_with(".idml") {
            return Ok(kition_layout_idml::export_idml(&self.document));
        }
        file::export_file(replica, blobs)
    }
}

/// Gives `new` the previous view's `Arc` of every part that reads back equal (see
/// [`KitModel::apply`] for [`LayoutModel`]).
fn share_unchanged(old: &Document, new: &mut Document) {
    fn share_items(old: &[Arc<kition_layout_doc::Item>], new: &mut [Arc<kition_layout_doc::Item>]) {
        let by_id: std::collections::HashMap<_, _> = old.iter().map(|item| (item.id, item)).collect();
        for item in new.iter_mut() {
            if let Some(previous) = by_id.get(&item.id)
                && ***previous == **item
            {
                *item = Arc::clone(previous);
            }
        }
    }
    fn share_spreads(old: &[Arc<kition_layout_doc::Spread>], new: &mut [Arc<kition_layout_doc::Spread>]) {
        let by_id: std::collections::HashMap<_, _> = old.iter().map(|spread| (spread.id, spread)).collect();
        for spread in new.iter_mut() {
            let Some(previous) = by_id.get(&spread.id) else { continue };
            if ***previous == **spread {
                *spread = Arc::clone(previous);
            } else {
                share_items(&previous.items, &mut Arc::make_mut(spread).items);
            }
        }
    }
    share_spreads(&old.spreads, &mut new.spreads);
    share_spreads(&old.parents, &mut new.parents);
    for (id, story) in new.stories.iter_mut() {
        let Some(previous) = old.stories.get(id) else { continue };
        carry_revs(previous, Arc::make_mut(story));
        if **previous == **story {
            *story = Arc::clone(previous);
        }
    }
    for (id, asset) in new.assets.iter_mut() {
        if let Some(previous) = old.assets.get(id)
            && **previous == **asset
        {
            *asset = Arc::clone(previous);
        }
    }
    if *old.styles == *new.styles {
        new.styles = Arc::clone(&old.styles);
    }
}

/// Gives `new` (read from the replica) the composition revisions `old` (the previous view) has:
/// `rev` is each view's own cache key, never replicated. A story (or a table cell's text) whose
/// content changed gets the next revision, so no cache keyed on it can answer for the old text.
fn carry_revs(old: &kition_layout_doc::Story, new: &mut kition_layout_doc::Story) {
    fn bump(old: &kition_layout_doc::Story, new: &mut kition_layout_doc::Story) {
        new.rev = old.rev;
        if *old != *new {
            new.rev = old.rev.wrapping_add(1);
        }
    }
    for (id, table) in new.tables.iter_mut() {
        let Some(previous) = old.tables.get(id) else { continue };
        if previous.cells.len() != table.cells.len() {
            continue;
        }
        let table = Arc::make_mut(table);
        for (cell, before) in table.cells.iter_mut().zip(&previous.cells) {
            bump(&before.text, &mut cell.text);
        }
    }
    bump(old, new);
}
