//! Two people editing one document at once: each edits their own replica through the bridge, the
//! replicas exchange changes, repair (validation, section 2.6) and converge to one state.

mod common;

use std::sync::Arc;

use common::*;
use kition_layout_doc::geom::{Rect, shapes};
use kition_layout_doc::vars::{TextVariable, VarKind, var_char, var_index};
use kition_layout_doc::{
    CharAttrs, Condition, Content, Document, Hyperlink, HyperlinkDest, HyperlinkSource, Item, ItemId, ParaFormat, Placeholder, PlaceholderAnchor,
    PlaceholderRole, Shape, SpreadRef, XrefFormat,
};
use kition_layout_kdm::kition_kit::kdm::Origin;
use kition_layout_kdm::kition_kit::session::{KitSession, MemoryBlobs};
use kition_layout_kdm::{LayoutModel, compare::normalize, write_changes};

struct Peer {
    session: KitSession<LayoutModel>,
}

impl Peer {
    fn doc(&self) -> Document {
        (**self.session.model().document()).clone()
    }

    /// One edit: `f` changes the view's document, the difference is recorded as a person's
    /// transaction, and the view adopts the result.
    fn edit(&mut self, f: impl FnOnce(&mut Document)) {
        let before = self.session.model().document().clone();
        let mut after = (*before).clone();
        f(&mut after);
        after.check().unwrap();
        let mut ids = self.session.model().ids().clone();
        let mut blobs = MemoryBlobs::default();
        self.session.record(&Origin::person("p"), |tx| Ok(write_changes(tx, &before, &after, &mut ids, &mut blobs)?)).unwrap();
        self.session.model_mut().adopt(Arc::new(after), ids);
    }
}

fn pair(doc: &Document) -> (Peer, Peer) {
    let (replica, blobs, _) = replica_of(doc);
    let a = KitSession::<LayoutModel>::from_replica(replica, Box::new(blobs.clone())).unwrap();
    let b = a.fork(actor(2), Box::new(blobs)).unwrap();
    (Peer { session: a }, Peer { session: b })
}

/// Exchanges changes both ways and checks the two replicas converged.
fn sync(a: &mut Peer, b: &mut Peer) {
    let (va, vb) = (a.session.version(), b.session.version());
    let from_a = a.session.export_changes(&vb).unwrap();
    let from_b = b.session.export_changes(&va).unwrap();
    a.session.import_changes(&from_b).unwrap();
    b.session.import_changes(&from_a).unwrap();
    // A repair on one side reaches the other too.
    let (va, vb) = (a.session.version(), b.session.version());
    let from_a = a.session.export_changes(&vb).unwrap();
    let from_b = b.session.export_changes(&va).unwrap();
    a.session.import_changes(&from_b).unwrap();
    b.session.import_changes(&from_a).unwrap();
    assert_eq!(a.session.state_hash().unwrap(), b.session.state_hash().unwrap(), "the replicas converge");
    assert_eq!(normalize(&a.doc()), normalize(&b.doc()), "and so do their views");
    a.doc().check().unwrap();
}

fn main_story_mut(d: &mut Document) -> &mut kition_layout_doc::Story {
    let sid = main_story(d);
    d.story_mut(sid).unwrap()
}

#[test]
fn concurrent_typing_in_one_story_keeps_both() {
    let (mut a, mut b) = pair(&rich());
    a.edit(|d| main_story_mut(d).insert(0, "[A]"));
    b.edit(|d| {
        let s = main_story_mut(d);
        let end = s.len();
        s.insert(end, "[B]");
    });
    sync(&mut a, &mut b);
    let text = a.doc().stories[&main_story(&a.doc())].text.clone();
    assert!(text.starts_with("[A]") && text.ends_with("[B]"), "{text}");
}

