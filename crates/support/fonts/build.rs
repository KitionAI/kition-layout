//! Optional kition-fonts build input (https://github.com/KitionAI/kition-fonts, recipe from its
//! `docs/integration.md`). With `KITION_FONTS_DIR=<checkout>` the fonts in its
//! `fonts/manifest.txt` are embedded as `KITION_FONTS`; unset, `KITION_FONTS` is empty.
//!
//! The web (wasm32) build embeds none of it: the kit's own editor serves the same fonts to the
//! engine at run time instead (Kition platform E9, `addFont`; see `web/scripts/build-engine.sh`).
use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-env-changed=KITION_FONTS_DIR");
    println!("cargo::rerun-if-env-changed=KITION_FONTS_REQUIRED");
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
    let mut src = String::from("pub static KITION_FONTS: &[KitionFont] = &[\n");
    if let Some(dir) = std::env::var_os("KITION_FONTS_DIR").filter(|d| !d.is_empty() && !wasm).map(PathBuf::from) {
        match kition_fonts(&dir) {
            Ok(entries) => src.push_str(&entries),
            Err(e) if std::env::var_os("KITION_FONTS_REQUIRED").is_some() => {
                println!("cargo::error=KITION_FONTS_DIR={}: {e}", dir.display());
            }
            Err(e) => println!("cargo::warning=building without kition-fonts: KITION_FONTS_DIR={}: {e}", dir.display()),
        }
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("kition_fonts.rs");
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}

/// One `KitionFont { .. }` initialiser per manifest line.
fn kition_fonts(dir: &std::path::Path) -> Result<String, String> {
    let manifest = dir.join("fonts/manifest.txt");
    println!("cargo::rerun-if-changed={}", manifest.display());
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let mut out = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split(" | ").map(str::trim).collect();
        let [family, style, file, scripts, ..] = f.as_slice() else {
            return Err(format!("malformed manifest line: {line}"));
        };
        let path = dir.join(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
        println!("cargo::rerun-if-changed={}", path.display());
        let scripts: Vec<String> = scripts.split(',').map(|s| format!("{:?}", s.trim())).collect();
        let _ = writeln!(
            out,
            "    KitionFont {{ family: {family:?}, style: {style:?}, scripts: &[{}], bytes: include_bytes!({:?}) }},",
            scripts.join(", "),
            path.display().to_string(),
        );
    }
    Ok(out)
}
