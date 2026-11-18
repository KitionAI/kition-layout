//! C8-layout-read-2: the source resolver and the guard must name the same cell.
use super::*;

#[tokio::test]
async fn read2_cell_style_source_holds_the_resolved_cell() {
    for change in ["source-fill", "other-cell", "source-text", "other-item"] {
        let needed = change == "source-fill";
        let f = Fixture::new(Guard::Premise);
        let mut doc = f.disk();
        doc.execute("text.select", json!({"story":f.story,"anchor":0})).unwrap();
        let table = doc.execute("table.insert", json!({"rows":2,"cols":2})).unwrap()["table"].clone();
        std::fs::write(f.root.join(FILE), doc.save(None).unwrap()).unwrap();
        let address = json!({"story":f.story,"table":table,"row":1,"col":1});
        let basis = f.basis().await;
        f.colleague(&[match change {
            "source-fill" => ("table.setCell", json!({"story":f.story,"table":table,"row":1,"col":1,"fill":"Black"})),
            "other-cell" => ("table.setCell", json!({"story":f.story,"table":table,"row":0,"col":0,"fill":"Black"})),
            "source-text" => ("table.setCell", json!({"story":f.story,"table":table,"row":1,"col":1,"text":"Colleague"})),
            _ => ("object.rename", json!({"id":f.b,"name":"Colleague"})),
        }]);
        let mut params = json!({"name":"From Cell","fromSelection":true});
        params.as_object_mut().unwrap().extend(address.as_object().unwrap().clone());
        let answer = f.write(basis, "style.cell.create", params).await;
        if needed {
            assert!(answer.is_err(), "changed source must refuse: {answer:?}");
            conflict(&answer.unwrap_err(), "changed");
        } else {
            assert!(answer.is_ok(), "unrelated {change} must apply: {answer:?}");
        }
        f.close().await;
    }
}

// The name stays: `cases/regressions/issue-c8-layout-read-2-step-source` runs this test by it.
#[tokio::test]
async fn waiting_for_sdk_read2_selection_step_source_premises() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let mut doc = f.disk();
        doc.execute("text.select", json!({"story":f.story,"anchor":0})).unwrap();
        let table = doc.execute("table.insert", json!({"rows":2,"cols":2})).unwrap()["table"].clone();
        std::fs::write(f.root.join(FILE), doc.save(None).unwrap()).unwrap();
        let basis = f.basis().await;
        f.colleague(&[if needed {
            ("table.setCell", json!({"story":f.story,"table":table,"row":1,"col":1,"fill":"Black"}))
        } else {
            ("object.rename", json!({"id":f.b,"name":"Colleague"}))
        }]);
        let answer = f
            .call(
                "run",
                json!({"basis":basis,"steps":[
                    {"command":"table.select","params":{"story":f.story,"table":table,"row":1,"col":1}},
                    {"command":"style.cell.create","params":{"name":"From Cell","fromSelection":true}}
                ]}),
            )
            .await;
        // kition-sdk 2ce294a1 asks the second step's premises on a copy after the first step
        // ran, so the cell it selected is the source the guard holds.
        if needed {
            conflict(&answer.unwrap_err(), "changed");
        } else {
            assert!(answer.is_ok(), "an unrelated change must apply: {answer:?}");
            let styles = f.disk().execute("style.list", json!({})).unwrap();
            assert!(styles["cell"].as_array().unwrap().contains(&json!("From Cell")), "the dependent command ran: {styles}");
        }
        f.close().await;
    }
}
