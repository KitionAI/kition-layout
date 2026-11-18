//! SDK behaviours at the pinned revision: what a repin fixed, and current wrong outcomes
//! (`waiting_for_sdk_*`) to replace after a joint repin.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_kit::kdm::{
    ActorId, DocId, Replica, Schema,
    batch::{self, ReplicaTarget},
};
use serde_json::json;
use std::sync::Arc;

#[test]
fn nested_json_member_limits_are_refused() {
    let schema = Schema::parse(
        &json!({"kdm":1,"kit":"kit.layout-probe","schemaVersion":1,"root":"Probe","types":{
            "Probe":{"fields":{"props":{"kind":"json","members":{"nested.opacity":{"minimum":0,"maximum":1}}}}}
        }})
        .to_string(),
    )
    .unwrap();
    let mut replica = Replica::create(Arc::new(schema), DocId::new(), ActorId::random()).unwrap();
    let result = batch::run(
        &mut ReplicaTarget(&mut replica),
        &json!({"ops":[
            {"op":"create","type":"Probe","id":"probe","fields":{"props":{"nested":{"opacity":2}}}}
        ]}),
    );
    // kition-sdk 2ce294a1 checks member limits inside nested JSON values.
    assert!(
        result.as_ref().err().is_some_and(|e| e.code == batch::BatchErrorCode::InvalidValue),
        "a nested value outside its limits is refused: {result:?}"
    );
}

#[test]
fn waiting_for_sdk_batch_tree_deletion_is_child_first() {
    let schema = Schema::parse(
        &json!({"kdm":1,"kit":"kit.layout-probe","schemaVersion":1,"root":"Probe","types":{
            "Probe":{"fields":{"items":{"kind":"tree","of":"Node"}}},"Node":{"fields":{}}
        }})
        .to_string(),
    )
    .unwrap();
    let mut replica = Replica::create(Arc::new(schema), DocId::new(), ActorId::random()).unwrap();
    batch::run(
        &mut ReplicaTarget(&mut replica),
        &json!({"ops":[
            {"op":"create","type":"Probe","id":"probe"},
            {"op":"create","type":"Node","id":"group","in":"probe.items"},
            {"op":"create","type":"Node","id":"child","in":"probe.items","parent":"group"}
        ]}),
    )
    .unwrap();
    let result = batch::run(&mut ReplicaTarget(&mut replica), &json!({"ops":[{"op":"delete","target":"group"}]}));
    assert!(
        result.as_ref().err().is_some_and(|e| e.message.contains("has children")),
        "waiting for SDK: delete descendants first; pin changed: {result:?}"
    );
}
