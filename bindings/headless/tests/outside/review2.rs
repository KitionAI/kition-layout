//! C8-layout review 2. Each stale write has a real colleague edit: an independent writer of the
//! file, or (`same_server`) another agent on the same server.
use super::*;

/// Another agent on the same server runs `steps` and saves.
pub(crate) async fn same_server(f: &Fixture, steps: &[(&str, Value)]) {
    let steps: Vec<Value> = steps.iter().map(|(command, params)| json!({"command":command,"params":params})).collect();
    let answer = tools::call_as(&f.kits, "layout_run", &json!({"path":FILE,"steps":steps}), f.options, "c8-colleague").await.unwrap();
    answer.unwrap_or_else(|error| panic!("same-server colleague: {}", error.structured()));
}

async fn colleague_by(f: &Fixture, outside: bool, steps: &[(&str, Value)]) {
    if outside {
        f.colleague(steps);
    } else {
        same_server(f, steps).await;
    }
}

fn story_text(f: &Fixture, story: &Value) -> String {
    f.disk().execute("story.get", json!({"story":story})).unwrap()["text"].as_str().unwrap().to_owned()
}

/// What a refusal says, `None` when the write applied.
pub(crate) fn reasons(answer: &Result<Value, Value>) -> Option<Vec<String>> {
    let error = answer.as_ref().err()?;
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    Some(error["conflicts"].as_array().unwrap().iter().map(|conflict| conflict["reason"].as_str().unwrap().to_owned()).collect())
}

