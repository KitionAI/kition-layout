//! A session made for a host ([`Session::for_host`], KHP A29, A42, A45, section 10.6) reaches no
//! file by path and starts no program, whatever command or document names one: it reads only what
//! its host lends, by the path in the workspace. And anyone but a person runs only what the
//! session's agent gate allows, at every step a command dispatches (`script.run`), not only the
//! first. An audit wrote and read files outside the workspace through the headless kit and
//! started `lpstat` and `lpr`.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kition_layout_kdm::kition_kit::kdm::Origin;
use serde_json::{Value, json};

use crate::Session;
use crate::disk::HostFiles;

/// The one file the test host lends: `inside.png`.
fn inside_png() -> Vec<u8> {
    kition_layout_render::Rendered { width: 4, height: 4, pixels: [201, 40, 160, 255].repeat(16) }.to_png()
}

fn lent_size(path: &str) -> std::result::Result<u64, String> {
    lent_read(path).map(|bytes| bytes.len() as u64)
}

fn lent_read(path: &str) -> std::result::Result<Vec<u8>, String> {
    match path {
        "inside.png" | "Links/inside.png" => Ok(inside_png()),
        _ => Err(format!("{path}: the host lends no such file")),
    }
}

const HOST: HostFiles = HostFiles { size: lent_size, read: lent_read };

/// A fresh folder for one test, with a file only a by-path read finds.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kd-hosted-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("secret.txt"), "SENTINEL-hosted").unwrap();
    std::fs::write(dir.join("secret.png"), inside_png()).unwrap();
    dir
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

fn hosted() -> Session {
    let mut s = Session::for_host(HOST);
    s.execute("file.new", &json!({})).unwrap();
    s
}

