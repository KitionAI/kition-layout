//! Kition Layout against the Kition platform convergence suite (L6): two to five replicas of a
//! layout, edited through the engine's own commands, exchanging changes in any order, with
//! duplicates, crashes and late blobs.

mod common;

use std::path::{Path, PathBuf};

use common::{LayoutKit, NAME};
use kition_convergence::wasm::{self, WasmEngine};
use kition_convergence::{Options, Outcome};

#[test]
fn converges() {
    let report = kition_convergence::run(&LayoutKit, &Options::from_env());
    println!("{}", report.render());
    report.save_if_requested().unwrap();
    assert!(report.passed(), "{}", report.render());
    // Every scenario runs but one: no command moves an existing object into another group
    // (grouping makes a new group), so there is no reparent to make concurrently. With such a
    // command the adapter's `Reparent` makes it, and this becomes `report.level6()`.
    let skipped: Vec<&str> = report.checks.iter().filter(|check| matches!(check.outcome, Outcome::Skipped(_))).map(|check| check.id).collect();
    assert_eq!(skipped, ["reparent"], "{}", report.render());
}

/// Native and WebAssembly engines reach one state from one change log. The WebAssembly engine is
/// the one in `KITION_WASM_ENGINE`, else the editor's `web/public/engine/` (`pnpm --dir web engine`).
#[test]
fn wasm_parity() {
    let engine = wasm_engine();
    wasm::check_parity(&LayoutKit, &engine, Path::new(env!("CARGO_TARGET_TMPDIR")), 1).unwrap_or_else(|error| {
        panic!("{error}\n(the engine in {}: rebuild it with `pnpm --dir web engine` if it predates your changes)", engine.dir.display())
    });
}

/// The Layout engine built for WebAssembly: a folder with `kition_layout_kit.js` and
/// `kition_layout_kit_bg.wasm` in `KITION_WASM_ENGINE`, else the editor's `web/public/engine/`.
/// It is built from this repository, not a private tool, so a missing engine fails the test
/// (docs/quality.md, Private tools) and says how to build it.
fn wasm_engine() -> WasmEngine {
    const MODULE: &str = "kition_layout_kit";
    let engine = WasmEngine::from_env(MODULE, "LayoutDocument", NAME).unwrap_or_else(|| WasmEngine {
        dir: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/public/engine"),
        module: MODULE.to_owned(),
        class: "LayoutDocument".to_owned(),
        file_name: NAME.to_owned(),
        node: std::env::var_os("NODE").map_or_else(|| PathBuf::from("node"), PathBuf::from),
    });
    for file in [format!("{MODULE}.js"), format!("{MODULE}_bg.wasm")] {
        assert!(
            engine.dir.join(&file).is_file(),
            "no WebAssembly engine: {file} is not in {}. Build it with `pnpm --dir web engine` (wasm-pack, into web/public/engine/), or set KITION_WASM_ENGINE to a folder with {MODULE}.js and {MODULE}_bg.wasm",
            engine.dir.display()
        );
    }
    engine
}
