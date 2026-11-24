//! C8-layout review. Each stale write has a real, independently saved colleague edit.
use super::*;

fn prepare(f: &Fixture, steps: &[(&str, Value)]) -> Vec<Value> {
    let mut doc = f.disk();
    let results = steps.iter().map(|(id, params)| doc.execute(id, params.clone()).unwrap()).collect();
    std::fs::write(f.root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
    results
}

#[tokio::test]
async fn review_delegated_text_consumers_hold_selected_story() {
    for (command, params) in [
        ("condition.apply", json!({"name":"Review"})),
        ("variables.insert", json!({"name":"Review"})),
        ("xml.tagText", json!({"tag":"Review"})),
        ("index.addReference", json!({"topic":"Review"})),
    ] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            prepare(&f, &[("condition.new", json!({"name":"Review"})), ("variables.define", json!({"name":"Review","text":"Value"}))]);
            let mut selected = f.disk();
            selected.execute("text.select", json!({"story":f.story,"anchor":4,"focus":6})).unwrap();
            let held = selected.under(command, &params).unwrap().unwrap_or_default();
            assert!(held.premises.iter().any(|p| p.id == f.story), "{command} must hold the selected story: {held:?}");
            let basis = f.basis().await;
            f.colleague(&[if needed {
                ("story.replaceRange", json!({"story":f.story,"start":0,"end":0,"text":"Early"}))
            } else {
                ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
            }]);
            let answer = f
                .call(
                    "run",
                    json!({"basis":basis,"steps":[
                        {"command":"text.select","params":{"story":f.story,"anchor":4,"focus":6}},
                        {"command":command,"params":params}
                    ]}),
                )
                .await;
            if needed {
                assert_eq!(answer.unwrap_err()["error"]["code"], "conflict");
            } else {
                answer.unwrap();
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review_generated_text_holds_source_content() {
    for command in ["toc.update", "index.update"] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            prepare(
                &f,
                &[
                    ("style.paragraph.create", json!({"name":"Review"})),
                    ("text.select", json!({"story":f.story,"anchor":0,"focus":6})),
                    ("style.paragraph.apply", json!({"name":"Review"})),
                    ("index.addReference", json!({"topic":"Review"})),
                    ("toc.generate", json!({"styles":["Review"]})),
                    ("index.generate", json!({})),
                ],
            );
            let basis = f.basis().await;
            f.colleague(&[if needed {
                ("story.replaceRange", json!({"story":f.story,"start":4,"end":5,"text":"Changed"}))
            } else {
                ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
            }]);
            expect_conflict_or_apply(&f, basis, command, json!({}), needed).await;
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review_page_deletion_holds_removed_story() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("story.replaceRange", json!({"story":f.story,"start":0,"end":0,"text":"Colleague"}))
        } else {
            ("layer.set", json!({"id":f.layers[1],"name":"Unrelated"}))
        }]);
        expect_conflict_or_apply(&f, basis, "layout.pages.delete", json!({"pages":[0]}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_page_deletion_holds_items_arriving_on_the_page() {
    // Deleting page 0 deletes whatever is on it when it runs: an item another writer put there
    // after the observation must refuse; an item put on a later spread must not.
    for change in ["created-on-page", "moved-onto-page", "created-elsewhere"] {
        let f = Fixture::new(Guard::Premise);
        let elsewhere = prepare(&f, &[("frame.create", json!({"rect":[20,20,60,60],"content":"unassigned","spread":3}))]).remove(0);
        let basis = f.basis().await;
        f.colleague(&[match change {
            "created-on-page" => ("frame.create", json!({"rect":[400,400,440,440],"content":"unassigned"})),
            "moved-onto-page" => ("transform.move", json!({"ids":[elsewhere["id"]],"dx":0,"dy":0,"toSpread":{"kind":"doc","index":0}})),
            _ => ("frame.create", json!({"rect":[100,100,140,140],"content":"unassigned","spread":4})),
        }]);
        expect_conflict_or_apply(&f, basis, "layout.pages.delete", json!({"pages":[0]}), change != "created-elsewhere").await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_page_size_holds_items_arriving_on_the_page() {
    // A page-size write reads the geometry of every item on the page (its items follow the page
    // and its liquid rule), so it holds them. Since kition-sdk 2ce294a1 a premise on an object
    // others created after `basis` refuses `relocated`: the write would move an item the agent
    // never observed. (At the earlier pin only a deletion of the page's items refused.)
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("frame.create", json!({"rect":[400,400,440,440],"content":"unassigned"}))]);
    let answer = f.write(basis, "layout.pageSize", json!({"pages":[1],"width":500})).await;
    conflict(&answer.unwrap_err(), "relocated");
    f.close().await;
}

#[tokio::test]
async fn review_layer_deletion_holds_the_items_it_deletes() {
    // Deleting a layer deletes the items on it when it runs.
    for change in ["item-on-layer-changed", "item-moved-onto-layer", "unrelated"] {
        let f = Fixture::new(Guard::Premise);
        let layer = f.layers[1].clone();
        prepare(&f, &[("object.setLayer", json!({"ids":[f.b],"layer":layer}))]);
        let basis = f.basis().await;
        f.colleague(&[match change {
            "item-on-layer-changed" => ("object.opacity", json!({"ids":[f.b],"opacity":0.25})),
            "item-moved-onto-layer" => ("object.setLayer", json!({"ids":[f.c],"layer":layer})),
            _ => ("object.opacity", json!({"ids":[f.c],"opacity":0.25})),
        }]);
        expect_conflict_or_apply(&f, basis, "layer.delete", json!({"id":layer}), change != "unrelated").await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_layer_assignment_ignores_items_arriving_on_the_layer() {
    // Moving an item onto a layer rests on the layer, not on what else another writer put on it.
    let f = Fixture::new(Guard::Premise);
    let layer = f.layers[1].clone();
    let basis = f.basis().await;
    f.colleague(&[("object.setLayer", json!({"ids":[f.c],"layer":layer}))]);
    expect_conflict_or_apply(&f, basis, "object.setLayer", json!({"ids":[f.a],"layer":layer}), false).await;
    f.close().await;
}

#[tokio::test]
async fn review_guide_index_tracks_earlier_guides() {
    // A guide's index on its page is a positional address: deleting an earlier guide shifts it.
    for command in ["guide.delete", "guide.move"] {
        for change in ["earlier-deleted", "addressed-moved", "unrelated"] {
            let f = Fixture::new(Guard::Premise);
            for position in [100, 200, 300] {
                prepare(&f, &[("guide.add", json!({"orientation":"horizontal","position":position,"page":0}))]);
            }
            let basis = f.basis().await;
            f.colleague(&[match change {
                "earlier-deleted" => ("guide.delete", json!({"page":0,"index":0})),
                "addressed-moved" => ("guide.move", json!({"page":0,"index":1,"position":250})),
                _ => ("object.opacity", json!({"ids":[f.c],"opacity":0.25})),
            }]);
            let params = if command == "guide.move" { json!({"page":0,"index":1,"position":220}) } else { json!({"page":0,"index":1}) };
            expect_conflict_or_apply(&f, basis, command, params, change != "unrelated").await;
            f.close().await;
        }
    }
}

#[tokio::test]
async fn list_move_after_deletion_before_its_indices_refuses() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("layer.delete", json!({"id":f.layers[0]}))]);
    let answer = f.call("batch", json!({"basis":basis,"ops":[{"op":"list.move","target":"layout","field":"layers","from":1,"to":2}]})).await;
    // kition-sdk 2ce294a1 follows the moved indices through the earlier deletion.
    conflict(&answer.unwrap_err(), "relocated");
    f.close().await;
}

#[tokio::test]
async fn review_settings_written_fields_hold_old_values() {
    for (command, theirs, mine) in [
        ("document.preferences", json!({"keyboardIncrement":2}), json!({"keyboardIncrement":3})),
        ("edit.transparencyBlendSpace", json!({"space":"rgb"}), json!({"space":"cmyk"})),
        ("spelling.setWords", json!({"words":["colleagueword"]}), json!({"words":["agentword"]})),
        ("changes.track", json!({"on":true}), json!({"on":false})),
    ] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            f.colleague(&[if needed { (command, theirs.clone()) } else { ("object.opacity", json!({"ids":[f.c],"opacity":0.25})) }]);
            expect_conflict_or_apply(&f, basis, command, mine.clone(), needed).await;
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review_reflow_holds_wrap_geometry() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let frame = prepare(
            &f,
            &[(
                "frame.create",
                json!({"rect":[20,20,250,200],"content":"text","text":"Words that fit around a wrapping object and occupy several lines."}),
            )],
        )
        .remove(0);
        prepare(&f, &[("object.textWrap", json!({"ids":[f.b],"mode":"bounds"}))]);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("transform.move", json!({"ids":[f.b],"dx":20,"dy":80}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        expect_conflict_or_apply(&f, basis, "frame.fitText", json!({"ids":[frame["id"]]}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_idml_is_view_only_and_write_refusal_preserves_bytes() {
    let f = Fixture::new(Guard::Premise);
    let original = f.disk().save(Some("View.idml")).unwrap();
    std::fs::write(f.root.join("View.idml"), &original).unwrap();
    let answer = tools::call_as(
        &f.kits,
        "layout_run",
        &json!({"path":"View.idml","steps":[{"command":"object.rename","params":{"id":f.a,"name":"Must refuse"}}]}),
        f.options,
        "c8-agent",
    )
    .await
    .unwrap();
    assert!(answer.is_err(), "IDML writes must refuse: {answer:?}");
    let error = answer.unwrap_err().structured();
    assert_eq!(error["error"]["code"], "invalid-params");
    assert!(error["error"]["message"].as_str().unwrap().contains(".klayout"));
    println!("IDMLREFUSAL {}", error);
    assert_eq!(std::fs::read(f.root.join("View.idml")).unwrap(), original);
    f.close().await;
}

async fn expect_conflict_or_apply(f: &Fixture, basis: Value, command: &str, params: Value, needed: bool) {
    let answer = f.write(basis, command, params).await;
    if needed {
        assert_eq!(answer.as_ref().err().map(|e| &e["error"]["code"]), Some(&json!("conflict")), "{command}: {answer:?}");
    } else {
        answer.unwrap_or_else(|e| panic!("unrelated change must permit {command}: {e}"));
    }
}

#[tokio::test]
async fn review_parent_application_holds_prefix_resolution() {
    for change in ["prefix", "definition", "other"] {
        let f = Fixture::new(Guard::Premise);
        let parent = f.disk().inspect().unwrap()["parents"][0]["id"].clone();
        let basis = f.basis().await;
        if change == "other" {
            f.colleague(&[("object.opacity", json!({"ids":[f.c],"opacity":0.25}))]);
        } else {
            let mut doc = f.disk();
            kition_kit::session::model::Exchange::new()
                .batch(
                    &mut doc,
                    &json!({"ops":[{
                        "op":"set","target":parent.to_string(),"field":if change == "prefix" {"prefix"} else {"allowShuffle"},
                        "value":if change == "prefix" {json!("Gone")} else {json!(false)}
                    }]}),
                    false,
                )
                .unwrap();
            std::fs::write(f.root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
        }
        expect_conflict_or_apply(&f, basis, "layout.pages.applyParent", json!({"pages":[1],"parent":"A"}), change == "prefix").await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_find_change_holds_query_membership_and_target_fields() {
    for change in ["target", "membership", "other"] {
        let f = Fixture::new(Guard::Premise);
        prepare(&f, &[("object.label", json!({"ids":[f.a],"label":"Review"}))]);
        let basis = f.basis().await;
        f.colleague(&[match change {
            "target" => ("object.opacity", json!({"ids":[f.a],"opacity":0.25})),
            "membership" => ("object.label", json!({"ids":[f.b],"label":"Review"})),
            _ => ("object.opacity", json!({"ids":[f.c],"opacity":0.25})),
        }]);
        expect_conflict_or_apply(&f, basis, "find.changeObjects", json!({"label":"Review","change":{"opacity":0.75}}), change != "other").await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_places_page_number_tracks_preceding_deletion() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        prepare(&f, &[("layout.pages.insert", json!({"count":5}))]);
        let basis = f.basis().await;
        f.colleague(&[if needed { ("layout.pages.delete", json!({"pages":[0]})) } else { ("object.opacity", json!({"ids":[f.c],"opacity":0.25})) }]);
        expect_conflict_or_apply(&f, basis, "layout.pageSize", json!({"pages":[3],"width":400}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_joined_fit_and_caret_keep_story_text() {
    // The composition premise on a story must add to, not replace, the story's text premise.
    for command in ["frame.fitText", "text.placeCaret"] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let frame = prepare(&f, &[("frame.create", json!({"rect":[20,200,300,400],"content":"text","text":"Original text"}))]).remove(0);
            let basis = f.basis().await;
            f.colleague(&[if needed {
                ("story.setText", json!({"story":frame["story"],"text":"Colleague text that is longer than before"}))
            } else {
                ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
            }]);
            let answer = if command == "text.placeCaret" {
                f.call("run", json!({"basis":basis,"steps":[{"command":command,"params":{"frame":frame["id"],"point":[25,205]}},{"command":"text.insert","params":{"text":"Mine"}}]})).await
            } else {
                f.write(basis, command, json!({"ids":[frame["id"]]})).await
            };
            if needed {
                assert_eq!(answer.as_ref().err().map(|e| &e["error"]["code"]), Some(&json!("conflict")), "{command}: {answer:?}");
            } else {
                answer.unwrap_or_else(|e| panic!("{command}: {e}"));
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review_joined_page_prefix_keeps_earlier_page_content() {
    // pages [1,3] hold page 1 entirely; page 3's counted prefix must not narrow it again.
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("layout.pageSize", json!({"pages":[1],"width":500}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        expect_conflict_or_apply(&f, basis, "layout.pageSize", json!({"pages":[1,3],"width":400}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_places_note_identity_and_content() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let frame = prepare(&f, &[("frame.create", json!({"rect":[20,200,300,400],"content":"text","text":"Body"}))]).remove(0);
        let note = prepare(&f, &[("text.select", json!({"story":frame["story"],"anchor":0})), ("note.new", json!({"text":"Original"}))]).remove(1);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("note.edit", json!({"story":note["story"],"id":note["id"],"text":"Colleague"}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        expect_conflict_or_apply(&f, basis, "note.convertToText", json!({"story":note["story"],"id":note["id"]}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_places_frame_formatting_holds_story_text() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let frame = prepare(&f, &[("frame.create", json!({"rect":[20,200,300,400],"content":"text","text":"Original text"}))]).remove(0);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("story.replaceRange", json!({"story":frame["story"],"start":0,"end":0,"text":"Earlier "}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        let answer = f.call("run", json!({"basis":basis,"steps":[{"command":"type.char","on":[frame["id"]],"params":{"attrs":{"size":18}}}]})).await;
        if needed {
            assert_eq!(answer.as_ref().err().map(|e| &e["error"]["code"]), Some(&json!("conflict")), "{answer:?}");
        } else {
            answer.unwrap();
        }
        f.close().await;
    }
}

#[tokio::test]
async fn review_sources_copies_and_outlines_hold_story_content() {
    for command in ["transform.move", "edit.stepAndRepeat", "type.createOutlines"] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let frame = prepare(&f, &[("frame.create", json!({"rect":[20,200,300,400],"content":"text","text":"Original text"}))]).remove(0);
            let basis = f.basis().await;
            f.colleague(&[if needed {
                ("story.setText", json!({"story":frame["story"],"text":"Colleague text"}))
            } else {
                ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
            }]);
            let params = match command {
                "transform.move" => json!({"ids":[frame["id"]],"dx":10,"dy":0,"copy":true}),
                "edit.stepAndRepeat" => json!({"ids":[frame["id"]],"count":1,"dx":10}),
                _ => json!({"ids":[frame["id"]]}),
            };
            expect_conflict_or_apply(&f, basis, command, params, needed).await;
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review_sources_destructive_groups_hold_descendant_stories() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let frame = prepare(&f, &[("frame.create", json!({"rect":[20,200,300,400],"content":"text","text":"Keep colleague text"}))]).remove(0);
        let group = prepare(&f, &[("object.group", json!({"ids":[frame["id"],f.b]}))]).remove(0)["id"].clone();
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("story.setText", json!({"story":frame["story"],"text":"Colleague"}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        expect_conflict_or_apply(&f, basis, "edit.clear", json!({"ids":[group]}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_consumers_font_replacement() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        prepare(&f, &[("style.paragraph.create", json!({"name":"Font source","chars":{"fontFamily":"ReviewMissing","fontStyle":"Regular"}}))]);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("style.paragraph.edit", json!({"name":"Font source","chars":{"fontStyle":"Italic"}}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        expect_conflict_or_apply(&f, basis, "font.replace", json!({"family":"ReviewMissing","toFamily":"Inter"}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_consumers_path_text_removal() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let frame = prepare(&f, &[("line.create", json!({"a":[20,200],"b":[400,200]}))]).remove(0);
        let story = prepare(&f, &[("type.onPath", json!({"id":frame["id"],"text":"Do not discard"}))]).remove(0);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("story.setText", json!({"story":story["story"],"text":"Colleague"}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        expect_conflict_or_apply(&f, basis, "type.pathOptions", json!({"id":frame["id"],"delete":true}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_consumers_document_replacement() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("object.opacity", json!({"ids":[f.c],"opacity":0.25}))]);
    expect_conflict_or_apply(&f, basis, "document.applyTemplate", json!({"template":"event"}), true).await;
    f.close().await;
}

#[tokio::test]
async fn review_sources_threading_holds_both_stories() {
    for side in ["from", "to", "unthread"] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let frames = prepare(
                &f,
                &[
                    ("frame.create", json!({"rect":[20,200,300,400],"content":"text","text":"First"})),
                    ("frame.create", json!({"rect":[20,420,300,700],"content":"text","text":"Second"})),
                ],
            );
            if side == "unthread" {
                prepare(&f, &[("frame.thread", json!({"from":frames[0]["id"],"to":frames[1]["id"]}))]);
            }
            let basis = f.basis().await;
            f.colleague(&[if needed {
                ("story.setText", json!({"story":frames[usize::from(side == "to")]["story"],"text":"Colleague"}))
            } else {
                ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
            }]);
            let (command, params) = if side == "unthread" {
                ("frame.unthread", json!({"frame":frames[0]["id"]}))
            } else {
                ("frame.thread", json!({"from":frames[0]["id"],"to":frames[1]["id"]}))
            };
            expect_conflict_or_apply(&f, basis, command, params, needed).await;
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review_reflow_holds_used_style_based_on_definitions() {
    for command in ["type.createOutlines", "frame.fitText"] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let frame = prepare(
                &f,
                &[
                    ("frame.create", json!({"rect":[20,200,300,400],"content":"text","text":"Style composition"})),
                    ("style.paragraph.create", json!({"name":"Composition base","chars":{"size":12}})),
                    ("style.paragraph.create", json!({"name":"Composition","basedOn":"Composition base"})),
                    ("style.paragraph.create", json!({"name":"Unused","chars":{"size":12}})),
                ],
            )
            .remove(0);
            prepare(
                &f,
                &[("text.select", json!({"story":frame["story"],"anchor":0,"focus":17})), ("style.paragraph.apply", json!({"name":"Composition"}))],
            );
            let basis = f.basis().await;
            f.colleague(&[("style.paragraph.edit", json!({"name":if needed {"Composition base"} else {"Unused"},"chars":{"size":24}}))]);
            expect_conflict_or_apply(&f, basis, command, json!({"ids":[frame["id"]]}), needed).await;
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review_settings_hyperlink_content_is_held() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let link = prepare(&f, &[("hyperlink.create", json!({"ids":[f.a],"url":"https://one.example"}))]).remove(0)["ids"][0].clone();
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("hyperlink.edit", json!({"id":link,"url":"https://two.example"}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        expect_conflict_or_apply(&f, basis, "hyperlink.edit", json!({"id":link,"url":"https://mine.example"}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_names_style_application_holds_resolution_not_definition() {
    for change in ["rename", "redefine", "other-family", "other"] {
        let f = Fixture::new(Guard::Premise);
        let frame = prepare(
            &f,
            &[("frame.create", json!({"rect":[20,200,300,400],"content":"text","text":"Text"})), ("style.paragraph.create", json!({"name":"A"}))],
        )
        .remove(0);
        let basis = f.basis().await;
        f.colleague(&[match change {
            "rename" => ("style.paragraph.edit", json!({"name":"A","rename":"Gone"})),
            "redefine" => ("style.paragraph.edit", json!({"name":"A","chars":{"size":20}})),
            "other-family" => ("style.character.edit", json!({"name":"A","rename":"Other"})),
            _ => ("object.opacity", json!({"ids":[f.c],"opacity":0.25})),
        }]);
        let answer =
            f.call("run", json!({"basis":basis,"steps":[{"command":"style.paragraph.apply","on":[frame["id"]],"params":{"name":"A"}}]})).await;
        if change == "rename" {
            assert_eq!(answer.as_ref().err().map(|e| &e["error"]["code"]), Some(&json!("conflict")), "{answer:?}");
        } else {
            answer.unwrap_or_else(|e| panic!("{change}: {e}"));
        }
        f.close().await;
    }
}

#[tokio::test]
async fn review_settings_bookmark_index_holds_prefix() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        prepare(
            &f,
            &[("bookmark.add", json!({"name":"First"})), ("bookmark.add", json!({"name":"Second"})), ("bookmark.add", json!({"name":"Third"}))],
        );
        let basis = f.basis().await;
        f.colleague(&[if needed { ("bookmark.delete", json!({"index":0})) } else { ("object.opacity", json!({"ids":[f.c],"opacity":0.25})) }]);
        expect_conflict_or_apply(&f, basis, "bookmark.rename", json!({"index":1,"name":"Mine"}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_sources_master_override_holds_copied_story() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let source = prepare(
            &f,
            &[("frame.create", json!({"spread":{"kind":"parent","index":0},"rect":[20,20,300,200],"content":"text","text":"Master text"}))],
        )
        .remove(0);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("story.setText", json!({"story":source["story"],"text":"Colleague master"}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        expect_conflict_or_apply(&f, basis, "layout.overrideParentItems", json!({"page":0}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_tables_addressed_cells_and_windows() {
    for change in ["target", "other", "row-before", "row-after", "merged"] {
        let f = Fixture::new(Guard::Premise);
        let frame = prepare(&f, &[("frame.create", json!({"rect":[20,200,500,700],"content":"text","text":""}))]).remove(0);
        let table = prepare(
            &f,
            &[
                ("text.select", json!({"story":frame["story"],"anchor":0})),
                ("table.insert", json!({"rows":3,"cols":3,"headerRows":1,"footerRows":1})),
            ],
        )
        .remove(1);
        let address = json!({"story":frame["story"],"table":table["table"]});
        let mut theirs = address.clone();
        let command = match change {
            "row-before" | "row-after" => {
                theirs["row"] = json!(if change == "row-before" { 0 } else { 3 });
                theirs["where"] = json!("above");
                "table.insertRow"
            }
            "merged" => {
                theirs["rows"] = json!([0, 1]);
                theirs["cols"] = json!([0, 1]);
                "table.merge"
            }
            _ => {
                theirs["row"] = json!(if change == "target" { 1 } else { 4 });
                theirs["col"] = json!(1);
                theirs["text"] = json!("Colleague");
                "table.setCell"
            }
        };
        let basis = f.basis().await;
        f.colleague(&[(command, theirs)]);
        let mut mine = address;
        mine["row"] = json!(1);
        mine["col"] = json!(1);
        mine["text"] = json!("Mine");
        expect_conflict_or_apply(&f, basis, "table.setCell", mine, matches!(change, "target" | "row-before" | "merged")).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_tables_structural_scopes_and_story_anchor() {
    for (command, params) in [
        ("table.merge", json!({"rows":[0,1],"cols":[0,1]})),
        ("table.unmerge", json!({"row":0,"col":0})),
        ("table.insertRow", json!({"row":0})),
        ("table.insertColumn", json!({"col":0})),
        ("table.deleteRow", json!({"row":0})),
        ("table.deleteColumn", json!({"col":0})),
        ("table.convertToText", json!({})),
        ("table.delete", json!({})),
    ] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let frame = prepare(&f, &[("frame.create", json!({"rect":[20,200,500,700],"content":"text","text":"Before\nAfter"}))]).remove(0);
            let table =
                prepare(&f, &[("text.select", json!({"story":frame["story"],"anchor":7})), ("table.insert", json!({"rows":4,"cols":4}))]).remove(1);
            let mut mine = params.clone();
            mine["story"] = frame["story"].clone();
            mine["table"] = table["table"].clone();
            let anchor = matches!(command, "table.convertToText" | "table.delete");
            let edit = if anchor {
                if needed {
                    ("story.replaceRange", json!({"story":frame["story"],"start":0,"end":0,"text":"Earlier"}))
                } else {
                    ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
                }
            } else {
                let at = if needed { if matches!(command, "table.insertRow" | "table.insertColumn") { 1 } else { 0 } } else { 3 };
                if command == "table.unmerge" && needed {
                    ("table.merge", json!({"story":frame["story"],"table":table["table"],"rows":[0,1],"cols":[0,1]}))
                } else {
                    ("table.setCell", json!({"story":frame["story"],"table":table["table"],"row":at,"col":at,"text":"Colleague","fill":"Black"}))
                }
            };
            let basis = f.basis().await;
            f.colleague(&[edit]);
            expect_conflict_or_apply(&f, basis, command, mine, needed).await;
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review_receipt_all_offsets_are_stated() {
    let f = Fixture::new(Guard::Premise);
    let mut doc = f.disk();
    let mut ids = Vec::new();
    for n in 0..200 {
        ids.push(doc.execute("frame.create", json!({"rect":[10+n,100,20+n,110],"content":"unassigned"})).unwrap()["id"].clone());
    }
    std::fs::write(f.root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
    let receipt = f.write(f.basis().await, "transform.move", json!({"ids":ids,"dx":0.123456789,"dy":0})).await.unwrap();
    let changes = &receipt["changes"];
    assert!(changes.get("truncated").is_none(), "all 200 offsets must be stated: {changes}");
    let stated = changes["changed"].as_array().unwrap().len()
        + changes["footprint"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| entry["ids"].as_array().map_or_else(|| entry["count"].as_u64().unwrap_or(0) as usize, Vec::len))
            .sum::<usize>();
    assert_eq!(stated, 200, "{changes}");
    println!(
        "REVIEWRECEIPT bytes={} changes={} detailed={} footprints={} stated={stated}",
        receipt.to_string().len(),
        changes.to_string().len(),
        changes["changed"].as_array().unwrap().len(),
        changes["footprint"].as_array().map_or(0, Vec::len)
    );
    f.close().await;
}

#[tokio::test]
async fn review_names_cell_and_table_edits_hold_definition_content() {
    for family in ["cell", "table"] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let create = format!("style.{family}.create");
            let edit = format!("style.{family}.edit");
            prepare(&f, &[(&create, json!({"name":"Review"}))]);
            let basis = f.basis().await;
            let fields = if family == "cell" { json!({"name":"Review","fill":"Black"}) } else { json!({"name":"Review","border":{"weight":3}}) };
            f.colleague(&[if needed { (&edit, fields) } else { ("object.opacity", json!({"ids":[f.c],"opacity":0.25})) }]);
            let mine = if family == "cell" { json!({"name":"Review","fill":"Paper"}) } else { json!({"name":"Review","border":{"weight":5}}) };
            expect_conflict_or_apply(&f, basis, &edit, mine, needed).await;
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review_settings_condition_definition_is_a_premise() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        prepare(&f, &[("condition.new", json!({"name":"Review"}))]);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("condition.options", json!({"name":"Review","visible":false}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        expect_conflict_or_apply(&f, basis, "condition.options", json!({"name":"Review","visible":true}), needed).await;
        f.close().await;
    }
}

#[tokio::test]
async fn review_settings_named_registers_and_document_options() {
    let mut failures = Vec::new();
    for (command, initial, theirs, mine) in [
        (
            "variables.define",
            json!({"name":"Review","text":"Initial"}),
            json!({"name":"Review","text":"Theirs"}),
            json!({"name":"Review","text":"Mine"}),
        ),
        (
            "list.define",
            json!({"name":"Review"}),
            json!({"name":"Review","continueAcrossStories":false}),
            json!({"name":"Review","continueAcrossStories":true}),
        ),
        (
            "strokeStyle.new",
            json!({"name":"Review","type":"dot"}),
            json!({"name":"Review","type":"dash","pattern":[3,2]}),
            json!({"name":"Review","type":"dot"}),
        ),
        ("footnote.options", json!({"prefix":"Initial"}), json!({"prefix":"Theirs"}), json!({"prefix":"Mine"})),
        ("endnote.options", json!({"prefix":"Initial"}), json!({"prefix":"Theirs"}), json!({"prefix":"Mine"})),
    ] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            prepare(&f, &[(command, initial.clone())]);
            let basis = f.basis().await;
            f.colleague(&[if needed { (command, theirs.clone()) } else { ("object.opacity", json!({"ids":[f.c],"opacity":0.25})) }]);
            let answer = f.write(basis, command, mine.clone()).await;
            if needed != answer.as_ref().err().is_some_and(|e| e["error"]["code"] == "conflict") || !needed && answer.is_err() {
                failures.push(format!("{command} needed={needed}: {answer:?}"));
            }
            f.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn review_reflow_point_and_outlines_hold_earlier_thread_geometry() {
    for command in ["text.placeCaret", "type.createOutlines", "frame.fitText"] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let frames = prepare(
                &f,
                &[
                    ("frame.create", json!({"rect":[20,200,200,240],"content":"text","text":"Threaded words ".repeat(30)})),
                    ("frame.create", json!({"rect":[20,300,200,500],"content":"text","text":""})),
                ],
            );
            prepare(&f, &[("frame.thread", json!({"from":frames[0]["id"],"to":frames[1]["id"]}))]);
            let basis = f.basis().await;
            f.colleague(&[if needed {
                ("transform.resize", json!({"ids":[frames[0]["id"]],"from":[20,200,200,240],"to":[20,200,200,290]}))
            } else {
                ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
            }]);
            let answer = if command == "text.placeCaret" {
                f.call("run", json!({"basis":basis,"steps":[{"command":command,"params":{"frame":frames[1]["id"],"point":[25,315]}},{"command":"text.insert","params":{"text":"Mine"}}]})).await
            } else {
                f.write(basis, command, json!({"ids":[frames[1]["id"]]})).await
            };
            if needed {
                assert_eq!(answer.as_ref().err().map(|e| &e["error"]["code"]), Some(&json!("conflict")), "{command}: {answer:?}");
            } else {
                answer.unwrap_or_else(|e| panic!("{command}: {e}"));
            }
            f.close().await;
        }
    }
}
