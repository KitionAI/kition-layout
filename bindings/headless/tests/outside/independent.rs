//! C8-layout-read: regressions chosen from the independent handler readings.
use super::*;

#[tokio::test]
async fn independent_root_settings_global_light() {
    root_setting("object.globalLight").await;
}

#[tokio::test]
async fn independent_root_settings_primary_story() {
    root_setting("object.primaryTextFrame").await;
}

#[tokio::test]
async fn independent_editor_selection_context_follows_the_selected_story() {
    for needed in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let inspected = f.disk().inspect().unwrap();
        let frame = inspected["spreads"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|spread| spread["items"].as_array().unwrap())
            .find(|item| item["story"] == f.story)
            .unwrap()["id"]
            .clone();
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("story.setDirection", json!({"story":f.story,"direction":"rightToLeft"}))
        } else {
            ("object.rename", json!({"id":f.b,"name":"Colleague"}))
        }]);
        let answer = f
            .call(
                "run",
                json!({"basis":basis,"steps":[{
                    "command":"story.setDirection","on":[frame],"params":{"direction":"leftToRight"}
                }]}),
            )
            .await;
        if needed {
            conflict(&answer.unwrap_err(), "changed");
        } else {
            answer.unwrap();
            assert_eq!(f.disk().execute("story.get", json!({"story":f.story})).unwrap()["direction"], "leftToRight");
        }
        f.close().await;
    }
}

async fn root_setting(command: &str) {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let inspect = f.disk().inspect().unwrap();
        let frame = inspect["spreads"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|spread| spread["items"].as_array().unwrap())
            .find(|item| item["story"] == f.story)
            .unwrap()["id"]
            .clone();
        let basis = f.basis().await;
        let params = if command == "object.globalLight" { json!({"angle":80}) } else { json!({"ids":[frame]}) };
        let colleague = if needed {
            if command == "object.globalLight" { (command, json!({"angle":50})) } else { (command, json!({"ids":[frame],"on":true})) }
        } else {
            ("object.rename", json!({"id":f.b,"name":"Colleague"}))
        };
        f.colleague(&[colleague]);
        let answer = f.write(basis, command, params).await;
        if needed {
            conflict(&answer.unwrap_err(), "changed");
        } else {
            let receipt = answer.unwrap();
            let result = &receipt["completed"][0]["result"];
            if command == "object.globalLight" {
                assert_eq!(result["angle"], 80.0);
            } else {
                assert_eq!(result["primary"], true);
            }
            assert_eq!(f.disk().execute("object.findByLabel", json!({"label":"unmatched"})).unwrap(), json!({"ids":[]}));
            let saved = f.disk().inspect().unwrap();
            assert!(
                saved["spreads"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|spread| { spread["items"].as_array().unwrap().iter().any(|item| item["id"] == f.b && item["name"] == "Colleague") })
            );
        }
        f.close().await;
    }
}
