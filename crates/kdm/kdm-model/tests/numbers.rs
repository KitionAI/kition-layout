//! Engine ids survive reading (KHP A46, A57; KitModel M8): every object the engine numbers (layers,
//! spreads and parent spreads, pages, stories, assets, items, a story's tables and inline objects)
//! stores its number in the replica (`number`), so a save and reopen, a replica-only open and a
//! reopen after a failed run all answer the ids the session had. An object made without a number
//! (through the object model) gets the next free one, stored by the repair, alike on a replica
//! that takes the same changes. An agent's draft and the person making an object each while the
//! draft is open mint one number: after the merge the object with the smaller object id keeps it,
//! the other gets the next free one, which the repair inside the merge stores.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::*;
use kition_layout_doc::geom::{Rect, shapes};
use kition_layout_doc::{Content, Document, Item, ItemId, ParaFormat, Shape, SpreadRef};
use kition_layout_kdm::kition_kit::kdm::{ActorId, ObjectId, Origin, Replica, Version};
use kition_layout_kdm::kition_kit::session::KitModel;
use kition_layout_kdm::{LayoutModel, LayoutSession, open_file, save_file};
use serde_json::{Value, json};

fn person() -> Origin {
    Origin::person("person:a")
}

fn edited(doc: &Arc<Document>, f: impl FnOnce(&mut Document)) -> Arc<Document> {
    let mut d = (**doc).clone();
    f(&mut d);
    d.check().unwrap();
    Arc::new(d)
}

/// Every engine id of `d`, by where it sits: what an agent may have noted.
fn ids_of(d: &Document) -> Value {
    fn items(list: &[Arc<Item>]) -> Vec<Value> {
        list.iter()
            .map(|item| match &item.content {
                Content::Group { items: kids } => json!({ "group": item.id.0, "children": items(kids) }),
                Content::Graphic(g) => json!({ "item": item.id.0, "asset": g.asset.0 }),
                Content::Text(t) => json!({ "item": item.id.0, "story": t.story.0 }),
                _ => json!(item.id.0),
            })
            .collect()
    }
    let spreads = |list: &[Arc<kition_layout_doc::Spread>]| -> Vec<Value> {
        list.iter()
            .map(|s| {
                json!({
                    "id": s.id.0,
                    "pages": s.pages.iter().map(|p| json!({ "id": p.id.0, "parent": p.parent.map(|x| x.0) })).collect::<Vec<_>>(),
                    "items": items(&s.items),
                })
            })
            .collect()
    };
    json!({
        "layers": d.layers.iter().map(|l| l.id.0).collect::<Vec<_>>(),
        "spreads": spreads(&d.spreads),
        "parents": spreads(&d.parents),
        "stories": d.stories.values().map(|s| json!({
            "id": s.id.0,
            "frames": s.frames.iter().map(|f| f.0).collect::<Vec<_>>(),
            "tables": s.tables.keys().collect::<Vec<_>>(),
            "notes": s.notes.iter().map(|n| n.id).collect::<Vec<_>>(),
            "endnotes": s.endnotes.iter().map(|n| n.id).collect::<Vec<_>>(),
            "anchors": s.anchors.iter().map(|a| a.id).collect::<Vec<_>>(),
            "editorial": s.editorial.iter().map(|n| n.id).collect::<Vec<_>>(),
            "anchored": items(&s.objects.iter().map(|o| Arc::new(o.item.clone())).collect::<Vec<_>>()),
        })).collect::<Vec<_>>(),
        "assets": d.assets.keys().map(|a| a.0).collect::<Vec<_>>(),
    })
}

fn add_rect(d: &mut Document, x: f64) -> ItemId {
    let lid = d.default_layer();
    let id = ItemId(d.alloc());
    let item = Item::new(id, lid, Shape::Rectangle, shapes::rectangle(Rect::new(x, 500.0, x + 40.0, 540.0)));
    d.insert_item(SpreadRef::Doc(0), item, None).unwrap();
    id
}

/// `s` saved and opened again.
fn reopened(s: &LayoutSession) -> LayoutSession {
    let bytes = save_file(s, None).unwrap().bytes;
    open_file(&bytes).unwrap().0
}