#[test]
fn concurrent_formatting_of_overlapping_ranges_both_applies() {
    let (mut a, mut b) = pair(&rich());
    a.edit(|d| main_story_mut(d).format_chars(6..12, |f| f.over.font_style = Some("Bold".into())));
    b.edit(|d| main_story_mut(d).format_chars(9..16, |f| f.over.underline = Some(true)));
    sync(&mut a, &mut b);
    let d = a.doc();
    let s = &d.stories[&main_story(&d)];
    let at = |byte: usize| s.char_format_at(byte).over.clone();
    let both: CharAttrs = at(10);
    assert_eq!((both.font_style.as_deref(), both.underline), (Some("Bold"), Some(true)), "the overlap has both");
    assert_eq!(at(7).underline, None);
    assert_eq!(at(14).font_style, None);
}

#[test]
fn a_split_and_a_restyle_of_one_paragraph() {
    // A splits the first paragraph; B gives it another style. The first half keeps the format the
    // split copied, the second (ending at the old break) gets B's style (design note 2.4).
    let mut doc = rich();
    Arc::make_mut(&mut doc.styles).paragraph.push(kition_layout_doc::ParagraphStyle {
        name: "Heading".into(),
        based_on: None,
        next_style: None,
        para: Default::default(),
        chars: Default::default(),
        shortcut: String::new(),
    });
    let (mut a, mut b) = pair(&doc);
    a.edit(|d| main_story_mut(d).insert(5, "\n"));
    b.edit(|d| main_story_mut(d).format_paras(0..1, |p| p.style = "Heading".into()));
    sync(&mut a, &mut b);
    let d = a.doc();
    let s = &d.stories[&main_story(&d)];
    assert_ne!(s.paras[0].style, "Heading", "the first half keeps the copied format");
    assert_eq!(s.paras[1].style, "Heading", "the second half ends at the restyled break");
}

#[test]
fn two_people_typing_into_one_empty_cell_make_one_cell() {
    let (mut a, mut b) = pair(&rich());
    let cell_edit = |text: &'static str| {
        move |d: &mut Document| {
            let s = main_story_mut(d);
            let t = Arc::make_mut(s.tables.values_mut().next().unwrap());
            t.cell_mut(0, 2).unwrap().text.insert(0, text);
        }
    };
    a.edit(cell_edit("left"));
    b.edit(cell_edit("right"));
    sync(&mut a, &mut b);
    let d = a.doc();
    let s = &d.stories[&main_story(&d)];
    let cell = s.tables.values().next().unwrap().cell(0, 2).unwrap().text.text.clone();
    assert!(cell.contains("left") && cell.contains("right"), "both texts are kept in one cell: {cell}");
    let table = s.tables.values().next().unwrap();
    assert_eq!(a.session.replica().objects_of_type("Cell").len(), table.nrows() * table.ncols(), "one object per cell, empty ones included");
}

#[test]
fn two_people_moving_one_item_leave_it_in_one_place() {
    let (mut a, mut b) = pair(&rich());
    let oval = |d: &Document| d.spreads[1].items.iter().find(|i| i.shape == Shape::Oval).unwrap().id;
    a.edit(|d| {
        let id = oval(d);
        let item = d.remove_item(id).unwrap();
        d.insert_item(SpreadRef::Doc(0), Arc::unwrap_or_clone(item), None).unwrap();
    });
    b.edit(|d| {
        let id = oval(d);
        let item = d.remove_item(id).unwrap();
        d.insert_item(SpreadRef::Doc(1), Arc::unwrap_or_clone(item), Some(0)).unwrap();
    });
    sync(&mut a, &mut b);
    let d = a.doc();
    let ovals = d.spreads.iter().flat_map(|s| s.items.iter()).filter(|i| i.shape == Shape::Oval).count();
    assert_eq!(ovals, 1);
}

#[test]
fn delete_wins_over_an_edit_inside() {
    let (mut a, mut b) = pair(&rich());
    // A deletes the story (all its frames); B types into it.
    a.edit(|d| {
        let sid = main_story(d);
        for frame in d.stories[&sid].frames.clone() {
            d.remove_item(frame).unwrap();
        }
    });
    b.edit(|d| main_story_mut(d).insert(0, "late"));
    sync(&mut a, &mut b);
    let d = a.doc();
    assert!(d.stories.values().all(|s| !s.text.contains("late")), "the deleted story stays deleted");
}

