//! `kition-layout`: the Layout kit for Kition's agent. The same document the in-page editor runs
//! (`kition-layout-kit`), served headless over the Kit Protocol by `kition-headless`, as the kit's
//! `kit.json` declares it, with the object model (`model.describe`, `model.batch`) and the replica
//! methods (`model.open`, `model.import`, `model.export`, `model.save`) over the document's change
//! exchange (engine contract 2).
//!
//! The executable's `main` serves [`backend`]; tests drive the same backend in-process.
//!
//! Its documents reach files only through the host (KHP A29, A45, section 10.6): every document
//! it makes is hosted (`kition_layout_kit::files::serve_hosted`), so its engine reads a file a
//! command or a document names only through the host, by its path in the workspace, reaches
//! nothing else by path, starts no program, and the process reads no font folder of the machine.

use kition_headless::{Headless, KitInfo};
use kition_layout_kit::LayoutDocument;

/// The kit's manifest.
pub const MANIFEST: &str = include_str!("../../../kit.json");
/// `inspect()` keys about the editing session rather than the file.
const VOLATILE: &[&str] = kition_layout_kit::EDITOR_STATE;

/// The kit as `kit.json` declares it, with the commands that name files an agent may run: they
/// read only what the host lends from the workspace.
pub fn kit() -> Result<KitInfo, String> {
    KitInfo::from_manifest(MANIFEST, kition_layout_kit::files::AGENT_FILE_COMMANDS, VOLATILE)
}

/// The backend the executable serves: from now on every document this process makes is hosted.
pub fn backend(kit: &KitInfo) -> Headless<LayoutDocument> {
    kition_layout_kit::files::serve_hosted();
    Headless::<LayoutDocument>::with_replica_port(kit.clone())
}
