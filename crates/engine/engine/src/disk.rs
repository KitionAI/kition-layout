//! The engine's door to files named by path, and the host-files-only mode (KHP A29, A42, A45,
//! section 10.6).
//!
//! A session made for a host ([`crate::Session::for_host`]: a kit's document, served headless or
//! in the page) works only on the files its host owns. While such a session runs a command
//! ([`enter`]), the engine
//!
//! - reads a file a command or a document names (a placed graphic, a link, an IDML package's
//!   linked resources, a data merge source and the images it names) only through the host
//!   ([`read_lent`]), by a path inside the workspace: absolute paths, URLs (`file:`, `http:`, …)
//!   and hidden names are refused here, and the host resolves the rest against the document's
//!   folder and refuses anything outside the workspace (`..` that climbs out, a link that leads
//!   out);
//! - reaches nothing else by path: no read, write, listing, folder, deletion or file status
//!   ([`read`], [`write`], [`is_file`], …, and [`need`] before what touches the file system in
//!   bulk: packages, recovery data, books, document fonts, colour profiles), and starts no
//!   program (printing, [`need`]).
//!
//! Outside such a session (the engine's own tests and tools) these are the file system's.

use std::cell::Cell;
use std::path::Path;

use crate::{EngineError, Result};

/// How a hosted session reads the files its host lends, by the path a document or a command names
/// (`kition_kit::host::size` and `read` for a kit).
#[derive(Clone, Copy)]
pub struct HostFiles {
    pub size: fn(&str) -> std::result::Result<u64, String>,
    pub read: fn(&str) -> std::result::Result<Vec<u8>, String>,
}

impl std::fmt::Debug for HostFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostFiles")
    }
}

thread_local! {
    /// The host of the hosted session running a command on this thread, if any.
    static HOST: Cell<Option<HostFiles>> = const { Cell::new(None) };
}

/// While a session runs: hosted when `host` is `Some`, and a hosted scope stays hosted (a session
/// made inside one, such as a scratch copy, reaches no more than its caller).
pub(crate) struct Scope(Option<HostFiles>);

pub(crate) fn enter(host: Option<HostFiles>) -> Scope {
    let previous = HOST.get();
    if host.is_some() {
        HOST.set(host);
    }
    Scope(previous)
}

impl Drop for Scope {
    fn drop(&mut self) {
        HOST.set(self.0);
    }
}

fn host() -> Option<HostFiles> {
    HOST.get()
}

/// Whether the command running now belongs to a hosted session.
pub fn hosted() -> bool {
    host().is_some()
}

/// The error for `what` in a hosted session: it would reach files or folders by path, or start a
/// program, and the host owns every file (KHP A29, A45; the message says what to do, A48).
pub fn refusal(what: &str) -> EngineError {
    EngineError::Other(format!(
        "{what} is not available here: this kit works only on files of its workspace, through its host, and starts no program; \
         open, save and export documents through the host, and name pictures and text by their path in the workspace"
    ))
}

/// Refuses `what` in a hosted session.
pub fn need(what: &str) -> Result<()> {
    if hosted() { Err(refusal(what)) } else { Ok(()) }
}

fn denied(path: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{path}: outside this kit's workspace"))
}

/// Why `path` may not be lent: it is not a plain path inside the workspace (an absolute path, a
/// URL or a drive, a hidden name, an empty or overlong name). A granted file's handle passes.
pub fn not_lent(path: &str) -> Option<String> {
    const MAX: usize = 4096;
    let refuse = |why: &str| Some(format!("{path}: {why}; name a file in the workspace by its path"));
    if path.trim().is_empty() || path.len() > MAX || path.contains('\0') {
        return refuse("not a file name");
    }
    if path.starts_with(['/', '\\']) {
        return refuse("an absolute path is outside the workspace");
    }
    // `file:`, `http:`, `C:`: a scheme or a drive, never fetched.
    if let Some((scheme, _)) = path.split_once(':')
        && !scheme.is_empty()
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return refuse("a URL or a drive is not a file of the workspace");
    }
    if path.split(['/', '\\']).any(|part| part.starts_with('.') && part != "." && part != "..") {
        return refuse("hidden files are not lent");
    }
    None
}