#[test]
fn concurrent_groups_of_one_item_keep_it_once() {
    let (mut a, mut b) = pair(&rich());
    let group_oval = |d: &mut Document| {
        let lid = d.default_layer();
        let id = d.spreads[1].items.iter().find(|i| i.shape == Shape::Oval).unwrap().id;
        let oval = d.remove_item(id).unwrap();
        let mut g = Item::new(ItemId(d.alloc()), lid, Shape::Group, shapes::rectangle(Rect::new(0.0, 0.0, 1.0, 1.0)));
        g.content = Content::Group { items: vec![oval] };
        d.insert_item(SpreadRef::Doc(1), g, None).unwrap();
    };
    a.edit(group_oval);
    b.edit(group_oval);
    sync(&mut a, &mut b);
    let d = a.doc();
    let mut ovals = 0;
    let mut empty_groups = 0;
    for spread in &d.spreads {
        for item in &spread.items {
            match &item.content {
                Content::Group { items } if items.is_empty() => empty_groups += 1,
                Content::Group { items } => ovals += items.iter().filter(|i| i.shape == Shape::Oval).count(),
                _ => ovals += usize::from(item.shape == Shape::Oval),
            }
        }
    }
    assert_eq!(ovals, 1, "one oval");
    assert_eq!(empty_groups, 0, "the group left empty is deleted (V9)");
}

#[test]
fn a_new_story_from_each_side_is_kept() {
    let (mut a, mut b) = pair(&blank());
    for (peer, text) in [(&mut a, "from A"), (&mut b, "from B")] {
        peer.edit(|d| {
            let lid = d.default_layer();
            d.add_text_frame(SpreadRef::Doc(0), Rect::new(10.0, 10.0, 90.0, 90.0), lid, text, ParaFormat::default()).unwrap();
        });
    }
    sync(&mut a, &mut b);
    let texts: Vec<String> = a.doc().stories.values().map(|s| s.text.clone()).collect();
    assert!(texts.contains(&"from A".to_owned()) && texts.contains(&"from B".to_owned()), "{texts:?}");
}

/// A small deterministic generator (no extra dependency).
struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

/// One random edit of the kinds a person makes.
/// A four-page layout of single-page spreads, each page holding one rectangle.
fn four_single_pages() -> Document {
    let mut d = Document::new(&kition_layout_doc::build::NewDocument { pages: 4, facing_pages: false, ..Default::default() });
    for (index, spread) in d.spreads.clone().iter().enumerate() {
        let x = spread.pages[0].x;
        let lid = d.layers[0].id;
        let item = Item::new(ItemId(d.alloc()), lid, Shape::Rectangle, shapes::rectangle(Rect::new(x + 10.0, 10.0 * index as f64, x + 60.0, 50.0)));
        d.insert_item(SpreadRef::Doc(index), item, None).unwrap();
    }
    d
}

fn page_ids(d: &Document) -> Vec<u64> {
    d.spreads.iter().flat_map(|s| s.pages.iter().map(|p| p.id.0)).collect()
}

/// Moving a page re-lays the spreads out; it must move the spreads it keeps, not make new ones,
/// so two people moving pages at once keep every page once (issue-concurrent-page-moves).
#[test]
fn concurrent_page_moves_keep_every_page_once() {
    let (mut a, mut b) = pair(&four_single_pages());
    let pages = page_ids(&a.doc());
    a.edit(|d| d.move_page(0, 3).unwrap());
    b.edit(|d| d.move_page(3, 0).unwrap());
    sync(&mut a, &mut b);
    let d = a.doc();
    let mut after = page_ids(&d);
    assert_eq!(after.len(), 4, "every page once: {after:?}");
    after.sort();
    let mut before = pages;
    before.sort();
    assert_eq!(after, before);
    assert_eq!(d.spreads.iter().map(|s| s.items.len()).sum::<usize>(), 4, "every object once");
}

