//! Names are scoped by the engine's actual family, including grouped names.
use super::*;
use kition_kit::session::model::Exchange;

impl Fixture {
    fn prepare_names(&self, steps: &[(&str, Value)]) -> Vec<Value> {
        let mut doc = self.disk();
        let results = steps.iter().map(|(command, params)| doc.execute(command, params.clone()).unwrap()).collect();
        std::fs::write(self.root.join(FILE), doc.save(None).unwrap()).unwrap();
        results
    }
    fn named_address(&self, family: &str, name: &str) -> Value {
        let mut doc = self.disk();
        let mut exchange = Exchange::new();
        let root = exchange.batch(&mut doc, &json!({"ops":[],"load":[{"target":"layout","fields":[family]}]}), true).unwrap();
        let loads =
            root["loaded"][0]["fields"][family].as_array().unwrap().iter().map(|id| json!({"target":id,"fields":["name"]})).collect::<Vec<_>>();
        let names = exchange.batch(&mut doc, &json!({"ops":[],"load":loads}), true).unwrap();
        names["loaded"].as_array().unwrap().iter().find(|value| value["fields"]["name"] == name).unwrap()["id"].clone()
    }
    fn colleague_batch(&self, ops: Value) {
        let mut doc = self.disk();
        let before = doc.save(None).unwrap();
        Exchange::new().batch(&mut doc, &json!({"ops":ops}), false).unwrap();
        let path = self.root.join(FILE);
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let saved = doc.save(None).unwrap();
        assert_ne!(before, saved, "independent batch must really change the saved document");
        std::fs::write(&path, saved).unwrap();
        std::fs::File::options().write(true).open(path).unwrap().set_modified(modified - std::time::Duration::from_secs(3600)).unwrap();
    }
}

#[tokio::test]
async fn review_names_gradient_edit_consumes_current_definition() {
    for needed in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let gradient = f.prepare_names(&[("object.gradient", json!({"ids":[f.a]}))]).remove(0);
        let address = f.named_address("swatches", gradient["swatch"].as_str().unwrap());
        let basis = f.basis().await;
        if needed {
            let mut doc = f.disk();
            let mut value = Exchange::new().batch(&mut doc,&json!({"ops":[],"load":[{"target":address,"fields":["value"]}]}),true).unwrap()["loaded"][0]["fields"]["value"].clone();
            value["gradient"]["stops"][0]["opacity"] = json!(0.25);
            f.colleague_batch(json!([{"op":"set","target":address,"field":"value","value":value}]));
        } else {
            f.colleague(&[("object.opacity", json!({"ids":[f.c],"opacity":0.25}))]);
        }
        let answer = f.write(basis, "object.gradient", json!({"ids":[f.a],"reverse":true})).await;
        if needed {
            assert_eq!(answer.as_ref().err().map(|e| &e["error"]["code"]), Some(&json!("conflict")), "gradient definition must be held");
        } else {
            answer.unwrap();
        }
        f.close().await;
    }
}

#[tokio::test]
async fn review_names_all_style_families_hold_resolution() {
    for (family, list) in [
        ("paragraph", "paragraphStyles"),
        ("character", "characterStyles"),
        ("object", "objectStyles"),
        ("table", "tableStyles"),
        ("cell", "cellStyles"),
    ] {
        for change in ["rename", "delete", "definition", "grouped", "unrelated"] {
            let f = Fixture::new(Guard::Premise);
            let create = format!("style.{family}.create");
            let apply = format!("style.{family}.apply");
            let frame = f.prepare_names(&[("frame.create", json!({"rect":[20,200,500,700],"content":"text","text":"Name resolution"}))]).remove(0);
            f.prepare_names(&[(&create, json!({"name":"Review/Shared"})), (&create, json!({"name":"Other/Shared"}))]);
            let address = f.named_address(list, "Review/Shared");
            let other = f.named_address(list, "Other/Shared");
            let mut params = json!({"name":"Review/Shared"});
            if matches!(family, "table" | "cell") {
                let table = f
                    .prepare_names(&[("text.select", json!({"story":frame["story"],"anchor":0})), ("table.insert", json!({"rows":2,"cols":2}))])
                    .remove(1);
                params["story"] = frame["story"].clone();
                params["table"] = table["table"].clone();
            }
            let basis = f.basis().await;
            match change {
                "rename" => f.colleague_batch(json!([{"op":"set","target":address,"field":"name","value":"Gone"}])),
                "delete" => f.colleague_batch(json!([{"op":"delete","target":address}])),
                "definition" => {
                    let (key, value) = match family {
                        "paragraph" | "character" => ("size", json!(24)),
                        "object" => ("paragraphStyle", json!("[Basic Paragraph]")),
                        "table" => ("spaceBefore", json!(12)),
                        _ => ("fill", json!("Black")),
                    };
                    f.colleague_batch(json!([{"op":"map.set","target":address,"field":"attrs","key":key,"value":value}]));
                }
                "grouped" => f.colleague_batch(json!([{"op":"set","target":other,"field":"name","value":"Third/Shared"}])),
                _ => f.colleague(&[("object.opacity", json!({"ids":[f.c],"opacity":0.25}))]),
            }
            let mut step = json!({"command":apply,"params":params});
            if !matches!(family, "table" | "cell") {
                step["on"] = json!([frame["id"]]);
            }
            let answer = f.call("run", json!({"basis":basis,"steps":[step]})).await;
            if matches!(change, "rename" | "delete") {
                assert_eq!(answer.as_ref().err().map(|e| &e["error"]["code"]), Some(&json!("conflict")), "{family} {change}: {answer:?}");
            } else {
                answer.unwrap_or_else(|error| panic!("{family} {change}: {error}"));
            }
            f.close().await;
        }
    }
}