/// The number the replica stores for each item object, by engine id.
fn stored_items(s: &LayoutSession) -> BTreeMap<u64, Value> {
    s.model().ids().items.iter().map(|(object, id)| (id.0, s.replica().get(object, "number").unwrap())).collect()
}

#[test]
fn every_kind_keeps_its_id_through_save_and_reopen() {
    for (name, doc) in fixtures() {
        let doc = Arc::new(doc);
        let s = LayoutSession::create(&doc, &Origin::system("create")).unwrap();
        let want = ids_of(&doc);
        assert_eq!(ids_of(s.document()), want, "{name}: the session's view");
        let once = reopened(&s);
        assert_eq!(ids_of(once.document()), want, "{name}: saved and reopened");
        assert_eq!(ids_of(reopened(&once).document()), want, "{name}: saved and reopened twice");
        let replica = LayoutSession::open(&s.export_changes(&Version::empty()).unwrap(), s.blobs().clone()).unwrap();
        assert_eq!(ids_of(replica.document()), want, "{name}: opened from its replica alone");
        let snapshot = LayoutSession::open(&s.snapshot().unwrap(), s.blobs().clone()).unwrap();
        assert_eq!(ids_of(snapshot.document()), want, "{name}: opened from its snapshot");
    }
}

#[test]
fn objects_made_in_a_session_keep_their_ids_through_save_and_reopen() {
    let start = Arc::new(rich());
    let mut s = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    let after = edited(s.document(), |d| {
        add_rect(d, 100.0);
        let lid = d.default_layer();
        d.add_text_frame(SpreadRef::Doc(1), Rect::new(40.0, 300.0, 300.0, 400.0), lid, "A new story", ParaFormat::default()).unwrap();
        let sid = d.stories.keys().next_back().copied().unwrap();
        d.story_mut(sid).unwrap().insert_note(2, "A new footnote", ParaFormat::default());
    });
    s.record(&person(), &after).unwrap();
    let want = ids_of(&after);
    assert_eq!(ids_of(s.document()), want);
    assert_eq!(ids_of(reopened(&s).document()), want);
    // Undoing and redoing the creation brings the objects back under the same ids.
    s.undo().unwrap();
    s.redo().unwrap();
    assert_eq!(ids_of(s.document()), want, "after undo and redo");
}

/// The fixtures written before engine ids were stored hold no numbers: they number in reading order
/// as before (the conformance fixture's overset frame stays item 10, as cases and agents name it),
/// and store those numbers, so the next save keeps them.
#[test]
fn a_file_without_numbers_reads_as_before_and_stores_them() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../conformance/check-overset.klayout")).unwrap();
    let (opened, _) = open_file(&bytes).unwrap();
    let frames: Vec<u64> = opened.document().stories.values().flat_map(|s| s.frames.iter().map(|f| f.0)).collect();
    assert!(frames.contains(&10), "the overset frame is item 10: {frames:?}");
    let stored = stored_items(&opened);
    assert!(stored.iter().all(|(id, number)| number.as_u64() == Some(*id)), "every item stores its id: {stored:?}");
    let want = ids_of(opened.document());
    let (again, _) = open_file(&bytes).unwrap();
    assert_eq!(ids_of(again.document()), want, "a second open numbers the same");
    assert_eq!(ids_of(reopened(&opened).document()), want, "and a save keeps it");
}

