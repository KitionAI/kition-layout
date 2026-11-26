//! `file.exportBytes`: the document in an export format, answered as base64 and never written
//! to a path. The host writes it as a new file (Kit UI `host/write` mode `export`, the agent's
//! `layout_export`); `kit.json` `exports` lists the same formats, which is what lets Kition write
//! them at all.

use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc, str_param};
use crate::{Result, Session};

const ID: &str = "file.exportBytes";

/// The formats, as `kit.json` `exports[].extension` names them.
pub const FORMATS: &[&str] = &["pdf", "png", "jpg", "html", "idml", "epub"];

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(query "file.exportBytes", "Export as Bytes", [], None,
        "{format: pdf|png|jpg|html|idml|epub, pages?: \"1-3,5\" | [1,3] (PDF, PNG, JPEG: 1-based; pictures default to the first page), bleed?: bool, ppi?: 72 (PNG, JPEG), background?: paper|transparent (PNG), quality?: 1..100 (JPEG), title?, author?, language?, cover?: bool (EPUB), fixedLayout?: bool (EPUB), embedImages?: bool (IDML), tagged?: bool (PDF), spreads?: bool (PDF: each spread on one sheet), marks?: bool | {crop?: bool, bleed?: bool, pageInfo?: bool} (PDF printer's marks), standard?: none|x4|a2b (PDF/X-4, PDF/A-2b), compressImages?: bool (PDF: images as JPEG)} — the file as base64, without writing it → {format, data, bytes, files?: [{page, data}] (pictures of several pages)}",
        has_doc, export_bytes)]
}

