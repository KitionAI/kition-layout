//! C8-layout review 3. Each stale write has a real colleague edit: an independent writer of the
//! file, or (`same_server`) another agent on the same server. Every behaviour has one colleague
//! change that must refuse and one unrelated change that must not.
use super::review2::{reasons, same_server};
use super::*;

fn prepare(f: &Fixture, steps: &[(&str, Value)]) -> Vec<Value> {
    let mut doc = f.disk();
    let results = steps.iter().map(|(id, params)| doc.execute(id, params.clone()).unwrap()).collect();
    std::fs::write(f.root.join(FILE), doc.save(None).unwrap()).unwrap();
    results
}

async fn colleague_by(f: &Fixture, outside: bool, steps: &[(&str, Value)]) {
    if outside {
        f.colleague(steps);
    } else {
        same_server(f, steps).await;
    }
}

fn xml_tag(f: &Fixture, id: &Value) -> String {
    let structure = f.disk().execute("xml.structure", json!({})).unwrap();
    structure["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["id"] == *id)
        .map_or(String::new(), |element| element["tag"].as_str().unwrap().to_owned())
}

#[tokio::test]
async fn review3_xml_delete_tag_holds_the_tagged_items() {
    // `xml.deleteTag` untags every item carrying the tag when it runs. An item another writer
    // tagged after the observation must refuse the write, not be untagged silently.
    for outside in [true, false] {
        for change in ["tags-another-item", "creates-a-tagged-item", "tags-with-another-name", "unrelated"] {
            let f = Fixture::new(Guard::Premise);
            prepare(&f, &[("xml.tag", json!({"tag":"story","ids":[f.a]})), ("xml.newTag", json!({"name":"other"}))]);
            let basis = f.basis().await;
            match change {
                "tags-another-item" => colleague_by(&f, outside, &[("xml.tag", json!({"tag":"story","ids":[f.c]}))]).await,
                "creates-a-tagged-item" => {
                    colleague_by(
                        &f,
                        outside,
                        &[("frame.add", json!({"rect":[400,400,440,440],"content":"unassigned"})), ("xml.tag", json!({"tag":"story"}))],
                    )
                    .await
                }
                "tags-with-another-name" => colleague_by(&f, outside, &[("xml.tag", json!({"tag":"other","ids":[f.c]}))]).await,
                _ => colleague_by(&f, outside, &[("object.opacity", json!({"ids":[f.c],"opacity":0.25}))]).await,
            }
            let answer = f.write(basis, "xml.deleteTag", json!({"name":"story"})).await;
            let needed = matches!(change, "tags-another-item" | "creates-a-tagged-item");
            assert_eq!(reasons(&answer).is_some(), needed, "outside={outside} {change}: {answer:?}");
            if needed {
                assert_eq!(xml_tag(&f, &f.a), "story", "outside={outside} {change}: a refused write changes nothing");
            } else {
                assert_eq!(xml_tag(&f, &f.a), "", "outside={outside} {change}");
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review3_compound_and_split_hold_ancestor_transforms() {
    // A cut point and a feather vector are spread coordinates mapped through every ancestor's
    // transform: another writer moving the group must refuse; a change elsewhere must not.
    for outside in [true, false] {
        for (command, params) in [("path.split", json!({"at":[40,20]})), ("object.gradientFeather", json!({"from":[20,40],"to":[60,40]}))] {
            for needed in [true, false] {
                let f = Fixture::new(Guard::Premise);
                let group = prepare(&f, &[("arrange.group", json!({"ids":[f.a,f.b]}))]).remove(0)["id"].clone();
                let basis = f.basis().await;
                colleague_by(
                    &f,
                    outside,
                    &[if needed {
                        ("transform.move", json!({"ids":[group],"dx":7,"dy":3}))
                    } else {
                        ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
                    }],
                )
                .await;
                let mut params = params.clone();
                if command == "path.split" {
                    params["id"] = f.a.clone();
                } else {
                    params["ids"] = json!([f.a, f.b]);
                }
                let answer = f.write(basis, command, params).await;
                assert_eq!(reasons(&answer).is_some(), needed, "outside={outside} {command} needed={needed}: {answer:?}");
                f.close().await;
            }
        }
    }
}

#[tokio::test]
async fn review3_toc_generate_holds_its_style_names() {
    // The TOC stores the paragraph style names it collects; a name another writer renamed away
    // (even one no paragraph uses yet) must refuse; another style's rename must not.
    for form in [json!({"styles":["Review"]}), json!({"entries":[{"style":"Review","level":1}]})] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            prepare(&f, &[("style.paragraph.add", json!({"name":"Review"})), ("style.paragraph.add", json!({"name":"Other"}))]);
            let basis = f.basis().await;
            f.colleague(&[if needed {
                ("style.paragraph.edit", json!({"name":"Review","rename":"Gone"}))
            } else {
                ("style.paragraph.edit", json!({"name":"Other","rename":"Elsewhere"}))
            }]);
            let answer = f.write(basis, "toc.generate", form.clone()).await;
            assert_eq!(reasons(&answer).is_some(), needed, "{form} needed={needed}: {answer:?}");
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review3_form_default_name_holds_the_names_it_avoids() {
    // A default form-field name is the first one no field has. Another writer's new field (or a
    // renamed one) changes which name that is: the write must refuse rather than pick a name the
    // agent did not see. An explicit name reads no other field.
    for outside in [true, false] {
        for (change, name, needed) in
            [("adds-a-field", None, true), ("adds-a-field", Some("Mine"), false), ("unrelated", None, false), ("renames-a-field", None, true)]
        {
            let f = Fixture::new(Guard::Premise);
            prepare(&f, &[("form.set", json!({"ids":[f.b],"kind":"textField","name":"Text Field 1"}))]);
            let basis = f.basis().await;
            match change {
                "adds-a-field" => colleague_by(&f, outside, &[("form.set", json!({"ids":[f.c],"kind":"textField"}))]).await,
                "renames-a-field" => colleague_by(&f, outside, &[("form.set", json!({"ids":[f.b],"kind":"textField","name":"Renamed"}))]).await,
                _ => colleague_by(&f, outside, &[("object.opacity", json!({"ids":[f.c],"opacity":0.25}))]).await,
            }
            let mut params = json!({"ids":[f.a],"kind":"textField"});
            if let Some(name) = name {
                params["name"] = json!(name);
            }
            let answer = f.write(basis, "form.set", params).await;
            assert_eq!(reasons(&answer).is_some(), needed, "outside={outside} {change} {name:?}: {answer:?}");
            f.close().await;
        }
    }
}

const PEOPLE: &str = "TmFtZSxDb2RlCkFkYSwxCg==";
const PLACES: &str = "TmFtZSxDaXR5CkJvLE9zbG8K";

#[tokio::test]
async fn review3_alternate_layouts_hold_their_pages_and_sources() {
    // Deleting an alternate layout deletes its pages and their items; creating one copies the
    // source pages' items. Another writer's frame on those pages must refuse; a new swatch not.
    for (command, needed, outside) in [
        ("layout.deleteAlternate", true, true),
        ("layout.deleteAlternate", true, false),
        ("layout.deleteAlternate", false, true),
        ("layout.addAlternate", true, true),
        ("layout.addAlternate", true, false),
        ("layout.addAlternate", false, true),
    ] {
        let f = Fixture::new(Guard::Premise);
        let first = if command == "layout.deleteAlternate" {
            prepare(&f, &[("layout.addAlternate", json!({"name":"Phone","width":300,"height":500}))]).remove(0)["firstPage"].clone()
        } else {
            json!(1)
        };
        let basis = f.basis().await;
        let spread = f.disk().inspect().unwrap()["spreads"].as_array().unwrap().len() - 1;
        colleague_by(
            &f,
            outside,
            &[if needed {
                if command == "layout.deleteAlternate" {
                    ("frame.add", json!({"rect":[20,20,60,60],"content":"unassigned","spread":spread}))
                } else {
                    ("edit.delete", json!({"ids":[f.a]}))
                }
            } else {
                ("swatch.add", json!({"name":"Unrelated","color":{"c":0,"m":0,"y":100,"k":0}}))
            }],
        )
        .await;
        let params = if command == "layout.deleteAlternate" { json!({"name":"Phone"}) } else { json!({"name":"Tablet","width":400,"height":600}) };
        let answer = f.write(basis, command, params).await;
        assert_eq!(reasons(&answer).is_some(), needed, "{command} (first page {first}) needed={needed} outside={outside}: {answer:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn review3_page_rule_for_all_pages_holds_every_page() {
    // Without `pages`, a page rule is written to every page there is when it runs: a page another
    // writer added, or a rule another writer set, must refuse; an unrelated change must not.
    for change in ["page-added", "rule-set", "unrelated"] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[match change {
            "page-added" => ("layout.pages.insert", json!({"count":1})),
            "rule-set" => ("liquid.pageRule", json!({"rule":"reCenter","pages":[3]})),
            _ => ("object.opacity", json!({"ids":[f.c],"opacity":0.25})),
        }]);
        let answer = f.write(basis, "liquid.pageRule", json!({"rule":"scale"})).await;
        assert_eq!(reasons(&answer).is_some(), change != "unrelated", "{change}: {answer:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn review3_data_merge_writes_hold_the_source() {
    // Removing the data source or inserting a field from it reads the source the agent saw:
    // another writer selecting another source must refuse; an unrelated change must not.
    for (command, needed) in
        [("data.source.remove", true), ("data.source.remove", false), ("data.placeholder.add", true), ("data.placeholder.add", false)]
    {
        let f = Fixture::new(Guard::Premise);
        prepare(&f, &[("data.source.select", json!({"bytes":PEOPLE,"name":"people.csv"}))]);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("data.source.select", json!({"bytes":PLACES,"name":"places.csv"}))
        } else {
            ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
        }]);
        let params = if command == "data.source.remove" { json!({}) } else { json!({"field":"Name","story":f.story}) };
        let answer = f.write(basis, command, params).await;
        assert_eq!(reasons(&answer).is_some(), needed, "{command} needed={needed}: {answer:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn review3_parent_guides_hold_the_parent_prefix() {
    // A guide on a parent spread named by its prefix: another writer swapping the prefixes of two
    // parents must refuse; a rename of an unrelated layer must not.
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        prepare(
            &f,
            &[
                ("layout.parents.new", json!({"prefix":"B","name":"Other"})),
                ("guide.add", json!({"spread":{"kind":"parent","index":0},"page":0,"orientation":"horizontal","position":100})),
                ("guide.add", json!({"spread":{"kind":"parent","index":1},"page":0,"orientation":"horizontal","position":200})),
            ],
        );
        let basis = f.basis().await;
        if needed {
            // Swap the two parents' prefixes in one save.
            let mut doc = f.disk();
            let mut exchange = kition_kit::session::model::Exchange::new();
            let root = exchange.batch(&mut doc, &json!({"ops":[],"load":[{"target":"layout","fields":["parents"]}]}), true).unwrap();
            let parents = root["loaded"][0]["fields"]["parents"].as_array().unwrap().clone();
            let loads: Vec<Value> = parents.iter().map(|id| json!({"target":id,"fields":["prefix"]})).collect();
            let loaded = exchange.batch(&mut doc, &json!({"ops":[],"load":loads}), true).unwrap();
            let by = |prefix: &str| loaded["loaded"].as_array().unwrap().iter().find(|p| p["fields"]["prefix"] == prefix).unwrap()["id"].clone();
            let (a, b) = (by("A"), by("B"));
            exchange
                .batch(
                    &mut doc,
                    &json!({"ops":[
                        {"op":"set","target":a,"field":"prefix","value":"B"},
                        {"op":"set","target":b,"field":"prefix","value":"A"}
                    ]}),
                    false,
                )
                .unwrap();
            std::fs::write(f.root.join(FILE), doc.save(None).unwrap()).unwrap();
        } else {
            f.colleague(&[("layer.set", json!({"id":f.layers[0],"name":"Renamed"}))]);
        }
        let answer = f.write(basis, "guide.liquid", json!({"spread":"A","page":0,"index":0})).await;
        assert_eq!(reasons(&answer).is_some(), needed, "needed={needed}: {answer:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn review3_long_text_offsets_hold_prefix_and_window() {
    // A story over 80 characters: change sets give it as an excerpt, and the kit finds the text
    // at the observation among the texts it held. An edit before or inside the range refuses;
    // one after it applies (it refused before review 3, as `place.unknown`).
    let long = "0123456789".repeat(12);
    for outside in [true, false] {
        for (change, at, needed) in [("before", 1, true), ("inside", 5, true), ("after", 100, false), ("just-after", 6, false)] {
            let f = Fixture::new(Guard::Premise);
            prepare(&f, &[("story.setText", json!({"story":f.story,"text":long}))]);
            let basis = f.basis().await;
            colleague_by(&f, outside, &[("story.replaceRange", json!({"story":f.story,"start":at,"end":at,"text":"Late"}))]).await;
            let answer = f.write(basis, "story.replaceRange", json!({"story":f.story,"start":4,"end":6,"text":"Mine"})).await;
            assert_eq!(reasons(&answer).is_some(), needed, "outside={outside} {change}: {answer:?}");
            if !needed {
                let text = f.disk().execute("story.get", json!({"story":f.story})).unwrap()["text"].as_str().unwrap().to_owned();
                assert!(text.starts_with("0123Mine") && text.contains("Late") && text.chars().count() == 126, "outside={outside} {change}: {text}");
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review3_long_repeated_text_is_not_guessed_as_later() {
    // An insertion into a long run of one character could have been made anywhere in the run.
    for outside in [true, false] {
        let f = Fixture::new(Guard::Premise);
        prepare(&f, &[("story.setText", json!({"story":f.story,"text":"a".repeat(120)}))]);
        let basis = f.basis().await;
        colleague_by(&f, outside, &[("story.replaceRange", json!({"story":f.story,"start":0,"end":0,"text":"a"}))]).await;
        let answer = f.write(basis, "story.replaceRange", json!({"story":f.story,"start":4,"end":6,"text":"Mine"})).await;
        assert!(reasons(&answer).is_some(), "outside={outside}: {answer:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn review3_layer_deletion_is_held_per_layer() {
    // The items a layer's deletion deletes are a place of that layer: another agent's item put
    // on it refuses (on this server too), one put on another layer does not (it refused while
    // the root's `~layerItems` was compared whole).
    for outside in [true, false] {
        for change in ["moved-onto-layer", "created-on-layer", "moved-onto-other-layer", "created-on-other-layer"] {
            let f = Fixture::new(Guard::Premise);
            let (layer, other) = (f.layers[1].clone(), f.layers[2].clone());
            prepare(&f, &[("object.setLayer", json!({"ids":[f.b],"layer":layer}))]);
            let basis = f.basis().await;
            let target = if change.ends_with("other-layer") { other } else { layer.clone() };
            colleague_by(
                &f,
                outside,
                &if change.starts_with("moved") {
                    vec![("object.setLayer", json!({"ids":[f.c],"layer":target}))]
                } else {
                    vec![("layer.activate", json!({"id":target})), ("frame.add", json!({"rect":[400,400,440,440],"content":"unassigned"}))]
                },
            )
            .await;
            let answer = f.write(basis, "layer.delete", json!({"id":layer})).await;
            let needed = !change.ends_with("other-layer");
            assert_eq!(reasons(&answer).is_some(), needed, "outside={outside} {change}: {answer:?}");
            f.close().await;
        }
    }
}

#[tokio::test]
async fn waiting_for_sdk_root_composition_frames_are_compared_whole() {
    // Waiting for the SDK (34dff06): a premise on `~compositionFrames.<story>` of the root is
    // touched by any change of the root's `~compositionFrames` (paths overlap; values at the
    // sub-path are not compared), and after a write on this server only the objects its change
    // set names are re-read, so the per-story value cannot be used. Moving another story's frame
    // therefore refuses fitting this one. Remove this pin when the SDK compares derived values at
    // a premise's sub-path or lets a kit name the objects whose derived values a change affects.
    for outside in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let frame = prepare(&f, &[("frame.add", json!({"rect":[20,20,250,200],"content":"text","text":"Words to fit."}))]).remove(0);
        let basis = f.basis().await;
        let other =
            f.disk().inspect().unwrap()["stories"].as_array().unwrap().iter().find(|story| story["id"] == f.story).unwrap()["frames"][0].clone();
        colleague_by(&f, outside, &[("transform.move", json!({"ids":[other],"dx":0,"dy":20}))]).await;
        let answer = f.write(basis, "frame.fitText", json!({"ids":[frame["id"]]})).await;
        assert!(reasons(&answer).is_some(), "waiting for the SDK: this false refusal is expected at 34dff06 (outside={outside}): {answer:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn review3_selected_placeholder_holds_the_selected_story() {
    // A field inserted at the text selection rests on the selected story's text.
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        prepare(&f, &[("data.source.select", json!({"bytes":PEOPLE,"name":"people.csv"}))]);
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
                    {"command":"text.select","params":{"story":f.story,"anchor":4,"focus":4}},
                    {"command":"data.placeholder.add","params":{"field":"Name"}}
                ]}),
            )
            .await;
        assert_eq!(reasons(&answer).is_some(), needed, "needed={needed}: {answer:?}");
        f.close().await;
    }
}
