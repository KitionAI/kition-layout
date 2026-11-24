//! C8-land-13-layout: a separate writer really opens and rewrites the file.
//! Mixed-table necessary rules are fixed in `necessary`, before any test is run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions, Guard};
use kition_kits::{KitDefinition, KitManager};
use kition_layout_kit::LayoutDocument;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "outside/independent.rs"]
mod independent;
#[path = "outside/independent_layers.rs"]
mod independent_layers;
#[path = "outside/independent_settings.rs"]
mod independent_settings;
#[path = "outside/names.rs"]
mod names;
#[path = "outside/read2.rs"]
mod read2;
#[path = "outside/read3.rs"]
mod read3;
#[path = "outside/review.rs"]
mod review;
#[path = "outside/review2.rs"]
mod review2;
#[path = "outside/review2_derived.rs"]
mod review2_derived;
#[path = "outside/review3.rs"]
mod review3;

const FILE: &str = "Outside.klayout";
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    kits: KitManager,
    root: PathBuf,
    options: CallOptions,
    a: Value,
    b: Value,
    c: Value,
    story: Value,
    layers: Vec<Value>,
}
impl Fixture {
    fn new(guard: Guard) -> Self {
        let root = std::env::temp_dir().join(format!("kition-layout-c8-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let mut doc = LayoutDocument::create(FILE, 612, 792).unwrap();
        let mut frames = Vec::new();
        for (n, rect) in [[20, 20, 60, 60], [100, 20, 140, 60], [300, 300, 340, 340]].into_iter().enumerate() {
            let id = doc.execute("frame.create", json!({"rect":rect,"content":"unassigned"})).unwrap()["id"].clone();
            doc.execute("object.rename", json!({"id":id,"name":format!("F{n}")})).unwrap();
            frames.push(id);
        }
        for name in ["A", "B"] {
            doc.execute("style.character.create", json!({"name":name,"chars":{"size":12}})).unwrap();
        }
        for name in ["L1", "L2", "L3"] {
            doc.execute("layer.new", json!({"name":name})).unwrap();
        }
        doc.execute("layout.pages.insert", json!({"count":5})).unwrap();
        let story = doc.execute("frame.create", json!({"rect":[20,500,300,700],"content":"text","text":"0123456789"})).unwrap()["story"].clone();
        let layers = doc.inspect().unwrap()["layers"].as_array().unwrap().iter().map(|layer| layer["id"].clone()).collect();
        std::fs::write(root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
        let kit = KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(root.clone()), vec![(kit, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-layout"))))]);
        Self {
            kits,
            root,
            options: CallOptions { guard, ..Default::default() },
            a: frames[0].clone(),
            b: frames[1].clone(),
            c: frames[2].clone(),
            story,
            layers,
        }
    }
    fn disk(&self) -> LayoutDocument {
        LayoutDocument::open(FILE, &std::fs::read(self.root.join(FILE)).unwrap()).unwrap()
    }
    fn colleague(&self, steps: &[(&str, Value)]) {
        let path = self.root.join(FILE);
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let mut doc = self.disk();
        for (command, params) in steps {
            doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("colleague {command}: {error}"));
        }
        std::fs::write(&path, doc.seal_and_save(None).unwrap()).unwrap();
        std::fs::File::options().write(true).open(path).unwrap().set_modified(modified - std::time::Duration::from_secs(3600)).unwrap();
    }
    async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("layout_{action}"), &input, self.options, "c8-agent").await {
            Some(Ok(answer)) => Ok(answer.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("missing layout_{action}"),
        }
    }
    async fn basis(&self) -> Value {
        self.call("inspect", json!({})).await.unwrap()["revision"].clone()
    }
    async fn write(&self, basis: Value, command: &str, params: Value) -> Result<Value, Value> {
        self.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params}]})).await
    }
    async fn close(self) {
        self.kits.shutdown().await;
        std::fs::remove_dir_all(self.root).unwrap();
    }
}
fn conflict(error: &Value, reason: &str) {
    assert_eq!(error["error"]["code"], "conflict", "{error}");
    assert!(error["conflicts"].as_array().unwrap().iter().any(|conflict| conflict["reason"] == reason), "{error}");
}

