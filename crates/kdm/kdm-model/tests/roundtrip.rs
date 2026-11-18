//! A document written to a replica reads back with the same content (ids aside), for every
//! fixture: the sample, a blank document, a rich one, and the rich one through IDML.

mod common;

use common::*;
use kition_layout_kdm::compare::normalize;
use kition_layout_kdm::kition_kit::kdm::Schema;

#[test]
fn the_schema_is_valid() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/model/schema.json")).unwrap();
    let schema = Schema::parse(&text).unwrap();
    assert_eq!(schema.kit, "kit.layout");
}

#[test]
fn every_fixture_reads_back_the_same() {
    for (name, doc) in fixtures() {
        let (replica, blobs, _) = replica_of(&doc);
        let back = read(&replica, &blobs);
        back.check().unwrap_or_else(|e| panic!("{name}: the read document is inconsistent: {e:?}"));
        assert_same(name, &doc, &back);
        // And again: a second write of what was read reads the same.
        let (again, again_blobs, _) = replica_of(&back);
        assert_same(name, &back, &read(&again, &again_blobs));
    }
}

#[test]
fn a_read_document_saves_and_loads_as_klayout() {
    let doc = rich();
    let (replica, blobs, _) = replica_of(&doc);
    let back = read(&replica, &blobs);
    let bytes = kition_layout_format::save(&back).unwrap();
    let loaded = kition_layout_format::load(&bytes).unwrap();
    assert_eq!(normalize(&loaded), normalize(&back));
}

#[test]
fn ids_round_trip_through_known() {
    let doc = rich();
    let (replica, blobs, ids) = replica_of(&doc);
    let mut known = kition_layout_kdm::Known::default();
    known.remember(&doc);
    let (back, back_ids) = read_known(&replica, &blobs, &known);
    // Every item, story, spread, page and layer keeps the local id the writing view gave it.
    assert_eq!(serde_json::to_value(&doc.spreads).unwrap(), serde_json::to_value(&back.spreads).unwrap());
    assert_eq!(doc.stories.keys().collect::<Vec<_>>(), back.stories.keys().collect::<Vec<_>>());
    assert_eq!(doc.layers.iter().map(|l| l.id).collect::<Vec<_>>(), back.layers.iter().map(|l| l.id).collect::<Vec<_>>());
    assert_eq!(back_ids.items.len(), ids.items.len());
    assert!(back.next_id >= doc.next_id.min(back.next_id));
}

#[test]
fn the_rich_fixture_keeps_what_it_holds() {
    let doc = rich();
    let (replica, blobs, _) = replica_of(&doc);
    let back = read(&replica, &blobs);
    let story = &back.stories[&main_story(&back)];
    assert_eq!(story.frames.len(), 2, "threaded across two frames");
    assert_eq!((story.notes.len(), story.endnotes.len(), story.anchors.len(), story.xrefs.len()), (1, 1, 1, 1));
    assert_eq!(story.index_refs.len(), 1);
    assert_eq!(story.objects.len(), 1, "the anchored object");
    assert_eq!(story.xrefs[0].target, story.anchors[0].id, "the cross-reference still names its anchor");
    assert_eq!(story.notes[0].text.text, "A footnote\nin two paragraphs");
    let table = story.tables.values().next().unwrap();
    assert_eq!(table.cell(0, 0).unwrap().text.text, "Head");
    assert_eq!(table.cell(0, 0).unwrap().fill, "Brand");
    assert_eq!(table.cell(1, 2).unwrap().text.text, "Body\nTwo");
    assert_eq!(table.cell(1, 0).unwrap().row_span, 2);
    assert!(story.paras.iter().any(|p| p.table == Some(table.id)), "a paragraph anchors the table");
    assert_eq!(story.runs().next().unwrap().1.over.fill.as_deref(), Some("Brand"));
    assert_eq!(back.parents.len(), doc.parents.len());
    assert!(back.spreads.iter().flat_map(|s| s.pages.iter()).any(|p| p.parent == Some(back.parents.last().unwrap().id)));
    assert_eq!(back.assets.values().next().unwrap().data.as_slice(), &[0x89, b'P', b'N', b'G', 1, 2, 3]);
    assert_eq!(back.sections.len(), doc.sections.len());
    // A changed document does not compare equal.
    let mut other = back.clone();
    other.title = "Other".into();
    assert_ne!(normalize(&other), normalize(&back));
}
