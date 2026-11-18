//! `layout_check` against its catalogue (`agent/checks.json`, KHP A38): the catalogue file is the
//! check's own (`check::CATALOGUE`), errors first, one sentence each; every fixture in
//! `conformance/` reports its code and only catalogued codes; `basic.klayout` is clean; and
//! checking never changes the document. The fixtures are written by the crate's `conformance` test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use kition_kit::KitDocument;
use kition_kit::diagnostics::{Severity, check_diagnostics};
use kition_layout_kit::LayoutDocument;
use kition_layout_kit::check::{CATALOGUE, catalogue_json};
use serde_json::Value;

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn open(name: &str) -> LayoutDocument {
    let bytes = std::fs::read(repo().join("conformance").join(name)).unwrap_or_else(|error| panic!("{name}: {error}"));
    LayoutDocument::open(name, &bytes).unwrap()
}

#[test]
fn the_catalogue_file_is_the_checks_own() {
    let file: Value = serde_json::from_str(&std::fs::read_to_string(repo().join("agent/checks.json")).unwrap()).unwrap();
    assert_eq!(file, catalogue_json(), "regenerate with KITION_WRITE_CONFORMANCE=1 cargo test -p kition-layout-kit conformance");
    let mut previous = Severity::Error;
    for entry in CATALOGUE {
        assert!(kition_kit::diagnostics::valid_code(entry.code) && entry.code.starts_with("layout."), "{}", entry.code);
        assert!(entry.severity >= previous, "errors first: {}", entry.code);
        previous = entry.severity;
        assert!(entry.summary.ends_with('.') && !entry.summary.contains('\n'), "{}: one sentence", entry.code);
    }
}

/// Regression (cases/regressions/issue-overflow-tag): tools and agents tell an overflow code by its
/// catalogue entry's `overflow`, never by its name (KHP A38, A40), so the kit's one overflow code,
/// `layout.overset`, carries `"overflow": "text"`, and only a known kind is ever given.
#[test]
fn the_catalogue_tags_its_overflow_codes() {
    let file: Value = serde_json::from_str(&std::fs::read_to_string(repo().join("agent/checks.json")).unwrap()).unwrap();
    let mut tagged = Vec::new();
    for entry in file.as_array().unwrap() {
        if let Some(kind) = entry.get("overflow") {
            assert!(["text", "off-page", "page-break"].contains(&kind.as_str().unwrap_or("")), "{entry}");
            tagged.push((entry["code"].as_str().unwrap(), kind.as_str().unwrap()));
        }
    }
    assert_eq!(tagged, [("layout.overset", "text")]);
}

#[test]
fn every_fixture_reports_its_code_and_only_catalogued_codes() {
    let known: Vec<&str> = CATALOGUE.iter().map(|entry| entry.code).collect();
    for entry in CATALOGUE {
        let name = format!("check-{}.klayout", entry.code.trim_start_matches("layout."));
        let document = open(&name);
        let found = document.check();
        check_diagnostics(&found).unwrap();
        assert!(found.iter().any(|diagnostic| diagnostic.code == entry.code), "{name}: {found:?}");
        for diagnostic in &found {
            assert!(known.contains(&diagnostic.code.as_str()), "{name}: {} is not catalogued", diagnostic.code);
        }
    }
}

#[test]
fn basic_is_clean_and_checking_changes_nothing() {
    let document = open("basic.klayout");
    let before = document.inspect().unwrap();
    assert_eq!(document.check(), Vec::new());
    assert_eq!(document.inspect().unwrap(), before);
    assert!(!document.dirty());
}

#[test]
fn each_fix_is_a_command_that_clears_its_code() {
    for entry in CATALOGUE {
        let name = format!("check-{}.klayout", entry.code.trim_start_matches("layout."));
        let mut document = open(&name);
        let fixes: Vec<_> = document.check().into_iter().filter(|d| d.code == entry.code).filter_map(|d| d.fix).collect();
        if fixes.is_empty() {
            continue;
        }
        for fix in &fixes {
            document.execute(&fix.command, fix.params.clone().unwrap_or_default()).unwrap_or_else(|error| panic!("{name}: {}: {error}", fix.command));
        }
        assert!(!document.check().iter().any(|d| d.code == entry.code), "{name}: the fix leaves {}", entry.code);
    }
}
