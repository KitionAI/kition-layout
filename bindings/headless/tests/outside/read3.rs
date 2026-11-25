//! C8-layout-read-3: what the blind readings name a write reads, an independent writer changes.
use super::*;

/// Typing records a tracked change only when the document tracks changes: a colleague who turns
/// tracking on changes what the agent's insertion writes. Renaming another frame does not.
#[tokio::test]
async fn read3_typing_holds_track_changes() {
    for change in ["track", "other-item"] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[if change == "track" {
            ("change.track", json!({"on":true}))
        } else {
            ("object.rename", json!({"id":f.b,"name":"Colleague"}))
        }]);
        let answer = f
            .call(
                "run",
                json!({"basis":basis,"steps":[
                    {"command":"text.select","params":{"story":f.story,"anchor":2,"focus":4}},
                    {"command":"text.insert","params":{"text":"x","raw":true}}
                ]}),
            )
            .await;
        if change == "track" {
            conflict(&answer.unwrap_err(), "changed");
        } else {
            assert!(answer.is_ok(), "unrelated {change} must apply: {answer:?}");
        }
        f.close().await;
    }
}

/// A new text frame's story takes its direction from the binding (doc/src/edit.rs:176): a
/// colleague who switches the binding changes what the agent's frame is created as.
#[tokio::test]
async fn read3_new_text_frame_holds_creation_defaults() {
    for change in ["binding", "other-item"] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[if change == "binding" {
            ("layout.documentSetup", json!({"binding":"rightToLeft"}))
        } else {
            ("object.rename", json!({"id":f.b,"name":"Colleague"}))
        }]);
        let answer = f.write(basis, "frame.add", json!({"rect":[400,20,500,120],"content":"text","text":"New"})).await;
        if change == "binding" {
            conflict(&answer.unwrap_err(), "changed");
        } else {
            assert!(answer.is_ok(), "unrelated {change} must apply: {answer:?}");
        }
        f.close().await;
    }
}

/// Editing a table style applies it again to every table using it (cmd/table.rs:1270): a
/// colleague's change of such a table's options changes what the edit writes there.
#[tokio::test]
async fn read3_table_style_edit_holds_the_tables_using_it() {
    for change in ["table-options", "other-item"] {
        let f = Fixture::new(Guard::Premise);
        let mut doc = f.disk();
        doc.execute("text.select", json!({"story":f.story,"anchor":0})).unwrap();
        let table = doc.execute("table.insert", json!({"rows":2,"cols":2})).unwrap()["table"].clone();
        doc.execute("style.table.add", json!({"name":"Grid"})).unwrap();
        doc.execute("style.table.apply", json!({"name":"Grid","story":f.story,"table":table})).unwrap();
        std::fs::write(f.root.join(FILE), doc.save(None).unwrap()).unwrap();
        let basis = f.basis().await;
        f.colleague(&[if change == "table-options" {
            ("table.options", json!({"story":f.story,"table":table,"spaceAfter":9}))
        } else {
            ("object.rename", json!({"id":f.b,"name":"Colleague"}))
        }]);
        let answer = f.write(basis, "style.table.edit", json!({"name":"Grid","spaceBefore":8})).await;
        if change == "table-options" {
            conflict(&answer.unwrap_err(), "changed");
        } else {
            assert!(answer.is_ok(), "unrelated {change} must apply: {answer:?}");
        }
        f.close().await;
    }
}

/// A variable instance stores the variable's index (doc/src/lib.rs `text_variables`): a
/// colleague's deletion of an earlier variable makes the agent's index name another one.
#[tokio::test]
async fn read3_variable_insertion_holds_the_variable_list() {
    for change in ["earlier-variable", "other-item"] {
        let f = Fixture::new(Guard::Premise);
        let mut doc = f.disk();
        doc.execute("variable.define", json!({"name":"First","type":"custom","text":"One"})).unwrap();
        doc.execute("variable.define", json!({"name":"Var","type":"custom","text":"Value"})).unwrap();
        std::fs::write(f.root.join(FILE), doc.save(None).unwrap()).unwrap();
        let basis = f.basis().await;
        f.colleague(&[if change == "earlier-variable" {
            ("variable.delete", json!({"name":"First"}))
        } else {
            ("object.rename", json!({"id":f.b,"name":"Colleague"}))
        }]);
        let answer = f
            .call(
                "run",
                json!({"basis":basis,"steps":[
                    {"command":"text.select","params":{"story":f.story,"anchor":2,"focus":2}},
                    {"command":"variable.insert","params":{"name":"Var"}}
                ]}),
            )
            .await;
        if change == "earlier-variable" {
            conflict(&answer.unwrap_err(), "changed");
        } else {
            assert!(answer.is_ok(), "unrelated {change} must apply: {answer:?}");
        }
        f.close().await;
    }
}

/// A new article without a name takes the first unused `Article N` (cmd/articles.rs:16-28): a
/// colleague's new article takes that name, so the agent's would duplicate it.
#[tokio::test]
async fn read3_new_article_holds_the_article_names() {
    for change in ["article", "other-item"] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[if change == "article" {
            ("article.new", json!({"ids":[f.c]}))
        } else {
            ("object.rename", json!({"id":f.b,"name":"Colleague"}))
        }]);
        let answer = f.write(basis, "article.new", json!({"ids":[f.a]})).await;
        if change == "article" {
            conflict(&answer.unwrap_err(), "changed");
        } else {
            assert!(answer.is_ok(), "unrelated {change} must apply: {answer:?}");
        }
        f.close().await;
    }
}

/// A drop shadow patches that effect only (cmd/object.rs set_flag): a colleague's rename or move
/// of the same frame applies; a colleague's drop shadow on it refuses.
#[tokio::test]
async fn read3_effect_holds_only_that_effect() {
    for change in ["shadow", "rename", "move"] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[match change {
            "shadow" => ("object.dropShadow", json!({"ids":[f.a],"distance":3})),
            "rename" => ("object.rename", json!({"id":f.a,"name":"Colleague"})),
            _ => ("transform.move", json!({"ids":[f.a],"dx":5,"dy":0})),
        }]);
        let answer = f.write(basis, "object.dropShadow", json!({"ids":[f.a],"distance":10})).await;
        if change == "shadow" {
            conflict(&answer.unwrap_err(), "changed");
        } else {
            assert!(answer.is_ok(), "unrelated {change} must apply: {answer:?}");
        }
        f.close().await;
    }
}
