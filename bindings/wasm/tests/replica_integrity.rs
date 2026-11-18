//! C8-land-13-layout: no successful engine command may silently abandon its replica.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod common;
use kition_convergence::{KitAdapter, Net, Rng};

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
#[test]
fn delivery_edits_preserve_replica_identity() {
    log::set_logger(&Warnings).unwrap();
    log::set_max_level(log::LevelFilter::Warn);
    let kit = common::LayoutKit;
    let mut net = Net::new(&kit, 3, 24).unwrap();
    let identity = net.docs[0].doc_id();
    for round in 0..4 {
        for index in 0..3 {
            let mut label = None;
            for _ in 0..8 {
                let mut rng = std::mem::replace(&mut net.rng, Rng::new(0));
                label = kit.random_edit(net.doc(index).unwrap(), &mut rng).unwrap();
                net.rng = rng;
                if label.is_some() {
                    break;
                }
            }
            eprintln!("round={round} replica={index} edit={label:?}");
            assert_eq!(net.docs[index].kdm_resets(), 0, "round={round} replica={index} edit={label:?}");
            assert_eq!(net.docs[index].doc_id(), identity, "round={round} replica={index} edit={label:?}");
        }
        if round == 1 {
            net.sync().unwrap();
        }
    }
}
