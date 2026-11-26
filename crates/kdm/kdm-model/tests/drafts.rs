//! An agent's draft merged into the layout ([`LayoutSession::merge_changes`], kition-sdk data model
//! 6.6, rules D1–D9): the person edited the layout while the draft was open, the merge keeps both,
//! and the layout's repair runs inside the merge, so it is one undo step of the person and one Undo
//! gives back the exact state before. The draft's preview runs the same repair, so it is what
//! accepting the draft gives.

mod common;

use std::sync::Arc;

use common::*;
use kition_layout_doc::geom::{Rect, shapes};
use kition_layout_doc::vars::{TextVariable, VarKind, var_char, var_index};
use kition_layout_doc::{Document, Item, ItemId, Shape, SpreadRef};
use kition_layout_kdm::LayoutSession;
use kition_layout_kdm::kition_kit::kdm::{Origin, UndoScope};

fn person() -> Origin {
    Origin::person("person:a")
}

fn agent() -> Origin {
    Origin::agent("agent:draft")
}

fn edited(doc: &Arc<Document>, f: impl FnOnce(&mut Document)) -> Arc<Document> {
    let mut d = (**doc).clone();
    f(&mut d);
    d.check().unwrap();
    Arc::new(d)
}

/// The person's session on `doc` and a draft of it (a replica on its line of changes), with the
/// version the draft starts from.
fn with_draft(doc: Document) -> (LayoutSession, LayoutSession, kition_layout_kdm::kition_kit::kdm::Version) {
    let created = LayoutSession::create(&Arc::new(doc), &Origin::system("create")).unwrap();
    let snapshot = created.snapshot().unwrap();
    let person = LayoutSession::open(&snapshot, created.blobs().clone()).unwrap();
    let draft = LayoutSession::open(&snapshot, created.blobs().clone()).unwrap();
    let since = person.version();
    (person, draft, since)
}

/// The variable names the instances in `story`'s text show, in text order.
fn instances(d: &Document, story: kition_layout_doc::StoryId) -> Vec<String> {
    d.story(story).unwrap().text.chars().filter_map(var_index).map(|i| d.text_variables[i].name.clone()).collect()
}

/// The repair reads text inside the merge (V6): the draft retitles the layout and inserts an
/// instance of the text variable "Year" while the person deletes that variable. Merged, the instance
/// names a variable that is gone and the repair deletes it, inside the merge's one step: the
/// title is the draft's, the text is as it was. The preview (`merged_state`, `compare_changes`) runs
/// the same repair, so it equals what the merge gave (the plain merge differs: it keeps the
/// instance); one Undo of the person gives back the exact state before the merge, one Redo the
/// exact repaired state.
#[test]
fn the_repair_of_a_merged_draft_reads_text_inside_the_step_and_the_preview_shows_it() {
    let mut doc = linked();
    doc.text_variables.push(TextVariable::new("Year", VarKind::Custom { text: "2026".into() }));
    doc.check().unwrap();
    let (mut p, mut draft, since) = with_draft(doc);
    let story = linked_story(p.document());
    let text_before = p.document().story(story).unwrap().text.clone();
    let draft_doc = edited(draft.document(), |d| {
        d.title = "From the draft".into();
        let year = d.text_variables.iter().position(|v| v.name == "Year").unwrap();
        let instance = var_char(year).unwrap().to_string();
        let s = d.story_mut(story).unwrap();
        let end = s.len();
        s.insert(end, &instance);
    });
    draft.record(&agent(), &draft_doc).unwrap();
    assert_eq!(instances(draft.document(), story), vec!["Client", "Year"], "the draft holds the instance");
    let p_doc = edited(p.document(), |d| d.text_variables.retain(|v| v.name != "Year"));
    p.record(&person(), &p_doc).unwrap();
    let changes = draft.export_changes(&since).unwrap();

    let preview = p.merged_state(&changes).unwrap();
    assert_ne!(preview, p.replica().merged_state(&changes).unwrap(), "the repair changes this merge");
    let summary = p.compare_changes(&changes).unwrap().to_json();
    let (before, version, depth) = (p.state_hash().unwrap(), p.version(), p.replica().undo_depth(UndoScope::Person));
    let (scope, _) = p.merge_changes(&changes, &agent()).unwrap();
    assert_eq!(scope, Some(UndoScope::Person), "the person's step, under the agent's origin");
    assert_eq!(p.replica().undo_depth(UndoScope::Person), depth + 1, "one step");
    assert_eq!(p.replica().undo_depth(UndoScope::Agent), 0, "none of the agent's");
    assert_eq!(p.replica().state_json().unwrap(), preview, "the preview is what accepting the draft gave");
    assert_eq!(p.replica().diff_since(&version).unwrap().to_json(), summary, "and its comparison what it changed");
    assert_eq!(p.document().title, "From the draft");
    assert_eq!(p.document().story(story).unwrap().text, text_before, "the instance of the deleted variable is gone");
    assert_eq!(instances(p.document(), story), vec!["Client"]);
    assert!(p.document().text_variables.iter().all(|v| v.name != "Year"));
    let merged = p.state_hash().unwrap();
    let version_merged = p.version();
    assert!(p.undo().unwrap().0);
    assert_eq!(p.state_hash().unwrap(), before, "one Undo: the exact state before the merge");
    assert_ne!(p.document().title, "From the draft");
    assert!(p.redo().unwrap().0);
    assert_eq!(p.state_hash().unwrap(), merged, "one Redo: the exact repaired state");
    assert_eq!(p.document().story(story).unwrap().text, text_before);
    assert!(p.version().includes(&version_merged));
}

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

/// Moving or adding a page re-lays the spreads out and keeps their ids, so an agent's draft that
/// moves the last page to the front merges with a page the person added meanwhile: every page and
/// every object once, the draft's move and the person's page both there
/// (issue-concurrent-page-moves: every spread got a new id, and two people adding a page each
/// ended with every page twice). One Undo takes the draft back exactly.
#[test]
fn a_draft_moving_a_page_merges_with_a_page_added_meanwhile() {
    let (mut p, mut draft, since) = with_draft(four_single_pages());
    let pages = page_ids(p.document());
    let draft_doc = edited(draft.document(), |d| d.move_page(3, 0).unwrap());
    draft.record(&agent(), &draft_doc).unwrap();
    assert_eq!(page_ids(draft.document()), vec![pages[3], pages[0], pages[1], pages[2]]);
    let mut added = Vec::new();
    let p_doc = edited(p.document(), |d| added = d.insert_pages(Some(1), 1, None).unwrap());
    p.record(&person(), &p_doc).unwrap();
    assert_eq!(page_ids(p.document()), vec![pages[0], pages[1], added[0].0, pages[2], pages[3]]);
    let before = p.state_hash().unwrap();
    let (scope, _) = p.merge_changes(&draft.export_changes(&since).unwrap(), &agent()).unwrap();
    assert_eq!(scope, Some(UndoScope::Person));
    let d = p.document();
    assert_eq!(page_ids(d), vec![pages[3], pages[0], pages[1], added[0].0, pages[2]], "the draft's move and the person's page, every page once");
    assert_eq!(d.spreads.iter().map(|s| s.items.len()).sum::<usize>(), 4, "every object once");
    d.check().unwrap();
    assert!(p.undo().unwrap().0);
    assert_eq!(p.state_hash().unwrap(), before, "one Undo takes the draft back exactly");
    assert_eq!(page_ids(p.document()), vec![pages[0], pages[1], added[0].0, pages[2], pages[3]]);
}