#[tokio::test]
async fn unrelated_field_applies_and_others_names_only_that_field() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("object.opacity", json!({"ids":[f.a],"opacity":0.5}))]);
    let receipt = f.write(basis, "object.rename", json!({"id":f.a,"name":"Mine"})).await.unwrap();
    let changed = receipt["others"]["changed"].as_array().unwrap();
    assert_eq!(changed.len(), 1, "{receipt}");
    assert_eq!(changed[0]["id"], f.a);
    assert_eq!(changed[0]["fields"], json!(["opacity"]));
    assert_eq!(f.disk().inspect().unwrap()["spreads"][0]["items"][0]["name"], "Mine");
    f.close().await;
}
#[tokio::test]
async fn same_field_is_changed_and_deleted_target_is_deleted() {
    for deleted in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[if deleted { ("edit.clear", json!({"ids":[f.a]})) } else { ("object.rename", json!({"id":f.a,"name":"Theirs"})) }]);
        let error = f.write(basis, "object.rename", json!({"id":f.a,"name":"Mine"})).await.unwrap_err();
        conflict(&error, if deleted { "deleted" } else { "changed" });
        f.close().await;
    }
}
#[tokio::test]
async fn insertion_before_the_index_refuses_and_after_applies() {
    for before in [true, false] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        let mut colleague = f.disk();
        let new = colleague.execute("layer.new", json!({"name":"Extra"})).unwrap()["id"].clone();
        if !before {
            colleague.execute("layer.move", json!({"id":new,"to":4})).unwrap();
        }
        std::fs::write(f.root.join(FILE), colleague.seal_and_save(None).unwrap()).unwrap();
        let answer = f.write(basis, "layer.move", json!({"id":f.layers[2],"to":1})).await;
        if before {
            conflict(&answer.unwrap_err(), "relocated");
        } else {
            answer.unwrap();
        }
        f.close().await;
    }
}
#[tokio::test]
async fn c8_fix_14_deletion_before_the_index_refuses() {
    let f = Fixture::new(Guard::Premise);
    let basis = f.basis().await;
    f.colleague(&[("layer.delete", json!({"id":f.layers[0]}))]);
    let answer = f
        .call(
            "batch",
            json!({"basis":basis,"ops":[
                {"op":"create","type":"Layer","ref":"$new","fields":{"name":"Inserted"}},
                {"op":"list.insert","target":"layout","field":"layers","index":2,"item":"$new"}
            ]}),
        )
        .await;
    // kition-sdk 2ce294a1 follows the insertion window through the earlier deletion.
    conflict(&answer.unwrap_err(), "relocated");
    f.close().await;
}
#[tokio::test]
async fn target_motion_refuses_by_id_and_by_point() {
    for point in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        f.colleague(&[("transform.move", json!({"ids":[f.a],"dx":200,"dy":0}))]);
        let answer = if point {
            f.write(basis, "gap.move", json!({"at":[80,40],"delta":2})).await
        } else {
            f.write(basis, "transform.move", json!({"ids":[f.a],"dx":2,"dy":0})).await
        };
        conflict(&answer.unwrap_err(), "relocated");
        f.close().await;
    }
}
#[tokio::test]
async fn moving_a_parent_refuses_a_child_move() {
    let f = Fixture::new(Guard::Premise);
    let mut doc = f.disk();
    let group = doc.execute("object.group", json!({"ids":[f.a,f.b]})).unwrap()["id"].clone();
    std::fs::write(f.root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
    let basis = f.basis().await;
    f.colleague(&[("transform.move", json!({"ids":[group],"dx":20,"dy":0}))]);
    conflict(&f.write(basis, "transform.move", json!({"ids":[f.a],"dx":2,"dy":0})).await.unwrap_err(), "relocated");
    f.close().await;
}
#[tokio::test]
async fn name_swaps_and_rename_away_are_refused() {
    for swap in [false, true] {
        let f = Fixture::new(Guard::Premise);
        let basis = f.basis().await;
        let mut steps = vec![("style.character.edit", json!({"name":"A","rename":"Temporary"}))];
        if swap {
            steps.extend([
                ("style.character.edit", json!({"name":"B","rename":"A"})),
                ("style.character.edit", json!({"name":"Temporary","rename":"B"})),
            ]);
        }
        f.colleague(&steps);
        conflict(&f.write(basis, "style.character.edit", json!({"name":"A","chars":{"size":18}})).await.unwrap_err(), "relocated");
        f.close().await;
    }
}
#[tokio::test]
async fn guard_modes_distinguish_unrelated_and_same_field() {
    for guard in [Guard::Premise, Guard::Epoch, Guard::Off] {
        for same in [false, true] {
            let f = Fixture::new(guard);
            let basis = f.basis().await;
            f.colleague(&[if same {
                ("object.rename", json!({"id":f.a,"name":"Theirs"}))
            } else {
                ("object.opacity", json!({"ids":[f.a],"opacity":0.5}))
            }]);
            let answer = f.write(basis, "object.rename", json!({"id":f.a,"name":"Mine"})).await;
            assert_eq!(answer.is_err(), guard == Guard::Epoch || guard == Guard::Premise && same, "{guard:?} same={same}: {answer:?}");
            f.close().await;
        }
    }
}

#[tokio::test]
async fn fifty_unrelated_rounds() {
    let mut table = Vec::new();
    for guard in [Guard::Premise, Guard::Epoch] {
        let f = Fixture::new(guard);
        let mut refused = 0;
        for round in 0..50 {
            let basis = f.basis().await;
            f.colleague(&[("object.opacity", json!({"ids":[f.c],"opacity":(round+1) as f64/51.0}))]);
            if f.write(basis, "object.rename", json!({"id":f.a,"name":format!("Round{round}")})).await.is_err() {
                refused += 1;
            }
        }
        table.push((format!("{guard:?}"), 50 - refused, refused));
        f.close().await;
    }
    println!("UNRELATED guard | applied | refused: {table:?}");
    assert_eq!(table[0].2, 0);
    assert_eq!(table[1].2, 50);
}

const KINDS: [&str; 14] = [
    "other-opacity",
    "target-name",
    "target-opacity",
    "target-move",
    "other-move",
    "new-frame",
    "other-delete",
    "style-rename",
    "layer-name",
    "target-hide",
    "page-before",
    "page-after",
    "text-before",
    "text-after",
];
const FORMS: [&str; 6] = ["by-id", "by-name", "by-point", "group", "page-index", "text-offset"];
// Necessary rule, fixed before execution:
// by-id renames A: A's name/existence only; by-name edits style A: style A's name/content.
// by-point resizes the gap at (80,40): its picked neighbours, their geometry/visibility,
// and page/layer facts; group A+B: both members' placement, order, existence and layers.
// Extra columns, fixed before execution: page-index changes page 3's width, holding its
// counted prefix and geometry. text-offset replaces bytes 4..6 (C8-layout review 2, a
// positional address): an edit before byte 4 refuses `relocated`, one inside [4,6) refuses
// `changed`, one after byte 6 applies. text-before (at 0) is necessary; text-after (at 9) is
// not. Other stories and pages strictly after page 3 are not.
fn necessary(kind: usize, form: usize) -> bool {
    match form {
        0 => kind == 1,
        1 => kind == 7,
        2 => matches!(kind, 3 | 9),
        3 => kind == 3,
        4 => kind == 10,
        5 => kind == 12,
        _ => false,
    }
}
fn change(f: &Fixture, kind: usize, round: usize) -> (&'static str, Value) {
    match kind {
        0 => ("object.opacity", json!({"ids":[f.c],"opacity":0.5})),
        1 => ("object.rename", json!({"id":f.a,"name":format!("Theirs{round}")})),
        2 => ("object.opacity", json!({"ids":[f.a],"opacity":0.5})),
        3 => ("transform.move", json!({"ids":[f.a],"dx":200,"dy":0})),
        4 => ("transform.move", json!({"ids":[f.c],"dx":10,"dy":0})),
        5 => ("frame.create", json!({"rect":[400,400,440,440],"content":"unassigned"})),
        6 => ("edit.clear", json!({"ids":[f.c]})),
        7 => ("style.character.edit", json!({"name":"A","rename":"Gone"})),
        8 => ("layer.set", json!({"id":f.layers[0],"name":"Changed"})),
        9 => ("object.hide", json!({"ids":[f.a]})),
        10 => ("layout.pages.delete", json!({"pages":[1]})),
        11 => ("layout.pageSize", json!({"pages":[6],"width":450})),
        12 => ("story.replaceRange", json!({"story":f.story,"start":0,"end":0,"text":"Early"})),
        _ => ("story.replaceRange", json!({"story":f.story,"start":9,"end":9,"text":"Late"})),
    }
}
fn write(f: &Fixture, form: usize) -> (&'static str, Value) {
    match form {
        0 => ("object.rename", json!({"id":f.a,"name":"Mine"})),
        1 => ("style.character.edit", json!({"name":"A","chars":{"size":18}})),
        2 => ("gap.move", json!({"at":[80,40],"delta":2})),
        3 => ("object.group", json!({"ids":[f.a,f.b]})),
        4 => ("layout.pageSize", json!({"pages":[3],"width":400})),
        _ => ("story.replaceRange", json!({"story":f.story,"start":4,"end":6,"text":"Mine"})),
    }
}
#[tokio::test]
async fn mixed_table_seed_9e3779b97f4a7c15() {
    let mut seed = 0x9e3779b97f4a7c15u64;
    let mut counts = vec![[0usize; 5]; KINDS.len() * FORMS.len()];
    for round in 0..50 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        // Four independent original forms per round (200), plus page/text columns (100).
        // Each form gets its own file, observation, independent rewrite and guarded write.
        let kind = if round < KINDS.len() { round } else { seed as usize % KINDS.len() };
        for (form, form_name) in FORMS.iter().enumerate() {
            let cell = kind * FORMS.len() + form;
            let needed = necessary(kind, form);
            let f = Fixture::new(Guard::Premise);
            let basis = f.basis().await;
            f.colleague(&[change(&f, kind, round)]);
            let (command, params) = write(&f, form);
            let answer = f.write(basis, command, params).await;
            let class = match &answer {
                Ok(_) => {
                    if needed {
                        3
                    } else {
                        0
                    }
                }
                Err(error) if error["error"]["code"] == "conflict" => {
                    if needed {
                        1
                    } else {
                        2
                    }
                }
                Err(_) => 4,
            };
            counts[cell][class] += 1;
            println!(
                "MIX round={round:02} kind={} form={} necessary={needed} class={}{}",
                KINDS[kind],
                form_name,
                ["applied", "refused-necessary", "refused-false", "applied-wrongly", "failed-kit"][class],
                if class >= 2 { format!(" answer={answer:?}") } else { String::new() }
            );
            f.close().await;
        }
    }
    for (kind, name) in KINDS.iter().enumerate() {
        for (form, write) in FORMS.iter().enumerate() {
            println!("MIXTABLE {name} | {write} | {:?}", counts[kind * FORMS.len() + form]);
        }
    }
    let totals = (0..5).map(|class| counts.iter().map(|row| row[class]).sum::<usize>()).collect::<Vec<_>>();
    println!("MIXTOTAL [applied,necessary,false,wrong,kit]={totals:?}");
    assert_eq!(totals[2..], [0, 0, 0], "mixed-table failures must be fixed or reported with their cause");
}