fn export_bytes(s: &mut Session, p: &Value) -> Result<Value> {
    let format = str_param(p, "format").map(str::to_ascii_lowercase).ok_or_else(|| bad(ID, format!("`format` is one of {}", FORMATS.join(", "))))?;
    // Never a path: the host writes the file.
    let mut params = p.as_object().cloned().unwrap_or_default();
    params.remove("path");
    params.remove("format");
    let answered = |answer: Value, field: &str| -> Result<Vec<u8>> {
        match answer.get(field).and_then(Value::as_str) {
            Some(text) if field == "text" => Ok(text.as_bytes().to_vec()),
            Some(data) => Ok(super::base64_decode(data)),
            None => Err(bad(ID, format!("the {format} export answered no file"))),
        }
    };
    let bytes = match format.as_str() {
        "pdf" => answered(super::export::export_pdf(s, &Value::Object(params))?, "base64")?,
        "html" => answered(super::export::export_html(s, &Value::Object(params))?, "text")?,
        "epub" => answered(super::export::export_epub(s, &Value::Object(params))?, "base64")?,
        "idml" => answered(super::interchange::export_idml(s, &Value::Object(params))?, "base64")?,
        "png" | "jpg" | "jpeg" => {
            let jpeg = format != "png";
            params.insert("format".into(), json!(if jpeg { "jpeg" } else { "png" }));
            params.entry("pages").or_insert(json!([1]));
            let answer = super::export_image::export_image(s, &Value::Object(params))?;
            let files: Vec<(u64, String)> = answer["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|file| Some((file.get("page")?.as_u64()?, file.get("base64")?.as_str()?.to_owned())))
                .collect();
            let (_, first) = files.first().ok_or_else(|| bad(ID, "no page to export"))?;
            let format = if jpeg { "jpg" } else { "png" };
            let size = super::base64_decode(first).len();
            let mut out = json!({ "format": format, "data": first, "bytes": size });
            if files.len() > 1 {
                out["files"] = files.iter().map(|(page, data)| json!({ "page": page, "data": data })).collect();
            }
            return Ok(out);
        }
        other => return Err(bad(ID, format!("unknown format `{other}` ({})", FORMATS.join(", ")))),
    };
    Ok(json!({ "format": format, "data": super::base64_encode(&bytes), "bytes": bytes.len() }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    fn sample() -> Session {
        let mut s = Session::new();
        s.execute("file.newSample", &json!({})).unwrap();
        s
    }

    fn bytes(s: &mut Session, params: serde_json::Value) -> Vec<u8> {
        let answer = s.execute("file.exportBytes", &params).unwrap();
        let data = crate::cmd::base64_decode(answer["data"].as_str().unwrap());
        assert_eq!(answer["bytes"].as_u64().unwrap() as usize, data.len());
        data
    }

    /// The first local entry of a ZIP: its name, compression method, extra field length, and data.
    fn first_entry(zip: &[u8]) -> (String, u16, u16, Vec<u8>) {
        assert_eq!(&zip[..4], b"PK\x03\x04", "a ZIP package");
        let u16_at = |at: usize| u16::from_le_bytes([zip[at], zip[at + 1]]);
        let size = u32::from_le_bytes([zip[18], zip[19], zip[20], zip[21]]) as usize;
        let (name_len, extra) = (u16_at(26) as usize, u16_at(28));
        let name = String::from_utf8(zip[30..30 + name_len].to_vec()).unwrap();
        let start = 30 + name_len + extra as usize;
        (name, u16_at(8), extra, zip[start..start + size].to_vec())
    }

    #[test]
    fn idml_is_a_package_that_opens_again() {
        let mut s = sample();
        let idml = bytes(&mut s, json!({ "format": "idml" }));
        let (name, method, _, data) = first_entry(&idml);
        assert_eq!((name.as_str(), method), ("mimetype", 0), "the stored mimetype comes first");
        assert_eq!(data, b"application/vnd.adobe.indesign-idml-package");
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(idml.clone())).unwrap();
        assert!(zip.by_name("designmap.xml").is_ok());
        let back = kition_layout_idml::import_idml(&idml).unwrap();
        assert_eq!(back.page_count(), s.doc().unwrap().doc.page_count());
    }

    #[test]
    fn epub_starts_with_its_uncompressed_mimetype() {
        let mut s = sample();
        for params in [json!({ "format": "epub" }), json!({ "format": "epub", "fixedLayout": true })] {
            let epub = bytes(&mut s, params.clone());
            let (name, method, extra, data) = first_entry(&epub);
            assert_eq!((name.as_str(), method, extra), ("mimetype", 0, 0), "{params}: stored, no extra field (OCF)");
            assert_eq!(data, b"application/epub+zip");
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(epub)).unwrap();
            let mut container = String::new();
            std::io::Read::read_to_string(&mut zip.by_name("META-INF/container.xml").unwrap(), &mut container).unwrap();
            assert!(container.contains("full-path=\"OEBPS/content.opf\""), "{container}");
            assert!(zip.by_name("OEBPS/content.opf").is_ok());
        }
    }

    #[test]
    fn every_declared_format_answers_its_file() {
        let mut s = sample();
        assert!(bytes(&mut s, json!({ "format": "pdf" })).starts_with(b"%PDF-"));
        assert!(bytes(&mut s, json!({ "format": "png" })).starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(bytes(&mut s, json!({ "format": "jpg" })).starts_with(&[0xff, 0xd8]));
        let html = String::from_utf8(bytes(&mut s, json!({ "format": "html" }))).unwrap();
        assert!(html.to_ascii_lowercase().starts_with("<!doctype html"), "{}", &html[..40.min(html.len())]);
        // Pictures of several pages answer each.
        let answer = s.execute("file.exportBytes", &json!({ "format": "png", "pages": "1-2" })).unwrap();
        assert_eq!(answer["files"].as_array().unwrap().len(), 2);
        assert_eq!(answer["format"], "png");
        // The formats are the ones kit.json declares.
        let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../../../kit.json")).unwrap();
        let declared: Vec<&str> = manifest["exports"].as_array().unwrap().iter().map(|e| e["extension"].as_str().unwrap()).collect();
        assert_eq!(declared, super::FORMATS);
    }

    #[test]
    fn never_writes_a_path_and_refuses_unknown_formats() {
        let mut s = sample();
        let dir = std::env::temp_dir().join(format!("kd-export-bytes-{}", std::process::id()));
        let path = dir.join("x.epub");
        let answer = s.execute("file.exportBytes", &json!({ "format": "epub", "path": path.to_string_lossy() })).unwrap();
        assert!(answer["data"].is_string() && !path.exists());
        assert!(s.execute("file.exportBytes", &json!({ "format": "docx" })).unwrap_err().to_string().contains("unknown format"));
        assert!(s.execute("file.exportBytes", &json!({})).is_err());
    }
}