/// Two people adding a page each at once make a layout of both new pages and the old ones, each
/// once (issue-concurrent-page-moves).
#[test]
fn concurrent_page_inserts_add_both_pages_and_nothing_else() {
    let (mut a, mut b) = pair(&four_single_pages());
    a.edit(|d| {
        d.insert_pages(Some(0), 1, None).unwrap();
    });
    b.edit(|d| {
        d.insert_pages(Some(3), 1, None).unwrap();
    });
    sync(&mut a, &mut b);
    let d = a.doc();
    assert_eq!(page_ids(&d).len(), 6, "four pages and the two new ones");
    assert_eq!(d.spreads.iter().map(|s| s.items.len()).sum::<usize>(), 4, "every object once");
}

/// The same on facing pages, where adding a page moves pages between spreads.
#[test]
fn concurrent_page_inserts_on_facing_pages_add_both_pages_and_nothing_else() {
    let (mut a, mut b) = pair(&rich());
    let pages = page_ids(&a.doc()).len();
    let items: usize = a.doc().spreads.iter().map(|s| s.items.len()).sum();
    a.edit(|d| {
        d.insert_pages(Some(0), 1, None).unwrap();
    });
    b.edit(|d| {
        d.insert_pages(Some(1), 1, None).unwrap();
    });
    sync(&mut a, &mut b);
    let d = a.doc();
    assert_eq!(page_ids(&d).len(), pages + 2, "the pages and the two new ones");
    assert_eq!(d.spreads.iter().map(|s| s.items.len()).sum::<usize>(), items, "every object once");
}

fn random_edit(rng: &mut Rng, d: &mut Document) {
    let char_at = |s: &kition_layout_doc::Story, k: usize| s.text.char_indices().nth(k).map_or(s.text.len(), |(i, _)| i);
    match rng.next(8) {
        0 | 1 => {
            let s = main_story_mut(d);
            let n = s.text.chars().count();
            let at = char_at(s, rng.next(n + 1));
            s.insert(at, ["x", "yz", "\n", "é"][rng.next(4)]);
        }
        2 => {
            let s = main_story_mut(d);
            let n = s.text.chars().count();
            if n > 2 {
                let k = rng.next(n - 1);
                let (a, b) = (char_at(s, k), char_at(s, k + 1));
                if !s.text[a..b].chars().any(|c| ('\u{E000}'..='\u{F8FF}').contains(&c)) {
                    s.delete(a..b);
                }
            }
        }
        3 => {
            let s = main_story_mut(d);
            let n = s.text.chars().count();
            let (a, b) = (char_at(s, rng.next(n)), char_at(s, n));
            let bold = rng.next(2) == 0;
            s.format_chars(a..b, |f| f.over.font_style = Some(if bold { "Bold" } else { "Italic" }.into()));
        }
        4 => {
            // A move to another spread (or to the front of its own).
            let from = rng.next(d.spreads.len());
            let n = d.spreads[from].items.len();
            if n > 0 {
                let k = rng.next(n);
                let item = Arc::make_mut(&mut d.spreads[from]).items.remove(k);
                let to = rng.next(d.spreads.len());
                Arc::make_mut(&mut d.spreads[to]).items.push(item);
            }
        }
        5 => {
            let s = main_story_mut(d);
            if let Some(t) = s.tables.values_mut().next() {
                let t = Arc::make_mut(t);
                let (r, c) = (rng.next(t.nrows()), rng.next(t.ncols()));
                if let Some(cell) = t.cell_mut(r, c) {
                    cell.text.insert(0, "c");
                }
            }
        }
        6 => {
            let lid = d.default_layer();
            let id = ItemId(d.alloc());
            let shape = Item::new(id, lid, Shape::Oval, shapes::ellipse(Rect::new(1.0, 1.0, 9.0, 9.0)));
            let spread = rng.next(d.spreads.len());
            d.insert_item(SpreadRef::Doc(spread), shape, None).unwrap();
        }
        _ => {
            let ovals: Vec<ItemId> = d.spreads.iter().flat_map(|s| s.items.iter()).filter(|i| i.shape == Shape::Oval).map(|i| i.id).collect();
            if let Some(id) = ovals.get(rng.next(ovals.len())) {
                d.remove_item(*id).unwrap();
            }
        }
    }
}

