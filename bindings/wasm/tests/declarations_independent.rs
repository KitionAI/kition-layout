//! C8-layout-read: expected dependencies recorded from handlers and their operations before
//! opening the declaration implementation. These are readings, not snapshots of `under`.
//! A row's provenance must say explicitly if it was pinned after declaration review.

#[path = "independent/harness.rs"]
mod harness;
#[path = "independent/layout.rs"]
mod layout;
#[path = "independent/misc.rs"]
mod misc;
#[path = "independent/objects.rs"]
mod objects;
#[path = "independent/text_styles.rs"]
mod text_styles;

#[derive(Debug)]
pub struct Row {
    pub id: &'static str,
    pub resolves: &'static str,
    pub reads: &'static str,
    pub writes: &'static str,
    pub editor: &'static [&'static str],
    pub provenance: &'static str,
}

fn rows() -> Vec<Row> {
    misc::rows().into_iter().chain(layout::rows()).chain(objects::rows()).chain(text_styles::rows()).collect()
}

pub struct Check {
    pub id: &'static str,
    pub params: &'static str,
    pub setup: &'static [(&'static str, &'static str)],
    pub required: &'static [(&'static str, &'static [&'static str])],
    pub free: &'static [(&'static str, &'static [&'static str])],
    pub editor: &'static [&'static str],
    pub provenance: &'static str,
}

#[test]
fn independent_readings_name_real_unique_commands_and_state_their_dependencies() {
    let mut seen = std::collections::BTreeSet::new();
    let rows = rows();
    for row in &rows {
        assert!(seen.insert(row.id), "duplicate reading: {}", row.id);
        assert!(kition_layout_engine::find_command(row.id).is_some(), "unregistered reading: {}", row.id);
        assert!(!row.resolves.is_empty() && !row.reads.is_empty() && !row.writes.is_empty(), "incomplete reading: {row:?}");
        assert!(matches!(row.provenance, "blind" | "pinned from the declaration after review"), "missing provenance: {row:?}");
        assert!(row.editor.iter().all(|s| !s.is_empty()), "empty editor dependency: {row:?}");
    }
    let writes = rows.iter().filter(|r| kition_layout_engine::find_command(r.id).is_some_and(|c| c.journal)).count();
    let registered: std::collections::BTreeSet<_> = kition_layout_engine::command_specs().iter().map(|c| c.id).collect();
    assert_eq!(seen, registered, "every registry handler needs an independent reading");
    assert_eq!(rows.len(), 461, "re-read new or removed handlers");
    eprintln!("independently read {} registry handlers, {writes} journaled commands", rows.len());
}

#[test]
fn blind_selection_and_active_reads_are_declared() {
    use kition_kit::{EditorState, KitDocument};
    let doc = kition_layout_kit::LayoutDocument::create("Independent.klayout", 612, 792).unwrap();
    let commands = doc.commands();
    let mut missing = Vec::new();
    for row in rows() {
        if !kition_layout_engine::find_command(row.id).is_some_and(|command| command.journal) {
            continue;
        }
        let command = commands.iter().find(|command| command.id == row.id).unwrap();
        for (name, axis) in [("selection", EditorState::Selection), ("active", EditorState::Active)] {
            if row.editor.contains(&name) && !command.reads.contains(&axis) {
                missing.push(format!("{}: {name}", row.id));
            }
        }
    }
    assert!(missing.is_empty(), "blind editor dependencies omitted:\n{}", missing.join("\n"));
}

// Input coverage is independent of the comparison floor below.
#[test]
fn blind_projection_catalogue_has_valid_inputs_and_covers_every_journaled_command() {
    let checks: Vec<_> = layout::checks().into_iter().chain(objects::checks()).chain(text_styles::checks()).chain(misc::checks()).collect();
    let mut projected = std::collections::BTreeSet::new();
    for check in &checks {
        projected.insert(check.id);
        assert!(kition_layout_engine::find_command(check.id).is_some(), "unknown projection {}", check.id);
        let params: serde_json::Value = serde_json::from_str(check.params).unwrap_or_else(|error| panic!("{}: {error}", check.id));
        assert!(params.is_object(), "{} params must be an object", check.id);
        for (command, params) in check.setup {
            assert!(kition_layout_engine::find_command(command).is_some(), "unknown fixture command {command}");
            let _: serde_json::Value = serde_json::from_str(params).unwrap_or_else(|error| panic!("{} fixture {command}: {error}", check.id));
        }
        assert!(check.required.iter().chain(check.free).all(|(role, fields)| !role.is_empty() && fields.iter().all(|field| !field.is_empty())));
        assert!(check.editor.iter().all(|axis| matches!(*axis, "selection" | "active" | "mode" | "area" | "part" | "time")));
        assert_eq!(check.provenance, "blind", "{} was expected independently", check.id);
    }
    for row in rows() {
        if kition_layout_engine::find_command(row.id).is_some_and(|command| command.journal) {
            assert!(projected.contains(row.id), "{} has no blind projection", row.id);
        }
    }
    // C8-layout-read-3: 66 misc projections written blind from the misc rows.
    assert_eq!(checks.len(), 412);
    eprintln!("validated {} blind projection inputs", checks.len());
}

