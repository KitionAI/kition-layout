//! The parameters of this kit's commands that name files or folders on disk (KHP A28), declared
//! as typed files in the commands' schemas (`kition_kit::files`): a host lends them, and a
//! headless kit keeps those commands from agents (the host opens, saves and exports documents).
//!
//! A document the kit serves is hosted ([`hosted`]): its engine session reaches files only
//! through the host (`kition_layout_engine::Session::for_host`), and starts no program. The
//! kit's own check ([`refuse_for_agent`]) keeps the commands that name files or start programs
//! from anyone but a person however the call arrives: directly, as a step of the object model's
//! `model.batch`, or as a step of `script.run` (the engine asks it before every command it runs).

use std::sync::atomic::{AtomicBool, Ordering};

use kition_kit::files::FileParam;
use kition_layout_engine::Session;
use kition_layout_engine::disk::HostFiles;

/// Every file or folder parameter of the kit's commands.
pub const FILES: &[FileParam] = &[
    FileParam::read("file.open", "path", &[]),
    FileParam::read("place.styles", "path", &[]),
    FileParam::read("snippet.place", "path", &[]),
    FileParam::read("place.load", "path", &[]),
    FileParam::read("file.place", "path", &[]),
    FileParam::read("table.placeGraphic", "path", &[]),
    FileParam::read("file.openIdml", "path", &[]),
    FileParam::read("library.open", "path", &[]),
    FileParam::read("xml.loadDtd", "path", &[]),
    FileParam::read("file.importXml", "path", &[]),
    FileParam::read("swatch.load", "path", &[]),
    FileParam::read("data.source.select", "path", &[]),
    FileParam::read("data.fields", "path", &[]),
    FileParam::read("data.merge", "path", &[]),
    FileParam::folder("file.exportIdml", "dir"),
    FileParam::folder("file.openIdml", "dir"),
    FileParam::folder("linkedFile.copyTo", "path"),
    FileParam::write("file.save", "path", &[]),
    FileParam::create("file.saveAs", "path", &[]),
    FileParam::create("file.saveACopy", "path", &[]),
    FileParam::folder("file.recovery.list", "dir"),
    FileParam::folder("file.recovery.save", "dir"),
    FileParam::folder("file.recovery.open", "dir"),
    FileParam::create("snippet.export", "path", &[]),
    FileParam::create("file.exportIdml", "path", &[]),
    FileParam::create("file.exportPdf", "path", &[]),
    FileParam::create("file.exportEpub", "path", &[]),
    FileParam::create("file.exportFixedEpub", "path", &[]),
    FileParam::create("file.printBooklet", "path", &[]),
    FileParam::create("file.exportHtml", "path", &[]),
    FileParam::create("file.exportText", "path", &[]),
    FileParam::create("book.new", "path", &[]),
    FileParam::read("book.open", "path", &[]),
    FileParam::read("book.add", "path", &[]),
    FileParam::create("book.exportPdf", "path", &[]),
    FileParam::read("color.loadProfile", "path", &[]),
    FileParam::create("library.new", "path", &[]),
    FileParam::create("file.exportXml", "path", &[]),
    FileParam::folder("file.package", "dir"),
    FileParam::folder("linkedFile.copyTo", "dir"),
    FileParam::create("swatch.save", "path", &[]),
    FileParam::read("linkedFile.relink", "path", &[]),
    FileParam::folder("linkedFile.relinkFolder", "dir"),
];

/// The commands with file parameters an agent may run: they read only what the host lends from
/// the workspace (`kition_kit::host`, by the path in the workspace), never the file system.
/// `kition-layout` names them to `kition-headless` (its sandboxed commands).
pub const AGENT_FILE_COMMANDS: &[&str] = &["file.place"];

