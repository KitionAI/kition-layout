//! Starter layouts (File › New from Template): Kition's four poster designs (Editorial, Event,
//! Promotion, Quote) as page-layout documents. Each is built from its description in
//! `templates/*.json` with the engine's own commands: named swatches (Primary, Text, Accent,
//! Surface) take the place of a brand binding, the background is a locked rectangle on the parent,
//! text frames use paragraph styles named after their slot (Label, Headline, Body), and the
//! layout's constraints become object-based liquid rules, so alternate layouts (sizes) follow them.
//! `file.applyTemplate` puts one into the open document; the package ships each as a
//! `.klayout` file too (`web/public/templates/`, made by the `templates` test), the file an
//! agent's `layout_create { from }` starts from.

use std::collections::BTreeMap;

use kition_layout_doc::{Document, Selection};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc, str_param};
use crate::{Result, Session};

/// The templates, in the order the New from Template dialog shows them.
pub const TEMPLATES: [(&str, &str); 4] = [
    ("editorial", include_str!("../../templates/editorial.json")),
    ("event", include_str!("../../templates/event.json")),
    ("sale", include_str!("../../templates/sale.json")),
    ("quote", include_str!("../../templates/quote.json")),
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Template {
    id: String,
    name: String,
    width: f64,
    height: f64,
    /// Swatch name → `#rrggbb`.
    swatches: BTreeMap<String, String>,
    /// The page background's swatch.
    background: Option<String>,
    paragraph_styles: BTreeMap<String, TextStyle>,
    /// Back to front.
    items: Vec<TemplateItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextStyle {
    font: String,
    font_style: String,
    size: f64,
    /// Fixed leading in points.
    leading: f64,
    fill: Option<String>,
}

#[derive(Deserialize, Default)]
struct Constraints {
    horizontal: Option<String>,
    vertical: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateItem {
    /// `rectangle`, `ellipse` or `text`.
    #[serde(rename = "type")]
    kind: String,
    name: String,
    /// `[x, y, width, height]` in points from the page's top-left corner.
    rect: [f64; 4],
    fill: Option<String>,
    stroke: Option<String>,
    stroke_weight: Option<f64>,
    radius: Option<f64>,
    style: Option<String>,
    text: Option<String>,
    #[serde(default)]
    constraints: Constraints,
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "template.list", "Templates", [], None, "{} → [{id, name, width, height}] — the starter layouts `file.applyTemplate` makes", super::always, |_, _| {
            let mut out = Vec::new();
            for (_, text) in TEMPLATES {
                let t = parse(text)?;
                out.push(json!({"id": t.id, "name": t.name, "width": t.width, "height": t.height}));
            }
            Ok(Value::Array(out))
        }),
        cmd!(
            "file.applyTemplate",
            "New from Template…",
            [],
            None,
            "{template: editorial|event|sale|quote} — replaces the document's pages, parents, swatches and styles with a starter layout (one undo step) → {template, name, width, height}",
            has_doc,
            apply_template
        ),
    ]
}

fn parse(text: &str) -> Result<Template> {
    serde_json::from_str(text).map_err(|e| bad("file.applyTemplate", format!("a bundled template is damaged: {e}")))
}

/// The template with `id`.
fn find(id: &str) -> Result<Template> {
    let (_, text) = TEMPLATES
        .iter()
        .find(|(tid, _)| *tid == id)
        .ok_or_else(|| bad("file.applyTemplate", format!("no template `{id}` (editorial, event, sale, quote)")))?;
    parse(text)
}

/// The document template `id` describes, titled `title` (empty: the template's name).
pub fn build(id: &str, title: &str) -> Result<Document> {
    let t = find(id)?;
    let title = if title.is_empty() { t.name.as_str() } else { title };
    let mut s = Session::new();
    let mut run = |command: &str, params: Value| s.execute(command, &params);
    run("file.new", json!({"width": t.width, "height": t.height, "pages": 1, "facingPages": false, "margins": 0, "title": title}))?;
    run("layout.documentSetup", json!({"intent": "web"}))?;
    for (name, color) in &t.swatches {
        run("swatch.add", json!({"name": name, "color": color}))?;
    }
    if let Some(background) = &t.background {
        run("page.background", json!({"color": background}))?;
    }
    run("liquid.pageRule", json!({"rule": "objectBased"}))?;
    for (name, style) in &t.paragraph_styles {
        let mut chars = json!({"fontFamily": style.font, "fontStyle": style.font_style, "size": style.size, "leading": {"kind": "points", "value": style.leading}});
        if let Some(fill) = &style.fill {
            chars["fill"] = json!(fill);
        }
        run("style.paragraph.add", json!({"name": name, "chars": chars, "para": {"hyphenate": false}}))?;
    }
    for item in &t.items {
        let [x, y, w, h] = item.rect;
        let rect = [x, y, x + w, y + h];
        let made = match item.kind.as_str() {
            "text" => run("frame.add", json!({"rect": rect, "content": "text", "text": item.text.clone().unwrap_or_default(), "caret": false}))?,
            "rectangle" | "ellipse" => run("frame.add", json!({"rect": rect, "shape": item.kind, "content": "unassigned"}))?,
            other => return Err(bad("file.applyTemplate", format!("unknown item type `{other}`"))),
        };
        let id = made.get("id").and_then(Value::as_u64).ok_or_else(|| bad("file.applyTemplate", "a frame was not made"))?;
        let ids = json!([id]);
        run("select.set", json!({"ids": ids}))?;
        if let Some(style) = &item.style {
            run("style.paragraph.apply", json!({"name": style, "clearOverrides": true}))?;
        }
        if item.kind != "text" {
            run("object.fill", json!({"swatch": item.fill.as_deref().unwrap_or("[None]"), "ids": ids}))?;
            match (&item.stroke, item.stroke_weight) {
                (Some(stroke), Some(weight)) => run("object.stroke", json!({"swatch": stroke, "weight": weight, "ids": ids}))?,
                _ => run("object.stroke", json!({"swatch": "[None]", "weight": 0, "ids": ids}))?,
            };
            if let Some(radius) = item.radius {
                run("object.cornerOptions", json!({"shape": "rounded", "size": radius, "ids": ids}))?;
            }
        }
        run("object.rename", json!({"id": id, "name": item.name}))?;
        run("liquid.object", liquid(&item.constraints, id))?;
    }
    run("edit.deselectAll", json!({}))?;
    Ok((*s.doc()?.doc).clone())
}

/// Design constraints as object-based liquid settings: `left`/`top` and `right`/`bottom` pin an
/// edge, `scale` resizes with the page, `center` keeps the size and follows the page's middle.
fn liquid(c: &Constraints, id: u64) -> Value {
    let h = c.horizontal.as_deref().unwrap_or("left");
    let v = c.vertical.as_deref().unwrap_or("top");
    json!({
        "ids": [id],
        "pinLeft": h == "left", "pinRight": h == "right", "resizeWidth": h == "scale",
        "pinTop": v == "top", "pinBottom": v == "bottom", "resizeHeight": v == "scale",
    })
}

fn apply_template(s: &mut Session, p: &Value) -> Result<Value> {
    let id = str_param(p, "template").ok_or_else(|| bad("file.applyTemplate", "`template` required (editorial, event, sale, quote)"))?;
    let t = find(id)?;
    let title = s.doc()?.doc.title.clone();
    let built = build(id, &title)?;
    s.edit(|d, sel| {
        *d = built;
        *sel = Selection::default();
        Ok(())
    })?;
    Ok(json!({"template": t.id, "name": t.name, "width": t.width, "height": t.height}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    #[test]
    fn every_template_builds_and_applies_in_one_step() {
        let list = Session::new().execute("template.list", &json!({})).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 4);
        for entry in list.as_array().unwrap() {
            let id = entry["id"].as_str().unwrap();
            let mut s = Session::new();
            s.execute("file.new", &json!({"title": "Mine"})).unwrap();
            let r = s.execute("file.applyTemplate", &json!({"template": id})).unwrap();
            assert_eq!(r["width"], 1080.0);
            let d = s.doc().unwrap().doc.clone();
            d.check().unwrap();
            assert_eq!(d.page_count(), 1);
            assert_eq!(d.title, "Mine", "the document keeps its title");
            assert!(super::super::background::background_of(&d, 0).is_some(), "{id}: a background");
            assert!(d.styles.para("Headline").is_some(), "{id}: slot styles");
            assert!(d.swatch("Text").is_some(), "{id}: named swatches");
            let inspected = s.execute("file.inspect", &json!({})).unwrap();
            let names: Vec<&str> = inspected["spreads"][0]["items"].as_array().unwrap().iter().filter_map(|i| i["name"].as_str()).collect();
            assert!(names.contains(&"Headline") && names.contains(&"Body"), "{id}: {names:?}");
            // Nothing oversets in the bundled fonts' absence either way: the stories exist.
            assert!(inspected["stories"].as_array().unwrap().len() >= 3);
            s.execute("edit.undo", &json!({})).unwrap();
            assert_eq!(s.doc().unwrap().doc.settings.page_width, 612.0, "undo brings the old document back");
            assert_eq!(s.doc().unwrap().kdm_resets(), 0, "{id}: the replica took every write");
        }
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        assert!(s.execute("file.applyTemplate", &json!({"template": "nope"})).is_err());
        assert!(s.execute("file.applyTemplate", &json!({})).is_err());
    }

    #[test]
    fn a_template_follows_its_constraints_into_a_size_variant() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("file.applyTemplate", &json!({"template": "event"})).unwrap();
        s.execute("layout.addAlternate", &json!({"name": "Square", "width": 1080, "height": 1080})).unwrap();
        let inspected = s.execute("file.inspect", &json!({})).unwrap();
        let body = |spread: usize| {
            inspected["spreads"][spread]["items"].as_array().unwrap().iter().find(|i| i["name"] == "Body").map(|i| i["bounds"].clone()).unwrap()
        };
        // The body is pinned to the bottom: 1440 - 1306 = 134 pt from the bottom edge on both.
        let tall = body(0);
        let square = body(1);
        assert!((1440.0 - tall[3].as_f64().unwrap() - (1080.0 - square[3].as_f64().unwrap())).abs() < 0.5, "{tall} {square}");
    }
}