// Translation rules (the projections stay independent of the declarations):
// - Fields/positions use the model schema. Story props.* is stored as options.*.
// - An owning list/tree/map field holds its descendants, including a field reached through
//   that ownership path. Non-owning refs (spread -> item, layer -> item) do not imply ownership.
// - covers is evaluated through covered(command, params, []); its returned objects are held
//   at covers.fields. For offset text commands, text denotes the addressed prefix/range, not
//   later text. within counts identities/places; it does not hold all list contents.
// - Composition frame probes denote effective areas/options/grid, not unused raw geometry:
//   ~compositionFrames is equivalent for the story/frame actually used by the handler.
//   edit.replaceObjects' content/shape predicate is ~queryKind (cmd/find.rs:298).
// - A coarse named-setting probe denotes just the handler-read member when the handler
//   patches/deletes by name: conditions.rs:46/73/85; lists.rs:24/44; style.rs:39/354/365;
//   variables.rs:113 (existing definition only); xref.rs:85/define_format/set_format.
//   Variable insertion/deletion reads indices/counts: no named-record equivalence there.
// - New/unused-layer predicates use count/identity/usage, guide editability its effective
//   visible/unlocked predicate, and allToProcess its effective default false value. These
//   are predicate equivalences, never permission to satisfy arbitrary fields on an object.
// - style.cell.add's coarse source text/strokes probes mean first paragraph style and
//   stroke[0], the only members copied at cmd/table.rs:1106-1111. The cell's ~styleSource holds exactly
//   that cell's copied values, excluding its text characters, remaining strokes and cell span.
// The finite required/free probes do not enumerate every possible field or conditional path.
// C8-layout-read-3 covers all 395 journaled commands (misc added, 66). Measured over all of them on
// the declarations before this card (8efd8626): 231 differ (108 held less, 180 more). The
// remaining branch/translation review is incomplete; the free probes do not enumerate every
// excessive hold. Blocked handlers compare under but have
// no claimed successful agent path; the report labels them explicitly.
// Initial C8-layout-read-2 measurement before premise fixes: 166/395 commands differ in
// these probes (76 less, 115 more, 25 both); 66 commands have no projection yet. Floors only
// decrease as premise fixes land. Improving translations is not a declaration fix.
// This baseline is scoped to the three existing families. Adding misc must establish its own
// initial baseline, not raise this one or count its unexamined commands as agreements.
// C8-layout-read-3, read class held less: 165/75/115 -> 127/21/110; defaults: -> 116/8/110;
// two paths: -> 111/1/110; value naming another object: -> 110/0/110.
// held more narrowed (one effect/field per target; editor axes): -> 100/0/100.
const LEFT: (usize, usize, usize) = (100, 0, 100);
// C8-layout-read-3: the misc family's own initial baseline (66 commands), measured on the
// declarations before this card's fixes (8efd8626): 66/33/65; misc reads held: -> 65/1/65.
const MISC_LEFT: (usize, usize, usize) = (65, 1, 65);
const MISC_CLASSES_LEFT: [(&str, usize, usize); 6] = [
    ("parameter resolution", 0, 0),
    ("value naming another object", 0, 0),
    ("default", 0, 0),
    ("two paths", 0, 0),
    ("read", 1, 24),
    ("editor state", 0, 65),
];
const CLASSES_LEFT: [(&str, usize, usize); 6] = [
    ("parameter resolution", 0, 0),
    ("value naming another object", 0, 0),
    ("default", 0, 0),
    ("two paths", 0, 0),
    ("read", 0, 55),
    ("editor state", 0, 55),
];

