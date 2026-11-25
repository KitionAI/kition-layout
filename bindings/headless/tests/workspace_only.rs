//! `kition-layout` reads and writes only inside its workspace, and only through its host (KHP A29,
//! A37, A42, A45, section 10.6): whatever an agent runs, nothing appears in the process's current
//! folder, its home folder or its temporary folder, nothing outside the workspace is listed, read
//! into an answer, a picture or an export, changed or deleted, no program is started, and the
//! workspace holds only what the test put there. An audit found that an agent could start `lpstat`
//! and `lpr` (`file.printers`, `file.print`), write anywhere through `script.run` and
//! `model.batch` (`file.saveACopy`, `file.recovery.save`), and read any file back
//! (`file.place` of a text file, then `file.serialize`); a document's links (IDML
//! `LinkResourceURI`, a layout's linked graphics and data merge source) were read by path.
//!
//! The test runs itself again as a child process whose current folder, `HOME` and `TMPDIR` are
//! fresh empty folders and whose `PATH` starts with fake `lpr`, `lpstat`, `lp`, `open` and
//! `xdg-open` that only record that they ran, so every by-path write lands where it can be seen and
//! no real printer is ever reached. A folder beside them (`outside/`) holds this test's own
//! sentinel files, which the agent names by path (directly, inside `model.batch` as a script, as
//! the agent and as a person it claims to be, and inside `script.run`) and which the workspace's
//! documents link to (absolute paths, `../`, `file:` URLs, a remote URL, a hidden file, a symbolic
//! link out). The child drives the backend the executable serves ([`kition_layout::backend`]).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use kition_headless::{Headless, HostFolder};
use kition_kit::KitDocument;
use kition_kit_server::protocol::{Origin, ScreenshotScope};
use kition_kit_server::{KitBackend, OpenRequest, ScreenshotRequest};
use kition_layout_kit::LayoutDocument;
use serde_json::{Map, Value, json};

/// Set in the child: the folder holding `workspace/` and `outside/`.
const CHILD: &str = "KITION_LAYOUT_TEST_SANDBOX";
/// Text only files outside the workspace hold: it must reach no answer.
const SECRET: &str = "SENTINEL-5c3e";
/// A file outside the workspace that no call names: only a listing could find it.
const LISTED_ONLY: &str = "listed-only-5c3e";
/// A font family only the home folder's font folders hold: only a system font scan finds it.
const HOME_FONT: &str = "Planted Home Face 5c3e";
/// The pictures outside the workspace, the one inside it, and what a linked graphic showed when
/// its layout was made.
const OUTSIDE_RGB: [u8; 3] = [17, 233, 71];
const INSIDE_RGB: [u8; 3] = [201, 40, 160];
const BENIGN_RGB: [u8; 3] = [40, 90, 220];
/// The programs the kit may try to start, faked first on `PATH`.
const FAKES: &[&str] = &["lpr", "lpstat", "lp", "open", "xdg-open"];
/// What the workspace holds: what the test put there, nothing the kit wrote.
const WORKSPACE: &[&str] = &[".hidden.png", "basic.klayout", "entity.idml", "hostile.klayout", "inside.png", "link.png", "linked.idml"];

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode(text: &str) -> Option<Vec<u8>> {
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for byte in text.trim_end_matches('=').bytes() {
        let value = B64.iter().position(|known| *known == byte)?;
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

fn encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Every file and folder under `dir`, relative to it, with a file's bytes (a link's target).
fn tree(dir: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().into_owned();
            let link = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink());
            if link {
                out.push((name, std::fs::read_link(&path).ok().map(|target| target.to_string_lossy().into_owned().into_bytes())));
            } else if path.is_dir() {
                stack.push(path.clone());
                out.push((name, None));
            } else {
                out.push((name, std::fs::read(&path).ok()));
            }
        }
    }
    out.sort();
    out
}

fn names(dir: &Path) -> Vec<String> {
    tree(dir).into_iter().map(|(name, _)| name).collect()
}

