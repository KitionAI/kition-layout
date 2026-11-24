//! The engine command bridge's session (`LayoutSession`) and the `.klayout` file it opens and
//! saves (design note sections 3.1–3.2 and 4; task W3-22 DE3).

mod common;

use std::sync::Arc;

use common::*;
use kition_layout_doc::geom::{Rect, shapes};
use kition_layout_doc::{Document, Item, ItemId, ParaFormat, Shape, SpreadRef};
use kition_layout_kdm::kition_kit::kdm::{Origin, Replica, UndoScope, Version};
use kition_layout_kdm::{LayoutSession, compare::normalize, open_file, save_file};

fn person() -> Origin {
    Origin::person("person:a")
}

fn edited(doc: &Arc<Document>, f: impl FnOnce(&mut Document)) -> Arc<Document> {
    let mut d = (**doc).clone();
    f(&mut d);
    d.check().unwrap();
    Arc::new(d)
}

fn add_frame(d: &mut Document, text: &str) -> ItemId {
    let lid = d.default_layer();
    d.add_text_frame(SpreadRef::Doc(0), Rect::new(40.0, 40.0, 300.0, 200.0), lid, text, ParaFormat::default()).unwrap().0
}

#[test]
fn records_undoes_and_redoes_on_the_replica() {
    let start = Arc::new(blank());
    let mut s = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    assert!(!s.can_undo(), "the creating transaction is no undo step");
    let after = edited(&start, |d| {
        add_frame(d, "Hello");
    });
    assert_eq!(s.record(&person(), &after).unwrap(), 1);
    assert_eq!(s.record(&person(), &after).unwrap(), 0, "nothing changed");
    assert!(s.can_undo());
    let (done, _) = s.undo().unwrap();
    assert!(done);
    assert_eq!(normalize(s.document()), normalize(&start));
    assert!(s.can_redo());
    s.redo().unwrap();
    assert_eq!(normalize(s.document()), normalize(&after));
    // A system transaction is never an undo step.
    let more = edited(s.document(), |d| d.title = "Renamed".into());
    assert_eq!(s.record(&Origin::system("edit"), &more).unwrap(), 0);
}

#[test]
fn an_undo_group_is_one_step_and_ends_at_another_author() {
    let start = Arc::new(blank());
    let mut s = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    s.begin_undo_group("person:a").unwrap();
    let one = edited(&start, |d| d.title = "One".into());
    let two = edited(&one, |d| d.title = "Two".into());
    assert_eq!(s.record(&person(), &one).unwrap(), 1);
    assert_eq!(s.record(&person(), &two).unwrap(), 0, "joined the group's step");
    // An agent's edit ends the person's group: it is a step of its own (on another register, so
    // KitModel 1.1 M12a's "a later write to the same register survives" stays out of the way).
    let three = edited(&two, |d| d.layers[0].name = "Agent".into());
    assert_eq!(s.record(&Origin::agent("agent:x"), &three).unwrap(), 1);
    assert!(!s.grouping());
    // M12a: each scope undoes its own steps.
    s.undo_in(UndoScope::Agent).unwrap();
    assert_eq!(s.document().layers[0].name, blank().layers[0].name, "the agent's undo takes the agent's step");
    assert_eq!(s.document().title, "Two", "and keeps the person's");
    s.undo_in(UndoScope::Person).unwrap();
    assert_eq!(s.document().title, blank().title, "the group undid as one step");
}

#[test]
fn two_sessions_converge_through_their_changes() {
    let start = Arc::new(rich());
    let mut a = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    let mut b = LayoutSession::open(&a.snapshot().unwrap(), a.blobs().clone()).unwrap();
    let a_doc = edited(a.document(), |d| d.title = "From A".into());
    a.record(&person(), &a_doc).unwrap();
    let b_doc = edited(b.document(), |d| {
        add_frame(d, "From B");
    });
    b.record(&Origin::person("person:b"), &b_doc).unwrap();
    let (va, vb) = (a.version(), b.version());
    b.import_changes(&a.export_changes(&vb).unwrap()).unwrap();
    a.import_changes(&b.export_changes(&va).unwrap()).unwrap();
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
    assert_eq!(normalize(a.document()), normalize(b.document()));
    assert_eq!(a.document().title, "From A");
    // Undo takes back only this replica's own edit.
    a.undo().unwrap();
    assert_ne!(a.document().title, "From A");
    assert!(normalize(a.document()).to_string().contains("From B"));
}

