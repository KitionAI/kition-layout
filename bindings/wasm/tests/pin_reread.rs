//! Regression (cases/regressions/issue-c8-layout-pin-reread): a clean session reconciles a file
//! another writer saved (kition-sdk 2ce294a1, `KitDocument::reconcile_reread`). Only the file's
//! difference is imported, as one `import` transaction outside both undo histories, so the
//! session keeps its object identities, its editor state and both scopes' undo steps; an editor
//! state naming what the file no longer has is refused, and the session is left as it was.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_kit::session::model::Exchange;
use kition_layout_kdm::kition_kit::kdm::Origin;
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};

const FILE: &str = "Reread.klayout";

fn frame(doc: &mut LayoutDocument, rect: [u32; 4]) -> Value {
    doc.execute("frame.add", json!({"rect":rect,"content":"unassigned"})).unwrap()["id"].clone()
}

fn field(doc: &mut LayoutDocument, id: &Value, name: &str) -> Value {
    let loaded = Exchange::new().batch(doc, &json!({"ops":[],"load":[{"target":id.to_string(),"fields":[name]}]}), true).unwrap();
    loaded["loaded"][0]["fields"][name].clone()
}

/// The live session saved and clean, and the file as a colleague who ran `steps` saved it.
fn saved_and_rewritten(live: &mut LayoutDocument, steps: &[(&str, Value)]) -> LayoutDocument {
    let bytes = live.save(None).unwrap();
    live.mark_saved();
    let mut colleague = LayoutDocument::open(FILE, &bytes).unwrap();
    for (command, params) in steps {
        colleague.execute(command, params.clone()).unwrap();
    }
    LayoutDocument::open(FILE, &colleague.save(None).unwrap()).unwrap()
}

#[test]
fn an_outside_save_keeps_agent_undo_and_the_colleagues_edit() {
    let mut live = LayoutDocument::create(FILE, 612, 792).unwrap();
    let mine = frame(&mut live, [20, 20, 60, 60]);
    let theirs = frame(&mut live, [100, 20, 140, 60]);
    live.execute_as("object.opacity", json!({"ids":[mine],"opacity":0.5}), &Origin::agent("me")).unwrap();
    let mut fresh = saved_and_rewritten(&mut live, &[("object.opacity", json!({"ids":[theirs],"opacity":0.25}))]);
    fresh.restore_editor_state(&live.editor_state()).unwrap();
    assert!(live.reconcile_reread(&fresh).unwrap());
    assert!(!live.dirty(), "a reconciled session holds what the file holds");
    assert_eq!(field(&mut live, &theirs, "opacity"), json!(0.25), "the colleague's edit is imported");
    live.execute_as("edit.undo", json!({}), &Origin::agent("me")).unwrap();
    assert_eq!(field(&mut live, &mine, "opacity"), json!(1), "the agent's step still undoes");
    assert_eq!(field(&mut live, &theirs, "opacity"), json!(0.25), "the import is no undo step");
}

#[test]
fn a_new_object_in_the_file_is_imported_and_saved_again() {
    let mut live = LayoutDocument::create(FILE, 612, 792).unwrap();
    frame(&mut live, [20, 20, 60, 60]);
    let fresh = saved_and_rewritten(&mut live, &[("frame.add", json!({"rect":[300,300,340,340],"content":"unassigned"}))]);
    let wanted = fresh.inspect().unwrap()["items"].clone();
    assert!(live.reconcile_reread(&fresh).unwrap());
    assert_eq!(live.inspect().unwrap()["items"], wanted, "the session shows the file's items with their ids");
    let reopened = LayoutDocument::open(FILE, &live.save(None).unwrap()).unwrap();
    assert_eq!(reopened.inspect().unwrap()["items"], wanted, "the next save holds them");
}

#[test]
fn the_editor_state_resolves_on_the_fresh_document() {
    let mut live = LayoutDocument::create(FILE, 612, 792).unwrap();
    let a = frame(&mut live, [20, 20, 60, 60]);
    live.execute("select.set", json!({"ids":[a]})).unwrap();
    let mut fresh = saved_and_rewritten(&mut live, &[]);
    fresh.restore_editor_state(&live.editor_state()).unwrap();
    assert_eq!(fresh.inspect().unwrap()["selection"], live.inspect().unwrap()["selection"]);
}

#[test]
fn a_reread_refuses_a_deleted_selected_item_and_keeps_the_session() {
    let mut live = LayoutDocument::create(FILE, 612, 792).unwrap();
    let a = frame(&mut live, [20, 20, 60, 60]);
    frame(&mut live, [100, 20, 140, 60]);
    live.execute("select.set", json!({"ids":[a]})).unwrap();
    let saved = live.editor_state();
    let mut fresh = saved_and_rewritten(&mut live, &[("edit.delete", json!({"ids":[a]}))]);
    let error = fresh.restore_editor_state(&saved).unwrap_err();
    assert_eq!(error, format!("deleted selected item {a}"));
    let before = live.save(None).unwrap();
    assert_eq!(live.reconcile_reread(&fresh).unwrap_err(), format!("deleted selected item {a}"));
    assert_eq!(live.save(None).unwrap(), before, "a refused reread leaves the session as it was");
    assert_eq!(live.editor_state(), saved);
}

#[test]
fn the_sessions_own_state_is_put_back_after_a_call_deleted_what_it_selected() {
    // A host restores the person's editor state after an agent's call: what the call deleted is
    // left out, as before.
    let mut live = LayoutDocument::create(FILE, 612, 792).unwrap();
    let a = frame(&mut live, [20, 20, 60, 60]);
    live.execute("select.set", json!({"ids":[a]})).unwrap();
    let saved = live.editor_state();
    live.execute("edit.delete", json!({"ids":[a]})).unwrap();
    live.restore_editor_state(&saved).unwrap();
    assert_eq!(live.inspect().unwrap()["selection"]["items"], json!([]));
}
