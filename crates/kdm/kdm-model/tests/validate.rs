//! Validation and repair (design note section 2.6): one synthetic state per rule, as a merge can
//! leave it; the repair fixes it, a second repair writes nothing, and the result reads.

mod common;

use std::collections::BTreeSet;

use common::*;
use kition_layout_kdm::kition_kit::kdm::{KdmError, ObjectId, Origin, Replica, Tx};
use kition_layout_kdm::kition_kit::session::{KitModel, MemoryBlobs};
use kition_layout_kdm::{LayoutModel, repair};
use serde_json::{Value, json};

fn root() -> ObjectId {
    ObjectId::from_string("layout")
}

fn damage(replica: &mut Replica, f: impl FnOnce(&mut Tx<'_>) -> Result<(), KdmError>) {
    replica.transact(&Origin::system("merge"), f).unwrap();
}

/// Runs the repair; answers whether it wrote, and checks that a second one does not.
fn repair_once(replica: &mut Replica) -> bool {
    let run = |replica: &mut Replica| {
        replica
            .transact(&Origin::system("repair"), |tx| {
                <LayoutModel as KitModel>::validate(tx, &BTreeSet::new()).map_err(|e| KdmError::Engine(e.to_string()))
            })
            .unwrap()
    };
    let wrote = run(replica);
    assert!(!run(replica), "a second repair writes nothing");
    wrote
}

fn setup() -> (Replica, MemoryBlobs) {
    let (replica, blobs, _) = replica_of(&rich());
    (replica, blobs)
}

fn items_on(replica: &Replica, spread: &ObjectId) -> Vec<ObjectId> {
    replica
        .tree(&root(), "items")
        .unwrap()
        .into_iter()
        .filter(|e| {
            e.parent.is_none() && replica.get(&e.object, "spread").ok().and_then(|v| v.as_str().map(ObjectId::from_string)).as_ref() == Some(spread)
        })
        .map(|e| e.object)
        .collect()
}

#[test]
fn v1_list_entries_naming_nothing_are_dropped() {
    let (mut replica, blobs) = setup();
    let swatch = replica.list(&root(), "swatches").unwrap()[0].clone();
    damage(&mut replica, |tx| tx.delete(&swatch));
    assert!(repair_once(&mut replica));
    assert!(!replica.list(&root(), "swatches").unwrap().contains(&swatch));
    read(&replica, &blobs).check().unwrap();
}

#[test]
fn v2_items_on_a_deleted_spread_are_deleted() {
    let (mut replica, blobs) = setup();
    let spread = replica.list(&root(), "spreads").unwrap()[1].clone();
    let items = items_on(&replica, &spread);
    assert!(!items.is_empty());
    damage(&mut replica, |tx| tx.delete(&spread));
    assert!(repair_once(&mut replica));
    assert!(items.iter().all(|i| !replica.exists(i)));
    read(&replica, &blobs).check().unwrap();
}

#[test]
fn v3_a_page_in_two_spreads_stays_in_the_first_by_id() {
    let (mut replica, blobs) = setup();
    let spreads = replica.list(&root(), "spreads").unwrap();
    let page = replica.list(&spreads[0], "pages").unwrap()[0].clone();
    damage(&mut replica, |tx| tx.list_insert(&spreads[1], "pages", 0, &page));
    assert!(repair_once(&mut replica));
    let holders: Vec<&ObjectId> = spreads.iter().filter(|s| replica.list(s, "pages").unwrap().contains(&page)).collect();
    assert_eq!(holders, vec![spreads.iter().min().unwrap()]);
    read(&replica, &blobs).check().unwrap();
}

#[test]
fn v4_a_frame_threaded_into_two_stories_stays_in_the_first_by_id() {
    let mut doc = rich();
    let lid = doc.default_layer();
    doc.add_text_frame(
        kition_layout_doc::SpreadRef::Doc(1),
        kition_layout_doc::geom::Rect::new(10.0, 10.0, 90.0, 90.0),
        lid,
        "Second",
        Default::default(),
    )
    .unwrap();
    let (mut replica, blobs, _) = replica_of(&doc);
    let stories = replica.list(&root(), "stories").unwrap();
    assert_eq!(stories.len(), 2);
    let with_frames: Vec<ObjectId> = stories.iter().filter(|s| !replica.list(s, "frames").unwrap().is_empty()).cloned().collect();
    let frame = replica.list(&with_frames[0], "frames").unwrap()[0].clone();
    let other = stories.iter().find(|s| **s != with_frames[0]).unwrap().clone();
    damage(&mut replica, |tx| tx.list_insert(&other, "frames", 0, &frame));
    assert!(repair_once(&mut replica));
    let holders: Vec<ObjectId> =
        stories.iter().filter(|s| replica.exists(s) && replica.list(s, "frames").unwrap().contains(&frame)).cloned().collect();
    assert_eq!(holders.len(), 1);
    assert_eq!(&holders[0], [&with_frames[0], &other].into_iter().min().unwrap());
    read(&replica, &blobs).check().unwrap();
}

#[test]
fn v5_a_story_without_frames_is_deleted() {
    let (mut replica, blobs) = setup();
    let story = replica.list(&root(), "stories").unwrap().into_iter().find(|s| !replica.list(s, "frames").unwrap().is_empty()).unwrap();
    let frames = replica.list(&story, "frames").unwrap();
    damage(&mut replica, |tx| {
        for f in &frames {
            tx.delete(f)?;
        }
        Ok(())
    });
    assert!(repair_once(&mut replica));
    assert!(!replica.exists(&story));
    assert!(replica.list(&story, "inlines").is_err() || !replica.exists(&story));
    read(&replica, &blobs).check().unwrap();
}

#[test]
fn v6_unanchored_inlines_and_orphan_anchor_characters() {
    let (mut replica, blobs) = setup();
    let story = replica.list(&root(), "stories").unwrap().into_iter().max_by_key(|s| replica.text(s, "text").unwrap().len()).unwrap();
    let inlines = replica.list(&story, "inlines").unwrap();
    let before_len = replica.text(&story, "text").unwrap().chars().count();
    // An inline object no character marks, and a character whose inline object is gone.
    let footnote = inlines.iter().find(|i| replica.get(i, "kind").unwrap() == json!("footnote")).unwrap().clone();
    damage(&mut replica, |tx| {
        let stray = tx.create("Inline", serde_json::from_value(json!({ "kind": "textAnchor", "data": { "name": "stray" } })).unwrap())?;
        tx.list_insert(&story, "inlines", 0, &stray)?;
        tx.delete(&footnote)
    });
    let found = repair::read_repairs(&replica).unwrap();
    assert_eq!(found.inlines.len(), 1, "{found:?}");
    assert_eq!(found.chars.len(), 1, "{found:?}");
    let wrote = replica.transact(&Origin::system("repair"), |tx| repair::apply(tx, &found)).unwrap();
    assert!(wrote);
    assert!(repair::read_repairs(&replica).unwrap().is_empty());
    assert_eq!(replica.text(&story, "text").unwrap().chars().count(), before_len - 1);
    let doc = read(&replica, &blobs);
    doc.check().unwrap();
    assert!(doc.stories[&main_story(&doc)].notes.is_empty());
}

#[test]
fn v7_references_to_deleted_layers_swatches_and_styles() {
    let (mut replica, blobs) = setup();
    let layers = replica.list(&root(), "layers").unwrap();
    let doomed_layer = layers
        .iter()
        .find(|l| replica.objects_of_type("Item").iter().any(|i| replica.get(i, "layer").unwrap() == json!(l.as_str())))
        .unwrap()
        .clone();
    let swatch = replica.list(&root(), "swatches").unwrap().into_iter().find(|s| replica.get(s, "name").unwrap() == json!("Brand 30")).unwrap();
    damage(&mut replica, |tx| {
        tx.delete(&doomed_layer)?;
        tx.delete(&swatch)
    });
    assert!(repair_once(&mut replica));
    let first = replica.list(&root(), "layers").unwrap()[0].clone();
    for item in replica.objects_of_type("Item") {
        let layer = replica.get(&item, "layer").unwrap();
        assert!(layer.is_null() || layer == json!(first.as_str()), "{layer}");
        let fill = replica.get(&item, "fill").unwrap();
        assert!(!fill.to_string().contains(swatch.as_str()), "the swatch reference went: {fill}");
    }
    let doc = read(&replica, &blobs);
    doc.check().unwrap();
    assert!(doc.spreads.iter().flat_map(|s| s.items.iter()).any(|i| i.fill.swatch == "Brand 30"), "the name stays");
}

#[test]
fn v8_a_duplicate_key_is_cleared_on_the_later_item() {
    let (mut replica, _) = setup();
    let mut items = replica.objects_of_type("Item");
    items.sort();
    damage(&mut replica, |tx| {
        tx.set(&items[0], "key", json!("hero"))?;
        tx.set(&items[1], "key", json!("hero"))
    });
    assert!(repair_once(&mut replica));
    assert_eq!(replica.get(&items[0], "key").unwrap(), json!("hero"));
    assert_eq!(replica.get(&items[1], "key").unwrap(), Value::Null);
}

#[test]
fn v9_an_empty_group_is_deleted() {
    let (mut replica, blobs) = setup();
    let group = replica.objects_of_type("Item").into_iter().find(|i| replica.get(i, "content").unwrap() == json!("group")).unwrap();
    let tree = replica.tree(&root(), "items").unwrap();
    let node = tree.iter().find(|e| e.object == group).unwrap().node.clone();
    let children: Vec<ObjectId> = tree.iter().filter(|e| e.parent.as_ref() == Some(&node)).map(|e| e.object.clone()).collect();
    assert_eq!(children.len(), 2);
    damage(&mut replica, |tx| {
        for c in &children {
            tx.delete(c)?;
        }
        Ok(())
    });
    assert!(repair_once(&mut replica));
    assert!(!replica.exists(&group));
    read(&replica, &blobs).check().unwrap();
}

#[test]
fn v10_a_section_on_a_deleted_page_is_deleted() {
    let (mut replica, blobs) = setup();
    let section = replica.list(&root(), "sections").unwrap()[0].clone();
    let page = ObjectId::from_string(replica.get(&section, "start").unwrap().as_str().unwrap());
    let spread = replica.list(&root(), "spreads").unwrap().into_iter().find(|s| replica.list(s, "pages").unwrap().contains(&page)).unwrap();
    let index = replica.list(&spread, "pages").unwrap().iter().position(|p| *p == page).unwrap();
    damage(&mut replica, |tx| {
        tx.list_remove(&spread, "pages", index)?;
        tx.delete(&page)
    });
    assert!(repair_once(&mut replica));
    assert!(!replica.exists(&section));
    read(&replica, &blobs).check().unwrap();
}

#[test]
fn v11_a_story_text_without_its_final_break_is_refused() {
    let (mut replica, blobs) = setup();
    let story = replica.list(&root(), "stories").unwrap()[0].clone();
    let len = replica.text(&story, "text").unwrap().chars().count();
    damage(&mut replica, |tx| tx.text_delete(&story, "text", len - 1, 1));
    let error = kition_layout_kdm::read_document(&replica, &blobs).err().unwrap();
    assert!(error.to_string().contains("damaged"), "{error}");
    // The repair does not append one (two replicas appending would give two).
    repair_once(&mut replica);
    assert_eq!(replica.text(&story, "text").unwrap().chars().count(), len - 1);
}

#[test]
fn v12_an_override_of_an_item_not_on_a_parent_is_cleared() {
    let (mut replica, _) = setup();
    let spread = replica.list(&root(), "spreads").unwrap()[0].clone();
    let on_spread = items_on(&replica, &spread);
    let (a, b) = (on_spread[0].clone(), on_spread[1].clone());
    let parent = replica.list(&root(), "parents").unwrap().last().unwrap().clone();
    let on_parent = items_on(&replica, &parent)[0].clone();
    damage(&mut replica, |tx| {
        tx.set(&a, "overrides", json!(b.as_str()))?;
        tx.set(&b, "overrides", json!(on_parent.as_str()))
    });
    assert!(repair_once(&mut replica));
    assert_eq!(replica.get(&a, "overrides").unwrap(), Value::Null, "not a parent item");
    assert_eq!(replica.get(&b, "overrides").unwrap(), json!(on_parent.as_str()), "a parent item stays");
}

#[test]
fn v13_hyperlinks_and_placeholders_on_deleted_items_and_pages_are_deleted() {
    let (mut replica, blobs, _) = replica_of(&linked());
    let links = replica.list(&root(), "hyperlinks").unwrap();
    let on_item = links.iter().find(|l| !replica.get(l, "item").unwrap().is_null()).unwrap().clone();
    let oval = ObjectId::from_string(replica.get(&on_item, "item").unwrap().as_str().unwrap());
    let placeholders = replica.list(&root(), "placeholders").unwrap();
    let on_picture = placeholders.iter().find(|p| !replica.get(p, "item").unwrap().is_null()).unwrap().clone();
    let picture = ObjectId::from_string(replica.get(&on_picture, "item").unwrap().as_str().unwrap());
    damage(&mut replica, |tx| {
        tx.delete(&oval)?;
        tx.delete(&picture)
    });
    assert!(repair_once(&mut replica));
    assert!(!replica.exists(&on_item) && !replica.exists(&on_picture));
    assert_eq!(replica.list(&root(), "hyperlinks").unwrap().len(), links.len() - 1);
    assert_eq!(replica.list(&root(), "placeholders").unwrap().len(), placeholders.len() - 1);
    read(&replica, &blobs).check().unwrap();
}

/// The read repair: a hyperlink or placeholder whose text is gone (one person deleted the text
/// while another edited the hyperlink), and a variable instance naming a deleted variable.
#[test]
fn v13_sources_whose_text_is_gone_and_instances_of_deleted_variables() {
    let (mut replica, blobs, _) = replica_of(&linked());
    let story = replica.list(&root(), "stories").unwrap().into_iter().find(|s| replica.text(s, "text").unwrap().starts_with("Visit us")).unwrap();
    let links = replica.list(&root(), "hyperlinks").unwrap();
    let text_link = links.iter().find(|l| replica.get(l, "item").unwrap().is_null()).unwrap().clone();
    let variable = replica.list(&root(), "textVariables").unwrap().last().unwrap().clone();
    let before_len = replica.text(&story, "text").unwrap().chars().count();
    // "Visit us" (8 characters) goes, and so does the variable its instance names.
    damage(&mut replica, |tx| {
        tx.text_delete(&story, "text", 0, 8)?;
        tx.delete(&variable)
    });
    let found = repair::read_repairs(&replica).unwrap();
    assert_eq!(found.unanchored, vec![("hyperlinks", text_link.clone())], "{found:?}");
    assert_eq!(found.chars.len(), 1, "the instance: {found:?}");
    let wrote = replica.transact(&Origin::system("repair"), |tx| repair::apply(tx, &found)).unwrap();
    assert!(wrote);
    assert!(repair::read_repairs(&replica).unwrap().is_empty());
    assert!(!replica.exists(&text_link));
    assert_eq!(replica.text(&story, "text").unwrap().chars().count(), before_len - 8 - 1);
    let doc = read(&replica, &blobs);
    doc.check().unwrap();
    assert!(doc.stories.values().all(|s| s.text.chars().all(|c| kition_layout_doc::vars::var_index(c).is_none())));
}
