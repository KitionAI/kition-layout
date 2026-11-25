//! C8-land-13-layout: the host puts the person's whole editor state back.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
use kition_kit::{CallContext, KitDocument};
use kition_layout_kit::LayoutDocument;
use serde_json::json;

#[test]
fn additional_selection_and_active_dependencies_are_declared() {
    let doc = LayoutDocument::create("Reads.klayout", 612, 792).unwrap();
    let commands = doc.commands();
    for id in ["footnote.insert", "footnote.goToReference", "endnote.insert", "note.new", "style.paragraph.add", "style.object.add", "object.rename"]
    {
        let command = commands.iter().find(|command| command.id == id).unwrap();
        assert!(command.reads.contains(&kition_kit::EditorState::Selection), "{id} reads the insertion point, fromSelection, or targets fallback");
    }
    let grid = commands.iter().find(|command| command.id == "frame.grid").unwrap();
    assert!(grid.reads.contains(&kition_kit::EditorState::Active), "frame.grid delegates to frame.add, including its active layer");
}

#[test]
fn command_reads_cover_implicit_engine_state() {
    use kition_kit::EditorState::{Active, Mode, Selection};
    let doc = LayoutDocument::create("Reads.klayout", 612, 792).unwrap();
    let commands = doc.commands();
    for (id, reads) in [("frame.add", vec![Active, Mode]), ("object.fill", vec![Selection, Mode]), ("text.insert", vec![Selection, Mode])] {
        let command = commands.iter().find(|command| command.id == id).unwrap();
        for read in reads {
            assert!(command.reads.contains(&read), "{id} omits {read:?}");
        }
    }
}

#[test]
fn full_context_is_reversible_without_an_edit() {
    let mut doc = LayoutDocument::create("Context.klayout", 612, 792).unwrap();
    let a = doc.execute("frame.add", json!({"rect":[20,20,60,60],"content":"text"})).unwrap()["id"].clone();
    let b = doc.execute("frame.add", json!({"rect":[100,20,140,60],"content":"unassigned"})).unwrap()["id"].clone();
    doc.execute("select.set", json!({"ids":[a]})).unwrap();
    let saved = doc.editor_state();
    assert!(saved.is_object(), "the complete editor state must be available");
    kition_kit::testing::check_context(&mut doc, &CallContext { on: vec![b], ..Default::default() }, &["/selection", "/activeLayer", "/tool"])
        .unwrap();
    assert_eq!(doc.editor_state(), saved);
    let before = doc.inspect().unwrap();
    assert!(doc.set_context(&CallContext { on: vec![json!(u64::MAX)], ..Default::default() }).is_err());
    assert_eq!(doc.inspect().unwrap(), before, "bad context must be atomic");
    doc.execute("prefs.set", json!({"smartTextReflow":false})).unwrap();
    doc.execute("tool.select", json!({"tool":"pen"})).unwrap();
    doc.restore_editor_state(&saved).unwrap();
    assert_eq!(doc.editor_state(), saved, "tool and preferences must also be restored");
}