/// A file a hosted session reads: through the host, by its path in the workspace. Elsewhere the
/// file at `path`.
pub fn read_lent(path: &str) -> std::result::Result<Vec<u8>, String> {
    match host() {
        Some(host) => match not_lent(path) {
            Some(why) => Err(why),
            None => (host.read)(path),
        },
        None => std::fs::read(path).map_err(|e| format!("{path}: {e}")),
    }
}

/// The size of the file [`read_lent`] reads.
pub fn lent_len(path: &str) -> std::result::Result<u64, String> {
    match host() {
        Some(host) => match not_lent(path) {
            Some(why) => Err(why),
            None => (host.size)(path),
        },
        None => std::fs::metadata(path).map(|meta| meta.len()).map_err(|e| format!("{path}: {e}")),
    }
}

/// Whether [`read_lent`] finds a file at `path`.
pub fn is_lent(path: &str) -> bool {
    match host() {
        Some(_) => lent_len(path).is_ok(),
        None => Path::new(path).is_file(),
    }
}

/// The file at `path`, never through the host: refused in a hosted session.
pub fn read(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    let path = path.as_ref();
    if hosted() {
        return Err(denied(&path.to_string_lossy()));
    }
    std::fs::read(path)
}

/// [`read`] as text.
pub fn read_to_string(path: impl AsRef<Path>) -> std::io::Result<String> {
    let path = path.as_ref();
    if hosted() {
        return Err(denied(&path.to_string_lossy()));
    }
    std::fs::read_to_string(path)
}

/// Writes the file at `path`: refused in a hosted session.
pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    let path = path.as_ref();
    if hosted() {
        return Err(denied(&path.to_string_lossy()));
    }
    std::fs::write(path, bytes)
}

/// The file status at `path`: refused in a hosted session.
pub fn metadata(path: impl AsRef<Path>) -> std::io::Result<std::fs::Metadata> {
    let path = path.as_ref();
    if hosted() {
        return Err(denied(&path.to_string_lossy()));
    }
    std::fs::metadata(path)
}

/// The entries of the folder at `path`: refused in a hosted session.
pub fn read_dir(path: impl AsRef<Path>) -> std::io::Result<std::fs::ReadDir> {
    let path = path.as_ref();
    if hosted() {
        return Err(denied(&path.to_string_lossy()));
    }
    std::fs::read_dir(path)
}

/// Whether a file is at `path` (never, to a hosted session).
pub fn is_file(path: impl AsRef<Path>) -> bool {
    !hosted() && path.as_ref().is_file()
}

/// Whether a folder is at `path` (never, to a hosted session).
pub fn is_dir(path: impl AsRef<Path>) -> bool {
    !hosted() && path.as_ref().is_dir()
}

/// Whether anything is at `path` (nothing, to a hosted session).
pub fn exists(path: impl AsRef<Path>) -> bool {
    !hosted() && path.as_ref().exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lend(_: &str) -> std::result::Result<Vec<u8>, String> {
        Ok(b"lent".to_vec())
    }

    fn size(_: &str) -> std::result::Result<u64, String> {
        Ok(4)
    }

    const HOST: HostFiles = HostFiles { size, read: lend };

    #[test]
    fn a_hosted_scope_lends_plain_workspace_paths_only() {
        let _scope = enter(Some(HOST));
        assert_eq!(read_lent("pictures/a.png").unwrap(), b"lent");
        assert_eq!(read_lent("fh_123").unwrap(), b"lent");
        // `..` is the host's to confine: it resolves against the document's folder.
        assert!(read_lent("../a.png").is_ok());
        for path in [
            "/etc/passwd",
            "\\\\server\\share\\a.png",
            "file:///etc/passwd",
            "file:a.png",
            "http://example.com/a.png",
            "C:\\a.png",
            ".hidden.png",
            "a/.git/config",
            "",
            "a\0b",
        ] {
            assert!(read_lent(path).is_err(), "{path:?}");
            assert!(!is_lent(path), "{path:?}");
        }
        assert!(read("Cargo.toml").is_err() && write("x", b"x").is_err() && metadata(".").is_err() && read_dir(".").is_err());
        assert!(!is_file("Cargo.toml") && !is_dir(".") && !exists("."));
        assert!(need("file.print").is_err());
    }

    #[test]
    fn a_scope_stays_hosted_until_it_ends() {
        {
            let _hosted = enter(Some(HOST));
            let _inner = enter(None);
            assert!(hosted(), "a session made inside a hosted one reaches no more");
        }
        assert!(!hosted());
        assert!(is_dir("."));
    }
}
