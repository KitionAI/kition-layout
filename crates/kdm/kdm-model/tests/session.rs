//! The engine command bridge's session (`LayoutSession`) and the `.klayout` file it opens and
//! saves (design note sections 3.1–3.2 and 4; task W3-22 DE3).

mod common;

use std::sync::Arc;

use common::*;
use kition_layout_doc::geom::Rect;
use kition_layout_doc::{Document, ItemId, ParaFormat, SpreadRef};
use kition_layout_kdm::kition_kit::kdm::{Origin, UndoScope, Version};
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
fn the_file_holds_the_content_and_no_version_actor_or_origin() {
    let start = Arc::new(rich());
    let mut s = LayoutSession::create(&start, &Origin::system("create")).unwrap();
    let bytes = save_file(&s, None).unwrap().bytes;
    let (opened, base) = open_file(&bytes).unwrap();
    assert_eq!(opened.doc_id(), s.doc_id());
    assert_eq!(&opened.version(), base.version());
    assert_eq!(opened.state_hash().unwrap(), s.state_hash().unwrap(), "the file holds the session's state, under its ids");
    assert_eq!(normalize(opened.document()), normalize(&start));
    assert!(!opened.can_undo());
    let parts = kition_layout_format::container::read(&bytes).unwrap();
    let content = String::from_utf8(parts.content.clone()).unwrap();
    assert!(content.starts_with("{\"kdm\":1,\"kit\":\"kit.layout\",\"docId\":"), "{content}");
    for key in ["\"version\"", "\"lineage\"", "\"actor\"", "\"origin\"", "system:", "person:"] {
        assert!(!content.contains(key), "the content holds no version, actor or origin: {key}");
    }
    assert!(!parts.blobs.is_empty(), "the image's blob is in the file");

    let after = edited(s.document(), |d| d.title = "Edited".into());
    s.record(&person(), &after).unwrap();
    let version = s.version();
    let again = save_file(&s, Some(&base)).unwrap().bytes;
    assert_eq!(s.version(), version, "a save writes nothing into the replica");
    let (reopened, _) = open_file(&again).unwrap();
    assert_eq!(reopened.document().title, "Edited");
    assert_eq!(reopened.state_hash().unwrap(), s.state_hash().unwrap());
    let content = String::from_utf8(kition_layout_format::container::read(&again).unwrap().content).unwrap();
    assert!(!content.contains("person:a"), "the edit's origin stays in the session");
    // Nothing new: the same bytes, from the session and from the file opened again.
    assert_eq!(save_file(&s, Some(&base)).unwrap().bytes, again);
    assert_eq!(save_file(&reopened, None).unwrap().bytes, again, "the reopened file saves the same bytes");
    assert!(s.can_undo(), "a save is no undo step and clears none");
    s.undo().unwrap();
    assert_ne!(s.document().title, "Edited", "the undo took back the last edit");
}

#[test]
fn damaged_and_foreign_files_are_refused() {
    assert!(open_file(b"not a layout").unwrap_err().to_string().starts_with("not-a-layout"));
    let engine_snapshot = kition_layout_format::save(&blank()).unwrap();
    assert!(open_file(&engine_snapshot).is_err(), "the engine's own snapshot is not a .klayout file");
    let s = LayoutSession::create(&Arc::new(blank()), &Origin::system("create")).unwrap();
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