#[test]
fn the_file_holds_the_content_and_of_the_history_only_the_seal() {
    let start = Arc::new(rich());
    let mut s = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    s.seal().unwrap();
    let bytes = save_file(&s, None).unwrap().bytes;
    let (opened, base) = open_file(&bytes).unwrap();
    assert_eq!(opened.doc_id(), s.doc_id());
    assert_eq!(&opened.version(), base.version());
    assert_eq!(opened.version(), s.version(), "the file holds the session's version, seal included");
    assert_eq!(normalize(opened.document()), normalize(&start));
    assert!(!opened.can_undo());
    let parts = kition_layout_format::container::read(&bytes).unwrap();
    assert_eq!(Replica::snapshot_origins(&parts.content).unwrap(), vec![Some(Origin::system("content-seal"))]);
    assert!(!parts.blobs.is_empty(), "the image's blob is in the file");

    let after = edited(s.document(), |d| d.title = "Edited".into());
    s.record(&person(), &after).unwrap();
    assert!(!s.is_sealed(), "an edit after the seal");
    s.seal().unwrap();
    let again = save_file(&s, Some(&base)).unwrap().bytes;
    let (reopened, _) = open_file(&again).unwrap();
    assert_eq!(reopened.document().title, "Edited");
    assert_eq!(reopened.state_hash().unwrap(), s.state_hash().unwrap());
    let parts = kition_layout_format::container::read(&again).unwrap();
    assert_eq!(Replica::snapshot_origins(&parts.content).unwrap().len(), 1, "still one change");
    // Nothing new: the same bytes, from the session and from the file opened again.
    assert!(!s.seal().unwrap(), "a sealed session seals nothing more");
    assert_eq!(save_file(&s, Some(&base)).unwrap().bytes, again);
    assert_eq!(save_file(&reopened, None).unwrap().bytes, again, "the reopened file saves the same bytes");
}

/// A session nobody sealed does not save (`not-sealed: …`): hosts seal first (`prepare_save`), and
/// no sealed copy is made behind the session's back.
#[test]
fn an_unsealed_session_does_not_save() {
    let start = Arc::new(rich());
    let mut s = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    let after = edited(s.document(), |d| d.title = "Edited".into());
    s.record(&person(), &after).unwrap();
    let version = s.version();
    let error = save_file(&s, None).unwrap_err();
    assert!(error.to_string().starts_with("not-sealed"), "{error}");
    assert_eq!(s.version(), version, "the refused save left the session as it was");
    assert!(s.seal().unwrap());
    let saved = save_file(&s, None).unwrap();
    assert_eq!(saved.base.version(), &s.version(), "the file holds the sealed session's version");
    assert!(s.can_undo(), "the seal is no undo step and clears none");
    s.undo().unwrap();
    assert_ne!(s.document().title, "Edited", "the undo took back the last edit");
}

#[test]
fn damaged_and_foreign_files_are_refused() {
    assert!(open_file(b"not a layout").unwrap_err().to_string().starts_with("not-a-layout"));
    let engine_snapshot = kition_layout_format::save(&blank()).unwrap();
    assert!(open_file(&engine_snapshot).is_err(), "the engine's own snapshot is not a .klayout file");
    let mut s = LayoutSession::create(&Arc::new(blank()), &Origin::system("create")).unwrap();
    s.seal().unwrap();
    let bytes = save_file(&s, None).unwrap().bytes;
    let mut parts = kition_layout_format::container::read(&bytes).unwrap();
    parts.content = b"garbage".to_vec();
    let broken = kition_layout_format::container::write(&parts).unwrap();
    assert!(open_file(&broken).unwrap_err().to_string().starts_with("damaged"));
}

#[test]
fn blobs_since_ships_only_what_changed() {
    let start = Arc::new(rich());
    let mut s = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    assert!(!s.referenced_blobs().is_empty(), "the rich fixture places an image");
    let since = s.version();
    let after = edited(s.document(), |d| d.title = "No image".into());
    s.record(&person(), &after).unwrap();
    assert!(s.blobs_since(&since).is_empty());
    assert_eq!(s.blobs_since(&Version::empty()).len(), s.referenced_blobs().len());
}