#[test]
fn random_concurrent_edits_converge() {
    for seed in [7u64, 11, 23, 42] {
        let (mut a, mut b) = pair(&rich());
        let mut rng = Rng(seed);
        for round in 0..12 {
            for _ in 0..3 {
                a.edit(|d| random_edit(&mut rng, d));
                b.edit(|d| random_edit(&mut rng, d));
            }
            if round % 3 == 2 {
                sync(&mut a, &mut b);
            }
        }
        sync(&mut a, &mut b);
    }
}

/// What the engine does to the hyperlink sources and placeholders of `story` after `by` bytes were
/// typed at `at`: those after it move.
fn shift_ranges(d: &mut Document, story: kition_layout_doc::StoryId, at: usize, by: usize) {
    for link in &mut d.hyperlinks {
        if let HyperlinkSource::Text { story: s, start, end } = &mut link.source
            && *s == story
            && *start >= at
        {
            *start += by;
            *end += by;
        }
    }
    for p in &mut d.data_merge.placeholders {
        if let PlaceholderAnchor::Text { story: s, start, end } = &mut p.anchor
            && *s == story
            && *start >= at
        {
            *start += by;
            *end += by;
        }
    }
}

/// The text of `link`'s source in `d`.
fn source_text(d: &Document, name: &str) -> String {
    let link = d.hyperlinks.iter().find(|h| h.name == name).unwrap_or_else(|| panic!("no hyperlink {name}: {:?}", d.hyperlinks));
    let HyperlinkSource::Text { story, start, end } = link.source else { panic!("{name} starts from an item") };
    d.story(story).unwrap().text[start..end].to_owned()
}

/// Two people adding a hyperlink each at once keep both, and text one types before the other's
/// source moves it with its text: hyperlinks are objects, their sources marks on the text
/// (schema-freeze).
#[test]
fn concurrent_hyperlinks_keep_both_and_follow_their_text() {
    let (mut a, mut b) = pair(&linked());
    a.edit(|d| {
        let story = linked_story(d);
        let id = d.alloc();
        let source = HyperlinkSource::Text { story, start: 13, end: 14 };
        d.hyperlinks.push(Hyperlink { id, name: "Shout".into(), source, dest: HyperlinkDest::Url("https://a.example".into()) });
    });
    b.edit(|d| {
        let story = linked_story(d);
        d.story_mut(story).unwrap().insert(0, "Please ");
        shift_ranges(d, story, 0, 7);
        let oval = d.spreads.iter().flat_map(|s| s.items.iter()).find(|i| i.shape == Shape::Oval).map(|i| i.id).unwrap();
        let id = d.alloc();
        d.hyperlinks.push(Hyperlink { id, name: "Oval".into(), source: HyperlinkSource::Item { id: oval }, dest: HyperlinkDest::Page(0) });
    });
    sync(&mut a, &mut b);
    let d = a.doc();
    let mut names: Vec<&str> = d.hyperlinks.iter().map(|h| h.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["Oval", "Page 3", "Shout", "Site"], "both new hyperlinks and the old ones");
    assert_eq!(source_text(&d, "Site"), "Visit us");
    assert_eq!(source_text(&d, "Shout"), "!", "A's source moved with the text B typed before it");
    let story = &d.stories[&linked_story(&d)];
    let PlaceholderAnchor::Text { start, end, .. } = d.data_merge.placeholders[0].anchor else { panic!("a text placeholder") };
    assert_eq!(&story.text[start..end], "<<Name>>");
}