/// Commands that name no file but can only start a program (the print spooler): a hosted kit
/// refuses them to everyone, a served one does not list them, and the manifest's `agent.exclude`
/// lists them too, for the hosts that read it (KHP A45).
pub const AGENT_EXCLUDED: &[&str] = &["file.print", "file.printers"];

/// Refuses `command` for anyone but a person (KHP A29, A45, A48): it names files or folders by
/// path or starts a program, and this kit works only on its workspace's files, through its host.
pub fn refuse_for_agent(command: &str) -> Result<(), String> {
    let names_files = FILES.iter().any(|file| file.command == command) && !AGENT_FILE_COMMANDS.contains(&command);
    if names_files || AGENT_EXCLUDED.contains(&command) {
        return Err(format!(
            "`{command}` reaches files or folders by path or starts a program, and this kit works only on files of its workspace, \
             through its host: it is not available to the agent. Open, save and export the document through the host, and place \
             pictures and text from the workspace with file.place {{\"path\": \"<path in the workspace>\"}}"
        ));
    }
    Ok(())
}

/// Whether this process serves its documents for a host ([`serve_hosted`]).
static SERVED: AtomicBool = AtomicBool::new(false);

/// From now on, every document this process makes is hosted: its host owns every file (KHP A29,
/// A45, section 10.6), and the process reads no font folder of the machine. `kition-layout` calls
/// it before it serves anything. (In the page, documents are always hosted.)
pub fn serve_hosted() {
    SERVED.store(true, Ordering::Relaxed);
    kition_layout_fonts::no_system_fonts();
}

/// Whether a host owns every file of the documents this process makes: in the page, and once
/// [`serve_hosted`] ran.
pub fn hosted() -> bool {
    cfg!(target_arch = "wasm32") || served()
}

/// Whether this native process serves its documents for a host ([`serve_hosted`]).
pub(crate) fn served() -> bool {
    SERVED.load(Ordering::Relaxed)
}

/// The files a hosted session reads: what the host lends (`kition_kit::host`).
const HOST_FILES: HostFiles = HostFiles { size: kition_kit::host::size, read: kition_kit::host::read };

/// The engine session of a document the kit makes: for the host when [`hosted`], and keeping the
/// commands [`refuse_for_agent`] names from anyone but a person.
pub(crate) fn session() -> Session {
    let mut session = if hosted() { Session::for_host(HOST_FILES) } else { Session::new() };
    session.agent_gate = Some(refuse_for_agent);
    session
}

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;

    use super::*;

    /// The manifest keeps from agents (`agent.exclude`) what the kit refuses them although it
    /// names no file.
    #[test]
    fn the_manifest_excludes_what_the_kit_refuses_agents() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../kit.json")).unwrap_or_default();
        let excluded: Vec<&str> = manifest["agent"]["exclude"].as_array().into_iter().flatten().filter_map(serde_json::Value::as_str).collect();
        assert_eq!(excluded, AGENT_EXCLUDED);
        for command in AGENT_EXCLUDED.iter().chain(&["file.saveACopy", "file.recovery.save", "book.open", "linkedFile.relink"]) {
            assert!(refuse_for_agent(command).is_err(), "{command}");
        }
        for command in ["file.place", "frame.add", "script.run", "file.exportBytes"] {
            assert!(refuse_for_agent(command).is_ok(), "{command}");
        }
        let refused = refuse_for_agent("file.saveACopy").unwrap_err();
        assert!(refused.contains("workspace") && refused.contains("file.place"), "{refused}");
    }

    /// Parameters whose names read like files but are not: ids, property paths, directions,
    /// folders of the kit's own panels, in-memory data.
    const NOT_FILES: &[(&str, &str)] = &[("text.move", "dir")];

    #[test]
    fn every_file_parameter_is_declared() {
        let doc = crate::LayoutDocument::create("Untitled.klayout", 64, 64).unwrap_or_else(|error| panic!("{error}"));
        let problems = kition_kit::files::check(&doc.commands(), FILES, NOT_FILES);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