/// Page `x`, `side` and the overridden parent items are stored, not derived (design note 2.1,
/// decided in DE3): an IDML page keeps its place, and two people overriding different parent
/// items on one page keep both overrides.
#[test]
fn concurrent_overrides_on_one_page_keep_both() {
    let mut start = rich();
    let parent_items: Vec<ItemId> = start.parents.iter().flat_map(|p| p.items.iter().map(|i| i.id)).collect();
    assert!(!parent_items.is_empty(), "the rich fixture has a parent item");
    let lid = start.default_layer();
    let id = ItemId(start.alloc());
    let rect = Item::new(id, lid, Shape::Rectangle, shapes::rectangle(Rect::new(10.0, 10.0, 30.0, 30.0)));
    let extra = start.insert_item(SpreadRef::Parent(0), rect, None).unwrap();
    let (one, two) = (parent_items[0], extra);
    let start = Arc::new(start);
    let mut a = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    let mut b = LayoutSession::open(&a.snapshot().unwrap(), a.blobs().clone()).unwrap();
    let override_on = |doc: &Arc<Document>, item: ItemId| {
        edited(doc, |d| {
            let spread = Arc::make_mut(&mut d.spreads[0]);
            spread.pages[0].overridden.push(item);
            spread.pages[0].x += 0.0;
        })
    };
    let a_doc = override_on(a.document(), one);
    a.record(&person(), &a_doc).unwrap();
    let b_doc = override_on(b.document(), two);
    b.record(&Origin::person("person:b"), &b_doc).unwrap();
    let (va, vb) = (a.version(), b.version());
    b.import_changes(&a.export_changes(&vb).unwrap()).unwrap();
    a.import_changes(&b.export_changes(&va).unwrap()).unwrap();
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap());
    let mut overridden = a.document().spreads[0].pages[0].overridden.clone();
    overridden.sort();
    let mut wanted = vec![one, two];
    wanted.sort();
    assert_eq!(overridden, wanted);
}

/// Two sessions from one base, each recording its edit, then exchanging their changes both ways.
fn concurrently(start: Document, a_edit: impl FnOnce(&mut Document), b_edit: impl FnOnce(&mut Document)) -> (LayoutSession, LayoutSession) {
    let base = LayoutSession::create(&Arc::new(start), &Origin::system("create")).unwrap();
    let snapshot = base.snapshot().unwrap();
    let mut a = LayoutSession::open(&snapshot, base.blobs().clone()).unwrap();
    let mut b = LayoutSession::open(&snapshot, base.blobs().clone()).unwrap();
    let a_doc = edited(a.document(), a_edit);
    a.record(&person(), &a_doc).unwrap();
    let b_doc = edited(b.document(), b_edit);
    b.record(&Origin::person("person:b"), &b_doc).unwrap();
    let (va, vb) = (a.version(), b.version());
    let (from_a, from_b) = (a.export_changes(&vb).unwrap(), b.export_changes(&va).unwrap());
    a.import_changes(&from_b).unwrap();
    b.import_changes(&from_a).unwrap();
    // The repairs each side wrote reach the other.
    let (va, vb) = (a.version(), b.version());
    let (from_a, from_b) = (a.export_changes(&vb).unwrap(), b.export_changes(&va).unwrap());
    a.import_changes(&from_b).unwrap();
    b.import_changes(&from_a).unwrap();
    assert_eq!(a.state_hash().unwrap(), b.state_hash().unwrap(), "the replicas converge");
    assert_eq!(normalize(a.document()), normalize(b.document()), "and so do their views");
    (a, b)
}

/// One person deletes a text variable while another inserts an instance of it: the instance,
/// which names a variable that is gone, is removed (the read repair), the same on both sides.
#[test]
fn an_instance_of_a_variable_deleted_meanwhile_is_removed() {
    use kition_layout_doc::vars::{var_char, var_index};
    let (a, _) = concurrently(
        linked(),
        |d| {
            // What `variables.delete` does to "Client": its instance becomes its value.
            let story = linked_story(d);
            let client = var_char(d.text_variables.len() - 1).unwrap();
            let s = d.story_mut(story).unwrap();
            let at = s.text.find(client).unwrap();
            s.replace(at..at + client.len_utf8(), "Acme");
            d.text_variables.pop();
            // The placeholder after it moves with its text.
            for p in &mut d.data_merge.placeholders {
                if let kition_layout_doc::PlaceholderAnchor::Text { start, end, .. } = &mut p.anchor
                    && *start > at
                {
                    *start += "Acme".len() - client.len_utf8();
                    *end += "Acme".len() - client.len_utf8();
                }
            }
        },
        |d| {
            let story = linked_story(d);
            let client = var_char(d.text_variables.len() - 1).unwrap().to_string();
            let s = d.story_mut(story).unwrap();
            let end = s.len();
            s.insert(end, &client);
        },
    );
    let d = a.document();
    let text = &d.stories[&linked_story(d)].text;
    assert!(text.contains("Acme"), "{text}");
    assert_eq!(text.chars().filter_map(var_index).count(), 0, "no instance names a variable that is gone: {text:?}");
}