#[test]
fn a_hosted_session_writes_reads_and_lists_nothing_by_path() {
    let dir = folder("by-path");
    let at = |name: &str| dir.join(name).to_string_lossy().into_owned();
    let mut s = hosted();
    assert!(s.host_files_only());
    let id = s.execute("frame.create", &json!({"rect": [72, 72, 200, 200], "content": "text", "text": "Hello"})).unwrap()["id"].clone();
    s.execute("selection.set", &json!({"ids": [id]})).unwrap();
    let refused: Vec<(&str, Value)> = vec![
        ("file.saveACopy", json!({"path": at("a.klayout")})),
        ("file.save", json!({"path": at("b.klayout")})),
        ("file.saveAs", json!({"path": at("c.klayout")})),
        ("snippet.export", json!({"path": at("d.klayout")})),
        ("file.exportPdf", json!({"path": at("e.pdf")})),
        ("file.exportIdml", json!({"path": at("f.idml")})),
        ("file.exportEpub", json!({"path": at("g.epub")})),
        ("file.exportHtml", json!({"path": at("h.html")})),
        ("file.exportXml", json!({"path": at("i.xml")})),
        ("file.exportText", json!({"path": at("j.txt")})),
        ("file.printBooklet", json!({"path": at("k.pdf")})),
        ("swatch.save", json!({"path": at("l.ase")})),
        ("book.new", json!({"path": at("m.dcbook")})),
        ("library.new", json!({"path": at("n.dclib")})),
        ("file.recovery.save", json!({"dir": at("rec")})),
        ("file.recovery.list", json!({"dir": dir.to_string_lossy()})),
        ("file.recovery.open", json!({"dir": dir.to_string_lossy()})),
        ("file.package", json!({"dir": at("pkg")})),
        ("links.copyTo", json!({"dir": at("copies")})),
        ("links.relinkFolder", json!({"dir": dir.to_string_lossy()})),
        ("file.open", json!({"path": at("secret.txt")})),
        ("file.openIdml", json!({"path": at("secret.txt")})),
        ("book.open", json!({"path": at("secret.txt")})),
        ("library.open", json!({"path": at("secret.txt")})),
        ("xml.loadDtd", json!({"path": at("secret.txt")})),
        ("file.importXml", json!({"path": at("secret.txt")})),
        ("swatch.load", json!({"path": at("secret.txt")})),
        ("color.loadProfile", json!({"path": at("secret.txt")})),
        ("data.source.select", json!({"path": at("secret.txt")})),
        ("file.place", json!({"path": at("secret.png")})),
        ("file.place", json!({"path": "../secret.png"})),
        ("file.place", json!({"path": format!("file://{}", at("secret.png"))})),
        ("file.place", json!({"path": ".hidden.png"})),
        ("place.load", json!({"path": at("secret.png")})),
        ("snippet.place", json!({"path": at("secret.txt")})),
        ("place.styles", json!({"path": at("secret.txt")})),
        // Printing starts the spooler: not even a dry run, which makes a temporary file.
        ("file.printers", json!({})),
        ("file.print", json!({})),
        ("file.print", json!({"dryRun": true})),
    ];
    for (command, params) in &refused {
        let answer = s.execute(command, params);
        assert!(answer.is_err(), "{command} {params}: {answer:?}");
        assert!(!format!("{answer:?}").contains("SENTINEL"), "{command}: {answer:?}");
    }
    assert_eq!(files(&dir), ["secret.png", "secret.txt"], "nothing was written beside the sentinels");
    // What the host lends is placed, by its path in the workspace.
    let placed = s.execute("file.place", &json!({"path": "inside.png", "x": 300, "y": 300})).unwrap();
    assert!(placed["asset"].is_u64(), "{placed}");
    let links = s.execute("links.list", &json!({})).unwrap();
    assert_eq!(links[0]["status"], "ok", "{links}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_hosted_session_reports_links_outside_the_workspace_missing() {
    let dir = folder("links");
    let mut s = hosted();
    let png = inside_png();
    for (link, x) in [(dir.join("secret.png").to_string_lossy().into_owned(), 72), ("inside.png".to_owned(), 300)] {
        s.execute("edit.deselectAll", &json!({})).unwrap();
        s.execute("file.place", &json!({"base64": crate::cmd::base64_encode(&png), "name": "p.png", "x": x, "y": 72})).unwrap();
        s.edit(|d, sel| {
            let item =
                sel.items.first().copied().and_then(|id| d.item(id)).ok_or(kition_layout_doc::DocError::NoItem(kition_layout_doc::ItemId(0)))?;
            let kition_layout_doc::Content::Graphic(g) = &item.content else { return Ok(()) };
            let aid = g.asset;
            if let Some(asset) = d.assets.get_mut(&aid) {
                Arc::make_mut(asset).link = Some(link.clone());
            }
            Ok(())
        })
        .unwrap();
    }
    let listed = s.execute("links.list", &json!({})).unwrap();
    let status: Vec<&str> = listed.as_array().unwrap().iter().map(|l| l["status"].as_str().unwrap()).collect();
    let mut sorted = status.clone();
    sorted.sort();
    assert_eq!(sorted, ["missing", "ok"], "the link outside is missing, never read: {listed}");
    // Updating, relinking by extension and relinking to the file outside read nothing by path.
    s.execute("links.update", &json!({})).unwrap();
    let _ = s.execute("links.relinkFolder", &json!({"extension": "png"}));
    let asset = listed.as_array().unwrap().iter().find(|l| l["status"] == "missing").unwrap()["asset"].clone();
    assert!(s.execute("links.relink", &json!({"asset": asset, "path": dir.join("secret.png").to_string_lossy()})).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_hosted_session_reads_an_idml_package_links_only_through_its_host() {
    let dir = folder("idml");
    // A package linking a picture beside it, one by absolute path, one by `../` and a `file:` URL.
    let mut maker = Session::new();
    maker.execute("file.new", &json!({})).unwrap();
    let png = inside_png();
    let links = [
        "inside.png".to_owned(),
        dir.join("secret.png").to_string_lossy().into_owned(),
        "../secret.png".to_owned(),
        format!("file://{}", dir.join("secret.png").to_string_lossy()),
    ];
    for (n, link) in links.iter().enumerate() {
        maker.execute("edit.deselectAll", &json!({})).unwrap();
        maker.execute("file.place", &json!({"base64": crate::cmd::base64_encode(&png), "name": "p.png", "x": 72 + 100 * n, "y": 72})).unwrap();
        maker
            .edit(|d, sel| {
                let id = sel.items.first().copied().ok_or(kition_layout_doc::DocError::NoItem(kition_layout_doc::ItemId(0)))?;
                let Some(kition_layout_doc::Content::Graphic(g)) = d.item(id).map(|it| it.content.clone()) else { return Ok(()) };
                if let Some(asset) = d.assets.get_mut(&g.asset) {
                    // `Links/…` so the package keeps the path: the importer drops a bare name.
                    Arc::make_mut(asset).link = Some(if n == 0 { format!("Links/{link}") } else { link.clone() });
                }
                Ok(())
            })
            .unwrap();
    }
    let idml = kition_layout_idml::export_idml_with(&maker.doc().unwrap().doc, &kition_layout_idml::ExportOptions { embed_images: false });
    let mut s = Session::for_host(HOST);
    s.execute("file.openBytes", &json!({"name": "linked.idml", "base64": crate::cmd::base64_encode(&idml)})).unwrap();
    let doc = &s.doc().unwrap().doc;
    let mut read: Vec<bool> = doc.assets.values().map(|a| !a.data.is_empty()).collect();
    read.sort();
    assert_eq!(read, [false, false, false, true], "only the picture the host lends is read");
    // And as a placed page of a layout.
    let mut s = hosted();
    s.execute("file.place", &json!({"name": "linked.idml", "base64": crate::cmd::base64_encode(&idml)})).unwrap();
    let mut read: Vec<bool> = s.doc().unwrap().doc.assets.values().map(|a| !a.data.is_empty()).collect();
    read.sort();
    assert_eq!(read, [false, false, false, true]);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Refuses `file.saveACopy` and `script.run`'s neighbour `file.recovery.save` to anyone but a
/// person: a stand-in for the kit's list.
fn gate(command: &str) -> std::result::Result<(), String> {
    if matches!(command, "file.saveACopy" | "file.recovery.save") { Err(format!("`{command}` is not available to the agent")) } else { Ok(()) }
}

#[test]
fn every_step_an_agent_runs_passes_the_gate() {
    let dir = folder("gate");
    let copy = dir.join("copy.klayout").to_string_lossy().into_owned();
    let mut s = Session::new();
    s.agent_gate = Some(gate);
    s.execute("file.new", &json!({})).unwrap();
    let step = format!("file.saveACopy {}", json!({"path": copy}));
    let nested = json!({"text": format!("script.run {}", json!({"text": step}))});
    for origin in [
        Origin::agent("agent:test"),
        Origin::system("host"),
        kition_layout_kdm::kition_kit::kdm::Origin::new(kition_layout_kdm::kition_kit::kdm::OriginKind::Extension, "script"),
    ] {
        assert!(s.execute_as("file.saveACopy", &json!({"path": copy}), &origin).is_err());
        assert!(s.execute_as("script.run", &json!({"text": step}), &origin).is_err(), "a script's step");
        assert!(s.execute_as("script.run", &nested, &origin).is_err(), "a nested script's step");
        assert!(s.execute_as("frame.create", &json!({"rect": [72, 72, 100, 100]}), &origin).is_ok(), "what the gate allows runs");
    }
    assert_eq!(files(&dir), ["secret.png", "secret.txt"]);
    // A person runs what the gate keeps from agents (here, with no host, on the file system).
    s.execute_as("script.run", &json!({"text": step}), &Origin::person("person:local")).unwrap();
    assert!(Path::new(&copy).is_file());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Strips `#[cfg(test)]` / `#[cfg(all(test, …))]` items from Rust source: what is compiled into
/// the engine.
fn without_tests(source: &str) -> String {
    let mut out = String::new();
    let mut rest = source;
    while let Some(at) = ["#[cfg(test)]", "#[cfg(all(test"].iter().filter_map(|marker| rest.find(marker)).min() {
        out.push_str(&rest[..at]);
        let item = &rest[at..];
        // Skip to the item's closing brace, ignoring braces in strings, chars and comments.
        let (mut depth, mut started, mut end) = (0i32, false, item.len());
        let bytes = item.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    i = item[i..].find('\n').map_or(bytes.len(), |n| i + n);
                }
                b'r' if !bytes.get(i.wrapping_sub(1)).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                    && matches!(bytes.get(i + 1), Some(b'#' | b'"')) =>
                {
                    // A raw string: `r"…"`, `r#"…"#`.
                    let hashes = item[i + 1..].bytes().take_while(|b| *b == b'#').count();
                    if bytes.get(i + 1 + hashes) == Some(&b'"') {
                        let close = format!("\"{}", "#".repeat(hashes));
                        let from = i + 2 + hashes;
                        i = item[from..].find(&close).map_or(bytes.len(), |n| from + n + close.len() - 1);
                    }
                }
                b'"' => {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'"' {
                        i += if bytes[i] == b'\\' { 2 } else { 1 };
                    }
                }
                b'\'' if bytes.get(i + 2) == Some(&b'\'') || (bytes.get(i + 1) == Some(&b'\\') && bytes.get(i + 3) == Some(&b'\'')) => {
                    i += if bytes.get(i + 1) == Some(&b'\\') { 3 } else { 2 };
                }
                b'{' => {
                    depth += 1;
                    started = true;
                }
                b'}' => {
                    depth -= 1;
                    if started && depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                b';' if !started => {
                    end = i + 1;
                    break;
                }
                _ => {}
            }
            i += 1;
        }
        rest = &item[end.min(item.len())..];
    }
    out.push_str(rest);
    out
}

/// The engine reaches the file system, starts programs or opens connections only in [`crate::disk`],
/// which refuses all of it in a hosted session, and in the code that refuses a hosted session
/// before anything else (`disk::need` first): packages, recovery data, the print spooler.
#[test]
fn the_engine_reaches_files_and_programs_only_through_its_door() {
    const DOOR: &str = "disk.rs";
    /// Files that touch the file system or start programs behind a `disk::need` at every entry.
    const GUARDED: &[&str] = &["package.rs", "recovery.rs", "print.rs"];
    const REACH: &[&str] = &[
        "std::fs::",
        "std::process::",
        "Command::new",
        "tempfile::",
        "std::net::",
        "TcpStream",
        "UdpSocket",
        "File::open",
        "File::create",
        "OpenOptions",
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![root.clone()];
    let mut found = Vec::new();
    // What the scan reads is the engine's code, its tests left out: it sees the door's callers.
    let mut lent_reads = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !name.ends_with(".rs") || name.starts_with("tests") || name == DOOR || dir.ends_with("datamerge") && name == "tests.rs" {
                continue;
            }
            let source = without_tests(&std::fs::read_to_string(&path).unwrap());
            lent_reads += source.matches("crate::disk::read_lent(").count();
            assert!(!source.contains("#[test]"), "{name}: a test was left in the scanned code");
            for (n, line) in source.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if let Some(reach) = REACH.iter().find(|reach| code.contains(*reach))
                    && !(GUARDED.contains(&name.as_str()))
                {
                    found.push(format!("{}:{}: {reach}", path.strip_prefix(&root).unwrap().display(), n + 1));
                }
            }
            if GUARDED.contains(&name.as_str()) {
                assert!(source.contains("crate::disk::need("), "{name} refuses a hosted session first");
            }
        }
    }
    assert!(lent_reads >= 4, "the scan saw the engine's reads through its host ({lent_reads})");
    assert!(found.is_empty(), "file system, program or network access outside the engine's door: {found:#?}");
}
