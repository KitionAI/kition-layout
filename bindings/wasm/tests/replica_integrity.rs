//! C8-land-13-layout: no successful engine command may silently abandon its replica.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod common;
use kition_convergence::{KitAdapter, Rng};

struct Warnings;
impl log::Log for Warnings {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{}", record.args());
        }
    }
    fn flush(&self) {}
}

/// Random edits through the engine's commands, on one line of changes: the replica the layout
/// started with is the one it ends with.
#[test]
fn delivery_edits_preserve_replica_identity() {
    log::set_logger(&Warnings).unwrap();
    log::set_max_level(log::LevelFilter::Warn);
    let kit = common::LayoutKit;
    let mut doc = kit.create().unwrap();
    let identity = doc.doc_id();
    let mut rng = Rng::new(24);
    for round in 0..4 {
        for step in 0..3 {
            let mut label = None;
            for _ in 0..8 {
                label = kit.random_edit(&mut doc, &mut rng).unwrap();
                if label.is_some() {
                    break;
                }
            }
            eprintln!("round={round} step={step} edit={label:?}");
            assert_eq!(doc.kdm_resets(), 0, "round={round} step={step} edit={label:?}");
            assert_eq!(doc.doc_id(), identity, "round={round} step={step} edit={label:?}");
        }
    }
}