/// An object made through the object model (`model.batch`) carries no number: the replica gives
/// it the next free one, deterministically, so a peer that takes the same change numbers it alike.
#[test]
fn an_object_made_without_a_number_gets_the_next_free_one() {
    let start = Arc::new(rich());
    let a0 = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    let snapshot = a0.snapshot().unwrap();
    let mut a = LayoutSession::open(&snapshot, a0.blobs().clone()).unwrap();
    let mut b = LayoutSession::open(&snapshot, a0.blobs().clone()).unwrap();
    let before = a.document().next_id;
    let mut script = Replica::load(LayoutModel::schema(), &a.export_changes(&Version::empty()).unwrap(), ActorId::random()).unwrap();
    let since = script.version();
    let root = ObjectId::from_string("layout");
    let spread = script.list(&root, "spreads").unwrap()[0].clone();
    let existing = script.tree(&root, "items").unwrap()[0].object.clone();
    let path = script.get(&existing, "path").unwrap();
    let made = script
        .transact(&Origin::agent("agent:script"), |tx| {
            let mut fields = serde_json::Map::new();
            fields.insert("shape".into(), json!("rectangle"));
            fields.insert("content".into(), json!("unassigned"));
            fields.insert("xf".into(), json!([1, 0, 0, 1, 0, 0]));
            fields.insert("path".into(), path);
            fields.insert("spread".into(), json!(spread.as_str()));
            let object = tx.create("Item", fields)?;
            tx.tree_create(&root, "items", None, &object)?;
            Ok(object)
        })
        .unwrap();
    let change = script.export_changes(&since).unwrap();
    a.import_changes(&change).unwrap();
    b.import_changes(&a.export_changes(&b.version()).unwrap()).unwrap();
    b.import_changes(&change).unwrap();
    let id = a.model().ids().items.local(&made).unwrap().0;
    assert!(id > before, "a fresh number above every stored one: {id} > {before}");
    assert_eq!(a.replica().get(&made, "number").unwrap(), json!(id), "the repair stores it");
    assert_eq!(b.model().ids().items.local(&made).unwrap().0, id, "a peer taking the same change numbers it alike");
    assert_eq!(ids_of(reopened(&a).document()), ids_of(a.document()));
}

/// The person and an agent's draft each make an item while the draft is open: both mint one
/// number. Merged, the two items have distinct ids by a rule of the state alone: the smaller object
/// id keeps the number, the other gets the next free one, stored inside the merge's one undo step;
/// both survive saving and reopening, the next edit allocates above both, and undoing the merge
/// gives back the exact state before it.
#[test]
fn a_number_minted_by_a_draft_and_meanwhile_ends_distinct() {
    let start = Arc::new(rich());
    let base = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    let snapshot = base.snapshot().unwrap();
    let mut a = LayoutSession::open(&snapshot, base.blobs().clone()).unwrap();
    let mut draft = LayoutSession::open(&snapshot, base.blobs().clone()).unwrap();
    let since = a.version();
    let agent = Origin::agent("agent:draft");
    let mut minted = 0;
    let draft_doc = edited(draft.document(), |d| minted = add_rect(d, 200.0).0);
    draft.record(&agent, &draft_doc).unwrap();
    let a_doc = edited(a.document(), |d| assert_eq!(add_rect(d, 100.0).0, minted, "both mint one number"));
    a.record(&person(), &a_doc).unwrap();
    let mine = a.model().ids().item(ItemId(minted)).unwrap().clone();
    let theirs = draft.model().ids().item(ItemId(minted)).unwrap().clone();
    let before = a.state_hash().unwrap();
    let (scope, _) = a.merge_changes(&draft.export_changes(&since).unwrap(), &agent).unwrap();
    assert_eq!(scope, Some(kition_layout_kdm::kition_kit::kdm::UndoScope::Person));
    let local = |s: &LayoutSession, object: &ObjectId| s.model().ids().items.local(object).unwrap().0;
    let (keeper, other) = if mine < theirs { (&mine, &theirs) } else { (&theirs, &mine) };
    assert_eq!(local(&a, keeper), minted, "the smaller object id keeps the number");
    let next = local(&a, other);
    assert_ne!(next, minted);
    assert_eq!(a.replica().get(keeper, "number").unwrap(), json!(minted));
    assert_eq!(a.replica().get(other, "number").unwrap(), json!(next), "the merge's repair stores the new number");
    // Both survive saving and reopening.
    assert_eq!(ids_of(reopened(&a).document()), ids_of(a.document()));
    assert_eq!(local(&reopened(&a), other), next);
    // The next edit allocates above both.
    let mut added = 0;
    let a_doc = edited(a.document(), |d| added = add_rect(d, 300.0).0);
    assert!(added > next.max(minted), "{added} above {next} and {minted}");
    a.record(&person(), &a_doc).unwrap();
    // Undoing the edit keeps the ids; undoing the merge gives back the exact state before it.
    a.undo().unwrap();
    assert_eq!(local(&a, other), next);
    a.undo().unwrap();
    assert_eq!(a.state_hash().unwrap(), before, "the exact state before the merge");
    assert_eq!(local(&a, &mine), minted);
    assert!(!a.replica().exists(&theirs), "the draft's item went with the merge");
}