/// Two people adding a data merge placeholder each at once keep both.
#[test]
fn concurrent_placeholders_keep_both() {
    let (mut a, mut b) = pair(&linked());
    a.edit(|d| {
        let oval = d.spreads.iter().flat_map(|s| s.items.iter()).find(|i| i.shape == Shape::Oval).map(|i| i.id).unwrap();
        let id = d.alloc();
        let anchor = PlaceholderAnchor::Item { id: oval };
        d.data_merge.placeholders.push(Placeholder { id, source_id: 0, field: "Code".into(), role: PlaceholderRole::Qr, anchor });
    });
    b.edit(|d| {
        let story = linked_story(d);
        let id = d.alloc();
        let anchor = PlaceholderAnchor::Text { story, start: 0, end: 5 };
        d.data_merge.placeholders.push(Placeholder { id, source_id: 0, field: "Link".into(), role: PlaceholderRole::Hyperlink, anchor });
    });
    sync(&mut a, &mut b);
    let d = a.doc();
    let mut fields: Vec<&str> = d.data_merge.placeholders.iter().map(|p| p.field.as_str()).collect();
    fields.sort_unstable();
    assert_eq!(fields, vec!["Code", "Link", "Name", "Photo"]);
    let link = d.data_merge.placeholders.iter().find(|p| p.field == "Link").unwrap();
    let PlaceholderAnchor::Text { story, start, end } = link.anchor else { panic!("a text placeholder") };
    assert_eq!(&d.stories[&story].text[start..end], "Visit");
}

/// Two people defining a condition, a text variable (and inserting an instance of it) and a
/// cross-reference format each at once keep all of them, and every instance shows its own
/// variable, whatever place the merged list gives it.
#[test]
fn concurrent_named_definitions_keep_both_and_instances_their_variable() {
    let (mut a, mut b) = pair(&linked());
    for (peer, (condition, variable, format, at_end)) in [(&mut a, ("Review", "Year", "Title", true)), (&mut b, ("Final", "City", "Page", false))] {
        peer.edit(|d| {
            d.conditions.push(Condition { name: condition.into(), color: [0, 0, 255], visible: true });
            d.xref_formats.push(XrefFormat { name: format.into(), definition: "<pageNum />".into() });
            d.text_variables.push(TextVariable::new(variable, VarKind::Custom { text: variable.to_uppercase() }));
            let instance = var_char(d.text_variables.len() - 1).unwrap().to_string();
            let story = linked_story(d);
            let s = d.story_mut(story).unwrap();
            let at = if at_end { s.len() } else { 0 };
            s.insert(at, &instance);
            shift_ranges(d, story, at, instance.len());
        });
    }
    sync(&mut a, &mut b);
    let d = a.doc();
    let names = |list: Vec<&str>| {
        let mut list = list;
        list.sort_unstable();
        list
    };
    assert_eq!(names(d.conditions.iter().map(|c| c.name.as_str()).collect()), vec!["Draft", "Final", "Review"]);
    assert!(d.xref_formats.iter().any(|f| f.name == "Title") && d.xref_formats.iter().any(|f| f.name == "Page"));
    let story = &d.stories[&linked_story(&d)];
    let shown: Vec<&str> = story.text.chars().filter_map(var_index).map(|i| d.text_variables[i].name.as_str()).collect();
    assert_eq!(shown, vec!["City", "Client", "Year"], "each instance names its own variable");
}

/// A bookmark names its page: when another person adds pages before it, it still goes there.
#[test]
fn a_bookmark_follows_its_page_when_another_person_adds_pages() {
    let (mut a, mut b) = pair(&linked());
    let marked = page_ids(&a.doc())[1];
    b.edit(|d| {
        d.insert_pages(None, 2, None).unwrap();
        // The engine's own page indexes after the insertion (it keeps absolute indexes).
        for bookmark in &mut d.bookmarks {
            bookmark.page += 2;
            for child in &mut bookmark.children {
                child.page += 2;
            }
        }
        if let HyperlinkDest::Page(page) = &mut d.hyperlinks[1].dest {
            *page += 2;
        }
        for section in &mut d.sections {
            section.start += 2;
        }
    });
    a.edit(|d| d.bookmarks[0].name = "Opening".into());
    sync(&mut a, &mut b);
    let d = a.doc();
    assert_eq!(d.bookmarks[0].name, "Opening");
    assert_eq!(page_ids(&d)[d.bookmarks[0].page], marked, "the bookmark still goes to its page");
}
