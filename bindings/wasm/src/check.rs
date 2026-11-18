//! What [`KitDocument::check`](kition_kit::KitDocument::check) reports on a document (KHP A21):
//! the engine's live preflight (the Preflight panel's findings, `preflight.run`), as KHP
//! diagnostics. Items are named as `item:<id>` (the ids the engine's commands take in `ids`) with
//! the page they are on, counted from 1.
//!
//! Codes: `layout.overset`, `layout.missing-font`, `layout.missing-glyphs`, `layout.missing-link`,
//! `layout.unsupported-typography`, `layout.low-resolution`, `layout.empty-frame`, and
//! `layout.rgb-in-print`.

use kition_kit::diagnostics::{Diagnostic, Severity, Target};
use kition_layout_engine::Session;
use kition_layout_engine::cmd::preflight::{self, Issue};
use serde_json::json;

/// The effective resolution below which a placed image in a print document is reported.
pub const MIN_PPI: f64 = 150.0;

/// One code the check reports: its severity, a one-sentence summary for the catalogue, and the
/// kind of overflow it reports, if it reports one (KHP A38, A40: `text`, `off-page` or
/// `page-break`; tools tell overflow codes by this tag, never by their name).
pub struct Entry {
    pub code: &'static str,
    pub severity: Severity,
    pub summary: &'static str,
    pub overflow: Option<&'static str>,
}

const fn entry(code: &'static str, severity: Severity, summary: &'static str) -> Entry {
    Entry { code, severity, summary, overflow: None }
}

/// An entry for a code that reports an overflow of `kind`.
const fn overflow(code: &'static str, severity: Severity, summary: &'static str, kind: &'static str) -> Entry {
    Entry { code, severity, summary, overflow: Some(kind) }
}

/// Every code [`check`] reports, errors first: the catalogue shipped as `agent/checks.json` (KHP
/// A38), with one conformance fixture per code (`conformance/check-<name>.klayout`).
pub const CATALOGUE: &[Entry] = &[
    overflow("layout.overset", Severity::Error, "A story has more text than its frames hold, so the rest does not print.", "text"),
    entry("layout.missing-font", Severity::Error, "Text uses a font that is not available here, so it is drawn with a substitute."),
    entry("layout.missing-glyphs", Severity::Error, "Characters are drawn as their font's missing glyph because the font lacks them."),
    entry("layout.missing-link", Severity::Error, "A placed graphic's file is missing, so it prints as an empty frame."),
    entry(
        "layout.unsupported-typography",
        Severity::Warning,
        "Text uses a typography feature this build does not compose yet, so it may differ from the original.",
    ),
    entry(
        "layout.low-resolution",
        Severity::Warning,
        "A placed image in a print document has fewer pixels per inch than print needs at its placed size.",
    ),
    entry("layout.empty-frame", Severity::Warning, "A text frame holds no text; removing it changes nothing on the page."),
    entry("layout.rgb-in-print", Severity::Warning, "A print document uses an RGB swatch, which a printer converts unpredictably."),
];

/// The catalogue as `agent/checks.json` holds it: code, severity, summary, its fixture, and the
/// overflow it reports, if any.
pub fn catalogue_json() -> serde_json::Value {
    serde_json::Value::Array(
        CATALOGUE
            .iter()
            .map(|entry| {
                let mut out = json!({
                    "code": entry.code,
                    "severity": entry.severity,
                    "summary": entry.summary,
                    "fixture": format!("check-{}.klayout", entry.code.trim_start_matches("layout.")),
                });
                if let (Some(kind), Some(map)) = (entry.overflow, out.as_object_mut()) {
                    map.insert("overflow".into(), json!(kind));
                }
                out
            })
            .collect(),
    )
}

fn code_of(kind: &str) -> String {
    match kind {
        "overset" => "layout.overset".into(),
        "missingFont" => "layout.missing-font".into(),
        "missingGlyphs" => "layout.missing-glyphs".into(),
        "missingLink" => "layout.missing-link".into(),
        "unsupportedTypography" => "layout.unsupported-typography".into(),
        "lowResolution" => "layout.low-resolution".into(),
        "emptyText" => "layout.empty-frame".into(),
        "rgbColor" => "layout.rgb-in-print".into(),
        // A kind preflight learns later than this list.
        other => {
            let mut code = String::from("layout.");
            for c in other.chars() {
                if c.is_ascii_uppercase() {
                    code.push('-');
                    code.push(c.to_ascii_lowercase());
                } else {
                    code.push(c);
                }
            }
            code
        }
    }
}

fn diagnostic(issue: Issue) -> Diagnostic {
    let severity = if issue.severity == "error" { Severity::Error } else { Severity::Warning };
    let mut message = issue.message.trim_end_matches('.').to_owned();
    message.push('.');
    let mut diagnostic = Diagnostic::new(code_of(issue.kind), severity, message);
    if issue.item.is_some() || issue.page.is_some() {
        diagnostic = diagnostic.on(Target {
            path: issue.item.map(|id| format!("item:{id}")),
            page: issue.page.map(|page| (page + 1) as u32),
            ..Target::default()
        });
    }
    let missing_family = issue.message.strip_prefix("Missing font: ").map(str::to_owned);
    match (issue.kind, issue.item) {
        ("overset", Some(id)) => diagnostic.fix("frame.fitText", Some(json!({ "ids": [id] }))),
        // The family this build always has; the person may want another available one.
        ("missingFont", _) if missing_family.is_some() => {
            diagnostic.fix("font.replace", Some(json!({ "family": missing_family, "toFamily": kition_layout_fonts::DEFAULT_FAMILY })))
        }
        ("emptyText", Some(id)) => diagnostic.fix("edit.clear", Some(json!({ "ids": [id] }))),
        _ => diagnostic,
    }
}

/// Everything worth fixing in the active document, errors first.
pub fn check(session: &Session) -> Vec<Diagnostic> {
    let mut found: Vec<Diagnostic> = preflight::check(session, MIN_PPI).into_iter().map(diagnostic).collect();
    found.sort_by_key(|diagnostic| diagnostic.severity);
    found
}
