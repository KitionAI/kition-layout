//! A document built by the same commands saves the same bytes when `KITION_REPRODUCIBLE_IDS` is set
//! (with the KDM's `reproducible-ids` feature, which this crate enables in its dev-dependencies
//! only) and `SOURCE_DATE_EPOCH` fixes the clock, as a showcase sample or a gating test builds it:
//! the replica's actor and the document id come from a counter in the process instead of
//! randomness. `SOURCE_DATE_EPOCH` alone, which hosts pass to every kit, leaves ids random. Each
//! build runs in a process of its own: this test starts itself several times.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use kition_kit::KitDocument;
use kition_layout_kit::LayoutDocument;
use serde_json::json;

const OUT: &str = "KITION_LAYOUT_DETERMINISM_OUT";

fn build() -> Vec<u8> {
    let mut doc = LayoutDocument::create("folio.klayout", 400, 600).unwrap();
    doc.execute("swatch.add", json!({ "name": "Coral", "color": { "c": 0, "m": 66, "y": 62, "k": 0 } })).unwrap();
    doc.execute("frame.add", json!({ "spread": 0, "rect": [40, 40, 360, 200], "text": "Spring issue" })).unwrap();
    doc.execute("frame.add", json!({ "spread": 0, "rect": [40, 240, 360, 400], "content": "graphic" })).unwrap();
    doc.save(Some("folio.klayout")).unwrap()
}

/// The child's half: builds the drawing into the file `OUT` names (nothing without it).
#[test]
fn child_builds_a_document() {
    if let Ok(path) = std::env::var(OUT) {
        std::fs::write(path, build()).unwrap();
    }
}

fn build_in_a_process(path: &PathBuf, counted: bool) -> Vec<u8> {
    let mut child = Command::new(std::env::current_exe().unwrap());
    child.args(["--exact", "child_builds_a_document", "--test-threads=1", "--quiet"]).env("SOURCE_DATE_EPOCH", "1794182400").env(OUT, path);
    if counted {
        child.env("KITION_REPRODUCIBLE_IDS", "1");
    } else {
        child.env_remove("KITION_REPRODUCIBLE_IDS");
    }
    let status = child.status().unwrap();
    assert!(status.success(), "the child build failed");
    std::fs::read(path).unwrap()
}

#[test]
fn the_same_commands_save_the_same_bytes() {
    let dir = std::env::temp_dir().join(format!("kition-layout-determinism-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let first = build_in_a_process(&dir.join("0.klayout"), true);
    for build in 1..4 {
        // A second apart, so a clock read would show.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let again = build_in_a_process(&dir.join(format!("{build}.klayout")), true);
        assert!(first == again, "build {build} saved other bytes than build 0");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_build_epoch_alone_leaves_ids_random() {
    let dir = std::env::temp_dir().join(format!("kition-layout-random-ids-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let first = build_in_a_process(&dir.join("0.klayout"), false);
    let again = build_in_a_process(&dir.join("1.klayout"), false);
    assert!(first != again, "SOURCE_DATE_EPOCH alone counted the ids");
    std::fs::remove_dir_all(&dir).ok();
}