/// A 64 x 64 PNG of one colour.
fn png(rgb: [u8; 3]) -> Vec<u8> {
    let picture = image::RgbaImage::from_pixel(64, 64, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Whether some pixel of `rgba` is within 8 of `rgb` in every channel.
fn shows(rgba: &[u8], rgb: [u8; 3]) -> bool {
    rgba.as_chunks::<4>().0.iter().any(|px| (0..3).all(|c| px[c].abs_diff(rgb[c]) <= 8))
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn at(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// `bytes`, an IDML package, with each XML part rewritten by `edit` (its name, its text).
fn rewrite_idml(bytes: &[u8], mut edit: impl FnMut(&str, String) -> String) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).unwrap();
        let name = entry.name().to_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if name.ends_with(".xml") {
            data = edit(&name, String::from_utf8(data).unwrap()).into_bytes();
        }
        let method = if name == "mimetype" { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
        out.start_file(name, zip::write::SimpleFileOptions::default().compression_method(method)).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// `bytes`, an IDML package, with each graphic's `LinkResourceURI` that names `file` set to `to`.
fn relink_idml(bytes: &[u8], links: &[(&str, String)]) -> Vec<u8> {
    let mut relinked = 0;
    let out = rewrite_idml(bytes, |name, mut text| {
        assert!(!text.contains("<Contents>"), "{name}: the package embeds a graphic");
        for (file, to) in links {
            let mut from = 0;
            while let Some(start) = text[from..].find("LinkResourceURI=\"").map(|i| from + i + "LinkResourceURI=\"".len()) {
                let end = start + text[start..].find('"').unwrap();
                if text[start..end].ends_with(&format!("/{file}")) {
                    text.replace_range(start..end, to);
                    relinked += 1;
                }
                from = start;
            }
        }
        text
    });
    assert_eq!(relinked, links.len(), "each link was rewritten once");
    out
}

/// A layout whose one text frame holds `text`.
fn layout_with_text(text: &str) -> LayoutDocument {
    let mut doc = LayoutDocument::create("planted.klayout", 612, 792).unwrap();
    doc.execute("frame.add", json!({ "rect": [72, 72, 400, 200], "content": "text", "text": text, "caret": false })).unwrap();
    doc
}

/// Places the picture at `path` (an absolute path: the test's own file, read while it builds its
/// fixtures, before any host) as a new frame at `x`, `y`.
fn place(doc: &mut LayoutDocument, path: &str, x: u32, y: u32) {
    doc.execute("edit.deselectAll", json!({})).unwrap();
    doc.execute("file.place", json!({ "path": path, "x": x, "y": y, "width": 200 })).unwrap();
}

/// The workspace's files and the sentinels outside it, made by the parent before the child runs.
fn plant(root: &Path) {
    let workspace = root.join("workspace");
    let outside = root.join("outside");
    std::fs::write(workspace.join("inside.png"), png(INSIDE_RGB)).unwrap();
    std::fs::write(workspace.join(".hidden.png"), png(OUTSIDE_RGB)).unwrap();
    std::fs::copy(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.klayout")), workspace.join("basic.klayout")).unwrap();
    for name in ["secret.png", "secret2.png", "secret3.png", "secret4.png"] {
        std::fs::write(outside.join(name), png(OUTSIDE_RGB)).unwrap();
    }
    // A symbolic link in the workspace that leads out of it.
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.join("secret.png"), workspace.join("link.png")).unwrap();
    #[cfg(not(unix))]
    std::fs::write(workspace.join("link.png"), png(INSIDE_RGB)).unwrap();
    std::fs::write(outside.join("secret.txt"), format!("{SECRET}\n")).unwrap();
    std::fs::write(outside.join(format!("{LISTED_ONLY}.txt")), SECRET).unwrap();
    std::fs::write(outside.join("sub/keep.txt"), SECRET).unwrap();
    std::fs::write(outside.join("secret.dtd"), format!("<!ELEMENT {SECRET} (#PCDATA)>\n")).unwrap();
    std::fs::write(outside.join("secret.xml"), format!("<Root><{SECRET}>{SECRET}</{SECRET}></Root>\n")).unwrap();
    std::fs::write(outside.join("book.dcbook"), format!("{{\"documents\": [\"{SECRET}.klayout\"], \"styleSource\": 0}}")).unwrap();
    std::fs::write(outside.join("lib.dclib"), format!("{{\"items\": [{{\"name\": \"{SECRET}\", \"description\": \"\", \"snippet\": \"\"}}]}}"))
        .unwrap();
    std::fs::write(outside.join("planted.klayout"), layout_with_text(SECRET).save(None).unwrap()).unwrap();
    std::fs::write(outside.join("planted.idml"), layout_with_text(SECRET).save(Some("planted.idml")).unwrap()).unwrap();

    // An IDML package linking a picture in the workspace and three outside it: by absolute path,
    // by `../`, and by a `file:` URL.
    let mut doc = LayoutDocument::create("linked.klayout", 612, 792).unwrap();
    place(&mut doc, &at(&workspace, "inside.png"), 72, 72);
    place(&mut doc, &at(&outside, "secret.png"), 320, 72);
    place(&mut doc, &at(&outside, "secret2.png"), 72, 400);
    place(&mut doc, &at(&outside, "secret3.png"), 320, 400);
    let idml = doc.execute("file.exportIdml", json!({ "embedImages": false })).unwrap();
    let idml = decode(idml["base64"].as_str().unwrap()).unwrap();
    let links = [
        ("inside.png", "inside.png".to_owned()),
        ("secret.png", at(&outside, "secret.png")),
        ("secret2.png", "../outside/secret2.png".to_owned()),
        ("secret3.png", format!("file://{}", at(&outside, "secret3.png"))),
    ];
    std::fs::write(workspace.join("linked.idml"), relink_idml(&idml, &links)).unwrap();

    // An IDML package whose story declares an external entity naming a file outside the
    // workspace, and uses it.
    let entity = layout_with_text("Entity here").save(Some("entity.idml")).unwrap();
    let declaration = format!("<!DOCTYPE idPkg:Story [<!ENTITY outside SYSTEM \"file://{}\">]>", at(&outside, "secret.txt"));
    let mut used = 0;
    let entity = rewrite_idml(&entity, |name, text| {
        if !name.starts_with("Stories/") || !text.contains("Entity here") {
            return text;
        }
        used += 1;
        let body = text.find("?>").map_or(0, |end| end + 2);
        format!("{}{declaration}{}", &text[..body], &text[body..]).replace("Entity here", "Entity here &outside;")
    });
    assert_eq!(used, 1, "the story holds the entity");
    std::fs::write(workspace.join("entity.idml"), entity).unwrap();

    // A layout whose data merge source and one linked graphic are files outside the workspace,
    // harmless when it was made; the files then change to the sentinels.
    std::fs::write(outside.join("source.csv"), "name,@photo\nAlice,inside.png\n").unwrap();
    std::fs::write(outside.join("swap.png"), png(BENIGN_RGB)).unwrap();
    let mut doc = LayoutDocument::create("hostile.klayout", 612, 792).unwrap();
    doc.execute("data.source.select", json!({ "path": at(&outside, "source.csv") })).unwrap();
    place(&mut doc, &at(&outside, "swap.png"), 72, 72);
    std::fs::write(workspace.join("hostile.klayout"), doc.save(None).unwrap()).unwrap();
    std::fs::write(outside.join("source.csv"), format!("name,@photo\n{SECRET},{}\n", at(&outside, "secret4.png"))).unwrap();
    std::fs::write(outside.join("swap.png"), png(OUTSIDE_RGB)).unwrap();

    // A font only the home folder's font folders hold.
    let font = kition_layout_fonts::testing::font_with(HOME_FONT, &['A', 'B']).unwrap();
    for folder in ["home/Library/Fonts", "home/.fonts", "home/.local/share/fonts"] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
        std::fs::write(root.join(folder).join("planted.ttf"), &font).unwrap();
    }

    // The programs the kit might start: each records that it ran, and does nothing else.
    for fake in FAKES {
        let script = root.join("bin").join(fake);
        std::fs::write(&script, format!("#!/bin/sh\necho \"$0 $*\" >> \"{}\"\n", at(&root.join("marks"), fake))).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
}

/// The agent's session: the backend the executable serves, over the workspace, and every answer
/// it gave.
struct Agent {
    kit: Headless<LayoutDocument>,
    folder: HostFolder,
    answers: Vec<(String, Value)>,
}

impl Agent {
    fn open(&mut self, name: &str) -> String {
        self.kit.open(OpenRequest { file: &self.folder.file(name).unwrap(), read_only: false, create: None }).unwrap().id
    }

    fn keep(&mut self, what: String, answer: Result<Value, String>) -> Result<Value, String> {
        self.answers.push((what, answer.clone().unwrap_or_else(Value::String)));
        answer
    }

    /// `command` as the agent calls it directly.
    fn run(&mut self, id: &str, command: &str, value: Value) -> Result<Value, String> {
        let answer =
            self.kit.execute(id, command, params(value), Origin::Agent).map(|done| done.result.unwrap_or(Value::Null)).map_err(|e| format!("{e:?}"));
        self.keep(command.to_owned(), answer)
    }

    /// `command` as one `command` operation of `model.batch`, with the batch's `origin` (none: the
    /// object model's default, a script; or one the caller claims).
    fn batch(&mut self, id: &str, command: &str, value: Value, origin: &Value) -> Result<Value, String> {
        let mut request = json!({ "documentId": id, "ops": [{ "op": "command", "id": command, "params": value }] });
        if !origin.is_null() {
            request["origin"] = origin.clone();
        }
        let answer = self.kit.model("model.batch", request).map_err(|e| format!("{e:?}"));
        self.keep(format!("model.batch {command} as {origin}"), answer)
    }

    /// `command` as a step of the kit's own `script.run`.
    fn script(&mut self, id: &str, command: &str, value: &Value) -> Result<Value, String> {
        let answer = self.run(id, "script.run", json!({ "text": format!("{command} {value}") }));
        if let Some(last) = self.answers.last_mut() {
            last.0 = format!("script.run {command}");
        }
        answer
    }

    /// `command` every way an agent reaches it: directly, inside `model.batch` (as a script, as
    /// the agent, as a person it claims to be) and inside `script.run`.
    fn every_way(&mut self, id: &str, command: &str, value: &Value) {
        let _ = self.run(id, command, value.clone());
        for origin in [Value::Null, json!({ "kind": "agent", "principal": "agent:test" }), json!({ "kind": "person", "principal": "person:local" })] {
            let _ = self.batch(id, command, value.clone(), &origin);
        }
        let _ = self.script(id, command, value);
    }

    /// The canvas of document `id`, as RGBA.
    fn picture(&mut self, id: &str) -> Vec<u8> {
        let shot = self.kit.screenshot(id, &ScreenshotRequest { scope: ScreenshotScope::Canvas, max_size: 768, scale: 1.0, target: None }).unwrap();
        image::load_from_memory(&shot.png).unwrap().to_rgba8().into_raw()
    }

    fn check(&mut self, id: &str) -> Value {
        let found = self.kit.check(id).unwrap();
        self.answers.push(("document.check".into(), found.clone()));
        found
    }
}

/// How many diagnostics in `found` have `code`.
fn count(found: &Value, code: &str) -> usize {
    let list = found.get("diagnostics").unwrap_or(found);
    list.as_array().map_or(0, |all| all.iter().filter(|diagnostic| diagnostic["code"] == code).count())
}

/// The value a command's file parameter gets: a sentinel outside the workspace for what it
/// reads, a new name outside it for what it writes.
fn target(command: &str, node: &Value, outside: &Path) -> String {
    let folder = node["kind"] == "folder";
    if folder {
        return match command {
            "file.package" | "linkedFile.copyTo" | "file.recovery.save" | "file.exportIdml" => at(outside, &format!("dir-{command}")),
            _ => outside.to_string_lossy().into_owned(),
        };
    }
    if node["access"] != "read" {
        return at(outside, &format!("made-{command}.out"));
    }
    let name = match command {
        "file.open" | "book.add" | "snippet.place" => "planted.klayout",
        "file.openIdml" => "planted.idml",
        "book.open" => "book.dcbook",
        "library.open" => "lib.dclib",
        "xml.loadDtd" => "secret.dtd",
        "file.importXml" => "secret.xml",
        "data.source.select" | "data.fields" | "data.merge" => "source.csv",
        "linkedFile.relink" | "place.load" => "secret.png",
        _ => "secret.txt",
    };
    at(outside, name)
}

/// What the agent does in the child. Errors are fine; reaching files outside the workspace is not.
/// Returns what went wrong that the child can see (answers and pictures); the parent checks the
/// folders.
fn act(root: &Path) -> Vec<String> {
    let workspace = root.join("workspace");
    let outside = root.join("outside");
    let mut wrong = Vec::new();

    let kit = kition_layout::kit().unwrap();
    let mut agent = Agent { kit: kition_layout::backend(&kit), folder: HostFolder::new(&workspace).unwrap(), answers: Vec::new() };
    agent.kit.set_host(agent.folder.host());
    let id = agent.open("basic.klayout");

    // Pictures by path: outside the workspace every way it can be named (none is placed), and a
    // picture inside it, through the host (placed and shown).
    let hostile_paths = [
        at(&outside, "secret.png"),
        "../outside/secret2.png".to_owned(),
        format!("file://{}", at(&outside, "secret3.png")),
        "sub/../../outside/secret.png".to_owned(),
        ".hidden.png".to_owned(),
        "link.png".to_owned(),
        "http://127.0.0.1:9/secret.png".to_owned(),
    ];
    for (n, path) in hostile_paths.iter().enumerate() {
        let _ = agent.run(&id, "edit.deselectAll", json!({}));
        let placed = agent.run(&id, "file.place", json!({ "path": path, "x": 36 + 70 * n, "y": 500, "width": 60 }));
        if placed.is_ok() {
            wrong.push(format!("file.place placed {path}: {placed:?}"));
        }
    }
    let _ = agent.run(&id, "edit.deselectAll", json!({}));
    let inside = agent.run(&id, "file.place", json!({ "path": "inside.png", "x": 300, "y": 600, "width": 100 }));
    if let Err(error) = &inside {
        wrong.push(format!("file.place of a picture in the workspace failed: {error}"));
    }
    // A text file outside, placed: its text must not come back in the layout.
    let _ = agent.script(&id, "file.place", &json!({ "path": at(&outside, "secret.txt") }));
    let _ = agent.run(&id, "file.serialize", json!({}));
    // An SVG whose image names a picture outside, and an IDML page whose graphics link outside.
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"64\" height=\"64\">\
         <image href=\"{}\" width=\"32\" height=\"64\"/><image xlink:href=\"file://{}\" x=\"32\" width=\"32\" height=\"64\"/></svg>",
        at(&outside, "secret.png"),
        at(&outside, "secret2.png")
    );
    let _ = agent.run(&id, "edit.deselectAll", json!({}));
    let _ = agent.run(&id, "file.place", json!({ "base64": encode(svg.as_bytes()), "name": "outside.svg", "x": 400, "y": 72, "width": 150 }));
    let linked = std::fs::read(workspace.join("linked.idml")).unwrap();
    let _ = agent.run(&id, "file.place", json!({ "base64": encode(&linked), "name": "linked.idml", "x": 36, "y": 36 }));
    let shown = agent.picture(&id);
    if !shows(&shown, INSIDE_RGB) {
        wrong.push("the picture placed from the workspace is not shown".to_owned());
    }
    if shows(&shown, OUTSIDE_RGB) {
        wrong.push("a picture outside the workspace was drawn (file.place, an SVG image, an IDML page)".to_owned());
    }

    // An IDML package in the workspace linking a picture beside it and three outside: the one
    // inside is shown, the others are missing, reported, and never read.
    let idml = agent.open("linked.idml");
    let shown = agent.picture(&idml);
    if !shows(&shown, INSIDE_RGB) {
        wrong.push("linked.idml: the picture its package links in the workspace is not shown".to_owned());
    }
    if shows(&shown, OUTSIDE_RGB) {
        wrong.push("linked.idml: a picture outside the workspace was read into the render".to_owned());
    }
    let found = agent.check(&idml);
    if count(&found, "layout.missing-link") != 3 {
        wrong.push(format!("linked.idml: the three links outside the workspace are not each reported as layout.missing-link: {found}"));
    }
    let _ = agent.run(&idml, "linkedFile.list", json!({}));
    let _ = agent.run(&idml, "linkedFile.update", json!({}));
    let _ = agent.run(&idml, "linkedFile.relinkFolder", json!({ "extension": "png" }));
    if shows(&agent.picture(&idml), OUTSIDE_RGB) {
        wrong.push("linked.idml: linkedFile.update or linkedFile.relinkFolder read a picture outside the workspace".to_owned());
    }

    // An IDML package's external entity is not read: its story says only what the package says.
    let entity = agent.open("entity.idml");
    let text = agent.run(&entity, "file.inspect", json!({})).unwrap_or_default();
    if !text.to_string().contains("Entity here") {
        wrong.push(format!("entity.idml: the story's own text is not there: {text}"));
    }

    // A layout whose data merge source and a linked graphic are files outside the workspace.
    let hostile = agent.open("hostile.klayout");
    for (command, value) in [
        ("linkedFile.list", json!({})),
        ("linkedFile.update", json!({})),
        ("data.source.update", json!({})),
        ("data.fields", json!({})),
        ("data.preview", json!({ "record": 1 })),
        ("data.merge", json!({})),
    ] {
        let _ = agent.run(&hostile, command, value);
    }
    if shows(&agent.picture(&hostile), OUTSIDE_RGB) {
        wrong.push("hostile.klayout: a linked graphic or a merged image outside the workspace was read".to_owned());
    }
    // An IDML package as bytes, opened in the session: its links outside are not read either.
    let _ = agent.run(&hostile, "file.openBytes", json!({ "base64": encode(&linked), "name": "bytes.idml" }));
    if shows(&agent.picture(&hostile), OUTSIDE_RGB) {
        wrong.push("file.openBytes of an IDML package read a picture outside the workspace".to_owned());
    }
    let _ = agent.run(&hostile, "file.serialize", json!({}));
    let _ = agent.kit.model("model.export", json!({ "documentId": hostile })).map(|answer| agent.answers.push(("model.export".into(), answer)));

    // No program is started: printing and the printer list.
    for (command, value) in [("file.printers", json!({})), ("file.print", json!({})), ("file.print", json!({ "dryRun": true }))] {
        agent.every_way(&id, command, &value);
    }
    // Nested scripts reach nothing more than their steps.
    let nested = format!("file.saveACopy {}", json!({ "path": at(&outside, "nested.klayout") }));
    let _ = agent.run(&id, "script.run", json!({ "text": format!("script.run {}", json!({ "text": nested })) }));
    let built = json!([
        { "command": "story.get", "params": { "story": 6 } },
        { "command": "script.run", "params": { "text": format!("file.recovery.save {{\"dir\": \"{}\"}}", at(&outside, "rec-built")) } }
    ]);
    let _ = agent.run(&id, "script.run", json!({ "text": built.to_string() }));

    // Every command that names a file or folder, with that parameter outside the workspace, every
    // way the agent reaches it.
    let asset = inside.as_ref().ok().and_then(|placed| placed.get("asset")).cloned().unwrap_or(Value::Null);
    let commands = LayoutDocument::create("sweep.klayout", 64, 64).unwrap().commands();
    for command in &commands {
        let Some(schema) = command.schema.as_ref().and_then(Value::as_object) else { continue };
        let files: Vec<(&String, &Value)> = schema.iter().filter(|(_, node)| node["type"] == "file").collect();
        if files.is_empty() {
            continue;
        }
        for (name, node) in files {
            let mut value = json!({ name.as_str(): target(&command.id, node, &outside) });
            if command.id == "linkedFile.relink" {
                value["asset"] = asset.clone();
            }
            agent.every_way(&id, &command.id, &value);
            // The same parameter climbing out of the workspace from its folder.
            let climbing = json!({ name.as_str(): format!("../outside/climb-{}", command.id) });
            agent.every_way(&id, &command.id, &climbing);
        }
    }
    // The commands that reach the file system without a file parameter.
    for command in
        ["file.revert", "file.recovery.list", "file.recovery.save", "file.recovery.open", "linkedFile.update", "data.source.update", "book.list"]
    {
        agent.every_way(&id, command, &json!({}));
    }

    // Every command the kit offers the agent, with no parameters.
    let offered: Vec<String> = agent.kit.commands(Some(&id)).unwrap().into_iter().map(|command| command.id).collect();
    for command in offered {
        if command == "file.close" {
            continue;
        }
        let _ = agent.run(&id, &command, json!({}));
    }
    let _ = agent.run(&id, "file.serialize", json!({}));
    let _ = agent.run(&id, "file.exportBytes", json!({ "format": "pdf" }));
    let _ = agent.kit.model("model.export", json!({ "documentId": id })).map(|answer| agent.answers.push(("model.export".into(), answer)));

    // Nothing outside the workspace reached an answer: its files' text, a listing, a font folder of
    // the home folder, a folder of this process, the bytes of a picture outside.
    let home = root.join("home").to_string_lossy().into_owned();
    let tmp = root.join("tmp").to_string_lossy().into_owned();
    let outside_png = png(OUTSIDE_RGB);
    let leaks: [&[u8]; 5] = [SECRET.as_bytes(), LISTED_ONLY.as_bytes(), HOME_FONT.as_bytes(), home.as_bytes(), tmp.as_bytes()];
    for (what, answer) in &agent.answers {
        let mut texts = Vec::new();
        strings(answer, &mut texts);
        for text in texts {
            for leak in leaks {
                if text.windows(leak.len()).any(|window| window == leak) {
                    wrong.push(format!("{what}: an answer holds `{}` from outside the workspace", String::from_utf8_lossy(leak)));
                }
            }
            if text.windows(outside_png.len()).any(|window| window == outside_png.as_slice()) {
                wrong.push(format!("{what}: an answer holds the bytes of a picture outside the workspace"));
            }
        }
    }
    wrong.sort();
    wrong.dedup();
    wrong
}

/// Every string in `value`, and the bytes of each that reads as base64.
fn strings(value: &Value, out: &mut Vec<Vec<u8>>) {
    match value {
        Value::String(text) => {
            out.push(text.as_bytes().to_vec());
            if text.len() >= 16
                && let Some(bytes) = decode(text)
            {
                out.push(bytes);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(fields) => fields.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

#[test]
fn agent_commands_reach_no_file_outside_the_workspace() {
    if let Some(root) = std::env::var_os(CHILD) {
        let wrong = act(&PathBuf::from(root));
        assert!(wrong.is_empty(), "{wrong:#?}");
        return;
    }
    let root = std::env::temp_dir().join(format!("kition-layout-workspace-only-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["workspace", "cwd", "home", "tmp", "outside/sub", "bin", "marks"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let root = std::fs::canonicalize(root).unwrap();
    plant(&root);
    let outside = root.join("outside");
    let planted = tree(&outside);
    let home = tree(&root.join("home"));
    let workspace = tree(&root.join("workspace"));

    let path = format!("{}:/usr/bin:/bin", root.join("bin").display());
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "agent_commands_reach_no_file_outside_the_workspace", "--nocapture", "--test-threads=1"])
        .current_dir(root.join("cwd"))
        .env(CHILD, &root)
        .env("HOME", root.join("home"))
        .env("TMPDIR", root.join("tmp"))
        .env("XDG_CONFIG_HOME", root.join("home/.config"))
        .env("XDG_DATA_HOME", root.join("home/.local/share"))
        .env("XDG_CACHE_HOME", root.join("home/.cache"))
        .env("PATH", path)
        .status()
        .unwrap();
    let mut wrong = Vec::new();
    if !status.success() {
        wrong.push(format!("the agent's session found something outside the workspace (see its output above): {status}"));
    }
    for dir in ["cwd", "tmp"] {
        let found = names(&root.join(dir));
        if !found.is_empty() {
            wrong.push(format!("the kit wrote into its {dir} folder: {found:?}"));
        }
    }
    if tree(&root.join("home")) != home {
        wrong.push(format!("the kit wrote into its home folder: {:?}", names(&root.join("home"))));
    }
    let started = names(&root.join("marks"));
    if !started.is_empty() {
        wrong.push(format!("the kit started programs: {started:?}"));
    }
    let now = tree(&outside);
    if now != planted {
        let before: Vec<&String> = planted.iter().map(|(name, _)| name).collect();
        let after: Vec<&String> = now.iter().map(|(name, _)| name).collect();
        wrong.push(format!("the folder outside the workspace changed: before {before:?}, after {after:?}"));
    }
    let found = names(&root.join("workspace"));
    if found != WORKSPACE {
        wrong.push(format!("the workspace holds {found:?}, not only what the test put there ({WORKSPACE:?})"));
    } else if tree(&root.join("workspace")) != workspace {
        wrong.push("a file in the workspace changed".to_owned());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    let _ = std::fs::remove_dir_all(&root);
}