#[tokio::test]
async fn review2_text_offset_rests_on_prefix_and_window_not_on_later_text() {
    // story "0123456789"; the write replaces bytes [4,6) with "Mine".
    for outside in [true, false] {
        for (change, colleague, expected) in [
            ("insert-before", json!({"start":0,"end":0,"text":"Early"}), Err("relocated")),
            ("delete-before", json!({"start":1,"end":2,"text":""}), Err("relocated")),
            ("replace-before-same-length", json!({"start":1,"end":2,"text":"x"}), Err("relocated")),
            // Inside the window is `changed` by the rule; at SDK 34dff06 a place told by `covers`
            // can only answer `relocated` or `deleted`, so the refusal says `relocated`.
            ("inside", json!({"start":4,"end":5,"text":"X"}), Err("relocated")),
            ("straddles-start", json!({"start":3,"end":5,"text":""}), Err("relocated")),
            ("insert-at-end-of-range", json!({"start":6,"end":6,"text":"X"}), Ok("0123MineX6789")),
            ("insert-after", json!({"start":9,"end":9,"text":"Late"}), Ok("0123Mine678Late9")),
            ("delete-after", json!({"start":8,"end":10,"text":""}), Ok("0123Mine67")),
        ] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            let mut params = colleague.clone();
            params["story"] = f.story.clone();
            colleague_by(&f, outside, &[("story.replaceRange", params)]).await;
            let answer = f.write(basis, "story.replaceRange", json!({"story":f.story,"start":4,"end":6,"text":"Mine"})).await;
            let said = reasons(&answer);
            match expected {
                Ok(text) => {
                    assert_eq!(said, None, "outside={outside} {change}: {answer:?}");
                    assert_eq!(story_text(&f, &f.story), text, "outside={outside} {change}");
                }
                Err(reason) => {
                    let said = said.unwrap_or_else(|| panic!("outside={outside} {change} must refuse: {answer:?}"));
                    assert!(said.iter().any(|said| said == reason), "outside={outside} {change}: wanted {reason}, got {said:?}");
                }
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review2_text_offset_repeated_text_is_not_guessed_as_later() {
    // "aaaaaaaaaa" with one more "a" inserted at 0: a text comparison alone would place the
    // insertion at the end. Where it was is not known, so the write must refuse.
    for outside in [true, false] {
        let f = Fixture::new(Guard::Premise);
        prepare_text(&f, "aaaaaaaaaa");
        let basis = f.basis().await;
        colleague_by(&f, outside, &[("story.replaceRange", json!({"story":f.story,"start":0,"end":0,"text":"a"}))]).await;
        let answer = f.write(basis, "story.replaceRange", json!({"story":f.story,"start":4,"end":6,"text":"Mine"})).await;
        assert!(reasons(&answer).is_some(), "outside={outside}: {answer:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn review2_other_offset_writes_rest_on_prefix_and_window() {
    for (command, params) in
        [("spelling.change", json!({"start":4,"end":6,"to":"Mine"})), ("story.replaceRange", json!({"start":5,"end":5,"text":"Mine"}))]
    {
        for (before, needed) in [(true, true), (false, false)] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            f.colleague(&[(
                "story.replaceRange",
                json!({"story":f.story,"start":if before { 0 } else { 9 },"end":if before { 0 } else { 9 },"text":"Theirs"}),
            )]);
            let mut params = params.clone();
            params["story"] = f.story.clone();
            let answer = f.write(basis, command, params).await;
            assert_eq!(reasons(&answer).is_some(), needed, "{command} before={before}: {answer:?}");
            f.close().await;
        }
    }
}

fn prepare_text(f: &Fixture, text: &str) {
    let mut doc = f.disk();
    doc.execute("story.setText", json!({"story":f.story,"text":text})).unwrap();
    std::fs::write(f.root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
}

#[tokio::test]
async fn review2_text_offset_unlocatable_changes_refuse() {
    // A change set gives formatting not at all: where such a change was cannot be told, so even a
    // later one refuses (`place.unknown`). A text over 80 characters is given as an excerpt; since
    // review 3 the kit finds the observed text among those it held.
    for case in ["long-text-after", "formatting-after"] {
        let f = Fixture::new(Guard::Premise);
        if case == "long-text-after" {
            prepare_text(&f, &"0123456789".repeat(12));
        }
        let basis = f.basis().await;
        if case == "long-text-after" {
            f.colleague(&[("story.replaceRange", json!({"story":f.story,"start":100,"end":100,"text":"Late"}))]);
        } else {
            f.colleague(&[("text.select", json!({"story":f.story,"anchor":8,"focus":10})), ("type.char", json!({"attrs":{"size":20}}))]);
        }
        let answer = f.write(basis, "story.replaceRange", json!({"story":f.story,"start":4,"end":6,"text":"Mine"})).await;
        if case == "long-text-after" {
            // C8-layout review 3: the text at the observation is found among the texts the kit
            // held, so a later edit of a long story is located and applies.
            assert_eq!(reasons(&answer), None, "{case}: {answer:?}");
            f.close().await;
            continue;
        }
        let error = answer.as_ref().err().unwrap_or_else(|| panic!("{case} must refuse: {answer:?}"));
        assert!(error["conflicts"].as_array().unwrap().iter().any(|c| c["place"]["unknown"].is_string()), "{case}: {error}");
        f.close().await;
    }
}

#[tokio::test]
async fn review2_scanner_places_hold_counted_pages_and_spreads() {
    // Found by the structural integer scanner: a button's target page and a creation's spread
    // are counted positions. A page inserted before them must refuse; an unrelated edit applies.
    for (command, params) in [
        ("button.set", json!({"action":"page","page":3})),
        ("frame.create", json!({"rect":[20,20,60,60],"content":"unassigned","spread":3})),
        ("liquid.pageRule", json!({"rule":"scale","pages":[4]})),
        ("page.background", json!({"color":"#102030","pages":[4]})),
    ] {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            f.colleague(&[if needed {
                ("layout.pages.insert", json!({"after":0,"count":1}))
            } else {
                ("object.opacity", json!({"ids":[f.c],"opacity":0.25}))
            }]);
            let mut params = params.clone();
            if command == "button.set" {
                params["ids"] = json!([f.a]);
            }
            let answer = f.write(basis, command, params).await;
            assert_eq!(reasons(&answer).is_some(), needed, "{command} needed={needed}: {answer:?}");
            f.close().await;
        }
    }
}

/// The address of the `family` entry named `name`, as the object model lists it.
fn address_of(f: &Fixture, family: &str, name: &str) -> Value {
    let mut doc = f.disk();
    let mut exchange = kition_kit::session::model::Exchange::new();
    let root = exchange.batch(&mut doc, &json!({"ops":[],"load":[{"target":"layout","fields":[family]}]}), true).unwrap();
    let loads = root["loaded"][0]["fields"][family].as_array().unwrap().iter().map(|id| json!({"target":id,"fields":["name"]})).collect::<Vec<_>>();
    let names = exchange.batch(&mut doc, &json!({"ops":[],"load":loads}), true).unwrap();
    names["loaded"].as_array().unwrap().iter().find(|value| value["fields"]["name"] == name).unwrap()["id"].clone()
}

/// An independent writer runs a model batch and saves.
fn colleague_ops(f: &Fixture, ops: Value) {
    let mut doc = f.disk();
    kition_kit::session::model::Exchange::new().batch(&mut doc, &json!({"ops":ops}), false).unwrap();
    std::fs::write(f.root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
}

#[tokio::test]
async fn review2_identical_spelling_in_another_folder_refuses_a_write_by_name() {
    // Another writer gives a second swatch (in a colour group) or a second paragraph style the
    // spelling a write names: the write must refuse, never pick one of the two.
    for (family, command, params) in
        [("swatches", "object.fill", json!({"swatch":"Brand"})), ("paragraphStyles", "style.paragraph.apply", json!({"name":"Head"}))]
    {
        for needed in [true, false] {
            let f = Fixture::new(Guard::Premise);
            let mut doc = f.disk();
            if family == "swatches" {
                doc.execute("swatch.create", json!({"name":"Brand","color":{"c":70,"m":75,"y":0,"k":0}})).unwrap();
                doc.execute("swatch.create", json!({"name":"Other","color":{"c":0,"m":75,"y":70,"k":0}})).unwrap();
                doc.execute("swatch.newColorGroup", json!({"name":"Folder","swatches":["Other"]})).unwrap();
            } else {
                doc.execute("style.paragraph.create", json!({"name":"Head"})).unwrap();
                doc.execute("style.paragraph.create", json!({"name":"Other"})).unwrap();
            }
            std::fs::write(f.root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
            let other = address_of(&f, family, "Other");
            let basis = f.basis().await;
            let spelled = if family == "swatches" { "Brand" } else { "Head" };
            colleague_ops(&f, json!([{"op":"set","target":other,"field":"name","value":if needed { spelled } else { "Unrelated" }}]));
            let mut params = params.clone();
            let steps = if family == "swatches" {
                params["ids"] = json!([f.a]);
                json!([{"command":command,"params":params}])
            } else {
                json!([{"command":"text.select","params":{"story":f.story,"anchor":0,"focus":4}},{"command":command,"params":params}])
            };
            let answer = f.call("run", json!({"basis":basis,"steps":steps})).await;
            println!("IDENTICAL {family} needed={needed}: {answer:?}");
            if needed {
                let error = answer.as_ref().expect_err("an ambiguous name must refuse");
                assert!(
                    error["error"]["code"] == "conflict" || error["error"]["code"] == "invalid-params" || error["error"]["code"] == "command-failed",
                    "{family}: {error}"
                );
                let disk = f.disk().inspect().unwrap();
                assert_ne!(disk["spreads"][0]["items"][0]["fill"], "Brand", "{family}: nothing may be written");
            } else {
                answer.unwrap_or_else(|error| panic!("{family} unrelated: {error}"));
            }
            f.close().await;
        }
    }
}

#[tokio::test]
async fn review2_identical_spelling_already_observed_refuses_a_write_by_name() {
    // Two definitions already share the spelling when the agent observes: a write by that name
    // cannot say which one it means and must refuse rather than pick the first.
    for (family, command, params) in
        [("swatches", "object.fill", json!({"swatch":"Brand"})), ("paragraphStyles", "style.paragraph.apply", json!({"name":"Head"}))]
    {
        let f = Fixture::new(Guard::Premise);
        let mut doc = f.disk();
        if family == "swatches" {
            doc.execute("swatch.create", json!({"name":"Brand","color":{"c":70,"m":75,"y":0,"k":0}})).unwrap();
            doc.execute("swatch.create", json!({"name":"Other","color":{"c":0,"m":75,"y":70,"k":0}})).unwrap();
            doc.execute("swatch.newColorGroup", json!({"name":"Folder","swatches":["Other"]})).unwrap();
        } else {
            doc.execute("style.paragraph.create", json!({"name":"Head"})).unwrap();
            doc.execute("style.paragraph.create", json!({"name":"Other"})).unwrap();
        }
        std::fs::write(f.root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
        let other = address_of(&f, family, "Other");
        colleague_ops(&f, json!([{"op":"set","target":other,"field":"name","value":if family == "swatches" { "Brand" } else { "Head" }}]));
        let basis = f.basis().await;
        let mut params = params.clone();
        let steps = if family == "swatches" {
            params["ids"] = json!([f.a]);
            json!([{"command":command,"params":params}])
        } else {
            json!([{"command":"text.select","params":{"story":f.story,"anchor":0,"focus":4}},{"command":command,"params":params}])
        };
        let answer = f.call("run", json!({"basis":basis,"steps":steps})).await;
        println!("IDENTICALBASIS {family}: {answer:?}");
        assert!(answer.is_err(), "{family}: an ambiguous name must refuse: {answer:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn review2_align_to_page_holds_the_page_it_reads() {
    // Aligning to the margins reads the page under the objects: another writer's margins there
    // must refuse; a change elsewhere must not.
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("layout.marginsAndColumns", json!({"pages":[0],"margins":72}))
        } else {
            ("layout.marginsAndColumns", json!({"pages":[4],"margins":72}))
        }]);
        let answer = f.write(basis, "object.align", json!({"edge":"left","to":"margins","ids":[f.a]})).await;
        assert_eq!(reasons(&answer).is_some(), needed, "needed={needed}: {answer:?}");
        f.close().await;
    }
}
