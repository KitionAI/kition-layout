//! C8-layout review 2: derived premises (`~compositionFrames`, `~pageItems`, `~layerItems`)
//! against another agent on the same server, which the SDK re-reads only for named objects.
use super::review2::{reasons, same_server};
use super::*;

#[tokio::test]
async fn review2_same_server_derived_composition_frames() {
    // Another agent on this server moves a wrapping object into the frame's area.
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let frame = prepare_steps(
            &f,
            &[(
                "frame.create",
                json!({"rect":[20,20,250,200],"content":"text","text":"Words that fit around a wrapping object and occupy several lines."}),
            )],
        )
        .remove(0);
        prepare_steps(&f, &[("object.textWrap", json!({"ids":[f.c],"mode":"bounds"}))]);
        let basis = f.basis().await;
        same_server(
            &f,
            &[if needed {
                ("transform.move", json!({"ids":[f.c],"dx":-250,"dy":-260}))
            } else {
                ("object.opacity", json!({"ids":[f.a],"opacity":0.25}))
            }],
        )
        .await;
        let answer = f.write(basis, "frame.fitText", json!({"ids":[frame["id"]]})).await;
        assert_eq!(reasons(&answer).is_some(), needed, "needed={needed}: {answer:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn review2_same_server_derived_page_items() {
    // Another agent on this server puts an item on the page about to be deleted.
    for change in ["created-on-page", "created-elsewhere"] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        same_server(
            &f,
            &[if change == "created-on-page" {
                ("frame.create", json!({"rect":[400,400,440,440],"content":"unassigned"}))
            } else {
                ("frame.create", json!({"rect":[100,100,140,140],"content":"unassigned","spread":4}))
            }],
        )
        .await;
        let answer = f.write(basis, "layout.pages.delete", json!({"pages":[0]})).await;
        assert_eq!(reasons(&answer).is_some(), change == "created-on-page", "{change}: {answer:?}");
        f.close().await;
    }
}

#[tokio::test]
async fn review2_same_server_derived_layer_items() {
    // Another agent on this server moves an item onto the layer about to be deleted.
    for change in ["moved-onto-layer", "created-on-layer", "unrelated"] {
        let f = Fixture::new(Guard::Premise);
        let layer = f.layers[1].clone();
        prepare_steps(&f, &[("object.setLayer", json!({"ids":[f.b],"layer":layer}))]);
        let basis = f.basis().await;
        same_server(
            &f,
            &match change {
                "moved-onto-layer" => vec![("object.setLayer", json!({"ids":[f.c],"layer":layer}))],
                "created-on-layer" => {
                    vec![("layer.activate", json!({"id":layer})), ("frame.create", json!({"rect":[400,400,440,440],"content":"unassigned"}))]
                }
                _ => vec![("object.opacity", json!({"ids":[f.c],"opacity":0.25}))],
            },
        )
        .await;
        let answer = f.write(basis, "layer.delete", json!({"id":layer})).await;
        assert_eq!(reasons(&answer).is_some(), change != "unrelated", "{change}: {answer:?}");
        f.close().await;
    }
}

fn prepare_steps(f: &Fixture, steps: &[(&str, Value)]) -> Vec<Value> {
    let mut doc = f.disk();
    let results = steps.iter().map(|(id, params)| doc.execute(id, params.clone()).unwrap()).collect();
    std::fs::write(f.root.join(FILE), doc.save(None).unwrap()).unwrap();
    results
}

#[tokio::test]
async fn review2_derived_composition_departures_and_other_stories() {
    // A wrap object leaving the frame or deleted changes the composition too.
    for outside in [true, false] {
        for change in ["wrap-moved-away", "wrap-deleted", "other-story-frame-moved"] {
            let f = Fixture::new(Guard::Premise);
            let frame = prepare_steps(
                &f,
                &[(
                    "frame.create",
                    json!({"rect":[20,20,250,200],"content":"text","text":"Words that fit around a wrapping object and occupy several lines."}),
                )],
            )
            .remove(0);
            prepare_steps(&f, &[("object.textWrap", json!({"ids":[f.b],"mode":"bounds"}))]);
            let basis = f.basis().await;
            let other =
                f.disk().inspect().unwrap()["stories"].as_array().unwrap().iter().find(|story| story["id"] == f.story).unwrap()["frames"][0].clone();
            let step = match change {
                "wrap-moved-away" => ("transform.move", json!({"ids":[f.b],"dx":300,"dy":300})),
                "wrap-deleted" => ("edit.clear", json!({"ids":[f.b]})),
                _ => ("transform.move", json!({"ids":[other],"dx":0,"dy":20})),
            };
            if outside {
                f.colleague(&[step]);
            } else {
                same_server(&f, &[step]).await;
            }
            let answer = f.write(basis, "frame.fitText", json!({"ids":[frame["id"]]})).await;
            // `other-story-frame-moved` is not necessary, but the root's `~compositionFrames`
            // is compared whole: any story's composition change refuses (a known false refusal,
            // the price of detecting the others on this server at SDK 34dff06).
            assert!(reasons(&answer).is_some(), "outside={outside} {change}: {answer:?}");
            f.close().await;
        }
    }
}