#[test]
fn projection_fixtures_execute_and_report_effective_holds() {
    use std::collections::{BTreeMap, BTreeSet};
    let mut errors = Vec::new();
    let (mut projected, mut less, mut more) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut classes: BTreeMap<&str, (BTreeSet<&str>, BTreeSet<&str>)> = BTreeMap::new();
    let misc_ids: BTreeSet<&str> = misc::checks().iter().map(|check| check.id).collect();
    for check in layout::checks().into_iter().chain(objects::checks()).chain(text_styles::checks()).chain(misc::checks()) {
        let report = harness::project(&check);
        eprintln!("INDEPENDENT {}", serde_json::to_string(&report).unwrap());
        if let Some(error) = &report.error {
            errors.push(format!("{}: {error}", check.id));
        }
        if !kition_layout_engine::find_command(check.id).unwrap().journal {
            continue;
        }
        projected.insert(check.id);
        for field in &report.missing {
            less.insert(check.id);
            classes.entry(harness::class(check.id, field)).or_default().0.insert(check.id);
        }
        if !report.missing_editor.is_empty() {
            less.insert(check.id);
            classes.entry("editor state").or_default().0.insert(check.id);
        }
        if !report.held_free.is_empty() {
            more.insert(check.id);
            classes.entry("read").or_default().1.insert(check.id);
        }
        if !report.extra_editor.is_empty() {
            more.insert(check.id);
            classes.entry("editor state").or_default().1.insert(check.id);
        }
    }
    assert!(errors.is_empty(), "projection fixtures must reach their intended paths:\n{}", errors.join("\n"));
    // The misc family has its own floor (below); the three-family floors stay as they were.
    let misc = |set: &BTreeSet<&'static str>| set.iter().filter(|id| misc_ids.contains(*id)).copied().collect::<BTreeSet<_>>();
    let (misc_less, misc_more) = (misc(&less), misc(&more));
    let mut misc_classes: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (name, (class_less, class_more)) in &mut classes {
        let entry = misc_classes.entry(name).or_default();
        entry.0 = class_less.iter().filter(|id| misc_ids.contains(*id)).count();
        entry.1 = class_more.iter().filter(|id| misc_ids.contains(*id)).count();
        class_less.retain(|id| !misc_ids.contains(id));
        class_more.retain(|id| !misc_ids.contains(id));
    }
    let misc_differing = misc_less.union(&misc_more).count();
    for (name, less_left, more_left) in MISC_CLASSES_LEFT {
        let (class_less, class_more) = misc_classes.get(name).copied().unwrap_or_default();
        eprintln!("MISC CLASS {name}: less={class_less} more={class_more}");
        assert!(class_less <= less_left && class_more <= more_left, "misc {name} declarations moved away from the independent probes");
    }
    eprintln!(
        "MISC MEASURED {} commands; differing={misc_differing}, less={}, more={}, both={}",
        misc_ids.len(),
        misc_less.len(),
        misc_more.len(),
        misc_less.intersection(&misc_more).count()
    );
    assert!(
        misc_differing <= MISC_LEFT.0 && misc_less.len() <= MISC_LEFT.1 && misc_more.len() <= MISC_LEFT.2,
        "misc declarations moved away from the independent probes"
    );
    eprintln!("OVERALL {} / 395 commands; differing={}, less={}, more={}", projected.len(), less.union(&more).count(), less.len(), more.len());
    less.retain(|id| !misc_ids.contains(id));
    more.retain(|id| !misc_ids.contains(id));
    projected.retain(|id| !misc_ids.contains(id));
    for (name, less_left, more_left) in CLASSES_LEFT {
        let entry = classes.entry(name).or_default();
        eprintln!("CLASS {name}: less={} more={}", entry.0.len(), entry.1.len());
        assert!(entry.0.len() <= less_left && entry.1.len() <= more_left, "{name} declarations moved away from the independent probes");
    }
    let differing = less.union(&more).count();
    let unprojected: Vec<_> = rows()
        .into_iter()
        .filter(|row| kition_layout_engine::find_command(row.id).unwrap().journal && !projected.contains(row.id) && !misc_ids.contains(row.id))
        .map(|row| row.id)
        .collect();
    eprintln!("UNPROJECTED {}: {}", unprojected.len(), unprojected.join(", "));
    eprintln!(
        "MEASURED {} journaled commands projected / 395; differing={differing}, less={}, more={}, both={}",
        projected.len(),
        less.len(),
        more.len(),
        less.intersection(&more).count()
    );
    assert_eq!(projected.len(), 330, "the measured command coverage must not shrink");
    assert!(unprojected.is_empty(), "every journaled command has a blind projection: {unprojected:?}");
    assert_eq!(misc_ids.len(), 66);
    assert!(differing <= LEFT.0 && less.len() <= LEFT.1 && more.len() <= LEFT.2, "declarations moved away from the independent probes");
}
