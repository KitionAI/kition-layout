//! C8-layout-read: layer and guide dependencies read independently from their handlers.
use super::review2::reasons;
use super::*;

fn prepare(f: &Fixture, steps: &[(&str, Value)]) {
    let mut doc = f.disk();
    for (command, params) in steps {
        doc.execute(command, params.clone()).unwrap();
    }
    std::fs::write(f.root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
}

fn colleague_field(f: &Fixture, target: &Value, field: &str, value: Value) {
    let path = f.root.join(FILE);
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let mut doc = f.disk();
    let target = target.as_str().map(str::to_owned).unwrap_or_else(|| target.to_string());
    let field_value = json!({field:value});
    kition_kit::session::model::Exchange::new()
        .batch(&mut doc, &json!({"ops":[{"op":"set","target":target,"field":"props","value":field_value}]}), false)
        .unwrap();
    std::fs::write(&path, doc.seal_and_save(None).unwrap()).unwrap();
    std::fs::File::options().write(true).open(path).unwrap().set_modified(modified - std::time::Duration::from_secs(3600)).unwrap();
}

#[tokio::test]
async fn independent_layers_others_hold_affected_flags_and_leave_other_fields_free() {
    for mode in ["hide", "lock", "show", "unlock"] {
        for change in ["affected-flag", "excluded-flag", "excluded-name", "other-property", "new-layer", "layer-order", "unrelated-item"] {
            let f = Fixture::new(Guard::Premise);
            let field = if matches!(mode, "hide" | "show") { "visible" } else { "locked" };
            let initial = matches!(mode, "hide" | "unlock");
            for layer in &f.layers {
                let mut params = json!({"id":layer});
                params[field] = json!(initial);
                prepare(&f, &[("layer.set", params)]);
            }
            let basis = f.basis().await;
            let excluded = &f.layers[0];
            let affected = &f.layers[1];
            match change {
                "affected-flag" | "excluded-flag" => {
                    let mut params = json!({"id":if change == "affected-flag" {affected} else {excluded}});
                    params[field] = json!(!initial);
                    f.colleague(&[("layer.set", params)]);
                }
                "excluded-name" => f.colleague(&[("layer.set", json!({"id":excluded,"name":"Colleague"}))]),
                "other-property" => f.colleague(&[("layer.set", json!({"id":affected,"printable":false}))]),
                "new-layer" => f.colleague(&[("layer.new", json!({"name":"Colleague"}))]),
                "layer-order" => f.colleague(&[("layer.move", json!({"id":affected,"to":3}))]),
                _ => f.colleague(&[("object.opacity", json!({"ids":[f.c],"opacity":0.25}))]),
            }
            let mut params = json!({"id":excluded});
            params[mode] = json!(true);
            let answer = f.write(basis, "layer.others", params).await;
            let needed = matches!(change, "affected-flag" | "new-layer") || change == "excluded-flag" && matches!(mode, "show" | "unlock");
            assert_eq!(reasons(&answer).is_some(), needed, "{mode} {change}: {answer:?}");
            if !needed {
                assert!(answer.is_ok(), "{mode} {change}: {answer:?}");
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn independent_layers_new_holds_the_count_used_for_name_and_color() {
    for explicit in [false, true] {
        for change in ["new-layer", "layer-name", "layer-flags", "layer-order", "unrelated-item"] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            f.colleague(&[match change {
                "new-layer" => ("layer.new", json!({"name":"Colleague"})),
                "layer-name" => ("layer.set", json!({"id":f.layers[1],"name":"Colleague"})),
                "layer-flags" => ("layer.set", json!({"id":f.layers[1],"visible":false,"locked":true})),
                "layer-order" => ("layer.move", json!({"id":f.layers[1],"to":3})),
                _ => ("object.opacity", json!({"ids":[f.c],"opacity":0.25})),
            }]);
            let params = if explicit { json!({"name":"Mine"}) } else { json!({}) };
            let answer = f.write(basis, "layer.new", params).await;
            assert_eq!(reasons(&answer).is_some(), change == "new-layer", "explicit={explicit} {change}: {answer:?}");
            if change != "new-layer" {
                assert!(answer.is_ok(), "explicit={explicit} {change}: {answer:?}");
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn independent_layers_delete_unused_holds_usage_and_layer_membership() {
    for change in ["unused-becomes-used", "new-used-item", "new-layer", "layer-name", "layer-order", "used-to-used", "unrelated-item"] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        match change {
            "unused-becomes-used" => f.colleague(&[("object.setLayer", json!({"ids":[f.c],"layer":f.layers[1]}))]),
            "new-used-item" => f.colleague(&[
                ("layer.activate", json!({"id":f.layers[1]})),
                ("frame.create", json!({"rect":[400,400,440,440],"content":"unassigned"})),
            ]),
            "new-layer" => f.colleague(&[("layer.new", json!({"name":"Colleague"}))]),
            "layer-name" => f.colleague(&[("layer.set", json!({"id":f.layers[1],"name":"Colleague"}))]),
            "layer-order" => f.colleague(&[("layer.move", json!({"id":f.layers[1],"to":3}))]),
            "used-to-used" => f.colleague(&[("object.setLayer", json!({"ids":[f.c],"layer":f.layers[0]}))]),
            _ => f.colleague(&[("object.opacity", json!({"ids":[f.c],"opacity":0.25}))]),
        }
        let answer = f.write(basis, "layer.deleteUnused", json!({})).await;
        let needed = matches!(change, "unused-becomes-used" | "new-used-item" | "new-layer");
        assert_eq!(reasons(&answer).is_some(), needed, "{change}: {answer:?}");
        if !needed {
            assert!(answer.is_ok(), "{change}: {answer:?}");
        }
        f.close().await;
    }
}

#[tokio::test]
async fn independent_layers_delete_unused_keeps_the_observed_first_layer_when_empty() {
    for needed in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let inspected = f.disk().inspect().unwrap();
        let ids: Vec<Value> = inspected["spreads"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|spread| spread["items"].as_array().unwrap().iter().map(|item| item["id"].clone()))
            .collect();
        prepare(&f, &[("edit.clear", json!({"ids":ids}))]);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("layer.move", json!({"id":f.layers[1],"to":0}))
        } else {
            ("layer.set", json!({"id":f.layers[1],"name":"Colleague"}))
        }]);
        let answer = f.write(basis, "layer.deleteUnused", json!({})).await;
        assert_eq!(reasons(&answer).is_some(), needed, "empty needed={needed}: {answer:?}");
        if !needed {
            assert!(answer.is_ok(), "empty needed={needed}: {answer:?}");
            assert_eq!(f.disk().inspect().unwrap()["layers"][0]["id"], f.layers[0]);
        }
        f.close().await;
    }
}

#[tokio::test]
async fn independent_layers_guides_hold_layer_protection_only() {
    for command in ["guide.move", "guide.delete"] {
        for change in ["locked", "visible", "showGuides", "layer-name", "other-layer", "unrelated-item"] {
            let f = Fixture::new(Guard::Premise);
            prepare(&f, &[("layer.activate", json!({"id":f.layers[0]})), ("guide.add", json!({"orientation":"vertical","position":100}))]);
            let basis = f.basis().await;
            match change {
                "locked" => f.colleague(&[("layer.set", json!({"id":f.layers[0],"locked":true}))]),
                "visible" => f.colleague(&[("layer.set", json!({"id":f.layers[0],"visible":false}))]),
                "showGuides" => colleague_field(&f, &f.layers[0], "showGuides", json!(false)),
                "layer-name" => f.colleague(&[("layer.set", json!({"id":f.layers[0],"name":"Colleague"}))]),
                "other-layer" => f.colleague(&[("layer.set", json!({"id":f.layers[1],"locked":true,"visible":false}))]),
                _ => f.colleague(&[("object.opacity", json!({"ids":[f.c],"opacity":0.25}))]),
            }
            let params = if command == "guide.move" { json!({"page":0,"index":0,"position":200}) } else { json!({"page":0,"index":0}) };
            let answer = f.write(basis, command, params).await;
            let needed = matches!(change, "locked" | "visible" | "showGuides");
            assert_eq!(reasons(&answer).is_some(), needed, "{command} {change}: {answer:?}");
            if !needed {
                assert!(answer.is_ok(), "{command} {change}: {answer:?}");
            }
            f.close().await;
        }
    }
}