#[tokio::test]
async fn receipt_bytes_and_saved_values() {
    for (label, children, group, property) in [
        ("property", 1, false, true),
        ("move", 1, false, false),
        ("parent-5", 5, true, false),
        ("parent-200", 200, true, false),
        ("offset-200", 200, false, false),
    ] {
        let f = Fixture::new(Guard::Premise);
        let mut doc = f.disk();
        let mut ids = vec![f.a.clone()];
        for n in 1..children {
            ids.push(doc.execute("frame.create", json!({"rect":[10+n,100,20+n,110],"content":"unassigned"})).unwrap()["id"].clone());
        }
        if group {
            ids = vec![doc.execute("object.group", json!({"ids":ids})).unwrap()["id"].clone()];
        }
        std::fs::write(f.root.join(FILE), doc.seal_and_save(None).unwrap()).unwrap();
        let basis = f.basis().await;
        let receipt = if property {
            f.write(basis, "object.opacity", json!({"ids":ids,"opacity":0.123456789})).await.unwrap()
        } else {
            f.write(basis, "transform.move", json!({"ids":ids,"dx":0.123456789,"dy":0})).await.unwrap()
        };
        let changes = &receipt["changes"];
        assert!(changes.is_object(), "a write has a change set: {receipt}");
        println!(
            "RECEIPT {label}: full={} changes={} {}",
            receipt.to_string().len(),
            changes.to_string().len(),
            if children == 1 { changes.to_string() } else { String::new() }
        );
        println!(
            "RECEIPTDETAIL {label}: changed={} truncated={} carries={}",
            changes["changed"].as_array().unwrap().len(),
            changes.get("truncated").unwrap_or(&Value::Null),
            changes["changed"][0].get("carries").unwrap_or(&Value::Null)
        );
        if group {
            assert_eq!(changes["changed"][0]["carries"]["count"], children, "a parent receipt names its carried descendants");
        }
        let mut disk = f.disk();
        let mut exchange = kition_kit::session::model::Exchange::new();
        for changed in changes["changed"].as_array().unwrap() {
            let id = changed["id"].as_str().map(str::to_owned).unwrap_or_else(|| changed["id"].to_string());
            let loaded = exchange.batch(&mut disk, &json!({"ops":[],"load":[{"target":id}]}), true).unwrap();
            for edit in changed["edits"].as_array().unwrap() {
                let field = edit["field"].as_str().unwrap();
                if field.starts_with('~') {
                    continue;
                }
                if let Some(after) = edit.get("after") {
                    let pointer = format!("/{}", field.replace('.', "/"));
                    assert_eq!(
                        loaded["loaded"][0]["fields"].pointer(&pointer),
                        Some(after),
                        "{label} {id} {field}: receipt must agree with reopened file"
                    );
                }
            }
        }
        f.close().await;
    }
}
