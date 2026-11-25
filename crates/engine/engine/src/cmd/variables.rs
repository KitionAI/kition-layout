//! Type → Text Variables: define, list, delete and insert variables.

use kition_layout_doc::vars::{TextVariable, VarKind, var_char};
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc, str_param};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "variable.list", "Text Variables", [], None, "{} → [{index, name, type, …, value (page 1)}]", has_doc, |s, _| {
            let d = &s.doc()?.doc;
            Ok(Value::Array(
                d.text_variables
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let mut o = serde_json::to_value(v).unwrap_or_default();
                        o["index"] = json!(i);
                        o["value"] = json!(d.variable_value(i, Some(0)));
                        o
                    })
                    .collect(),
            ))
        }),
        cmd!(
            "variable.define",
            "Define Text Variable…",
            ["Type", "Text Variables"],
            None,
            "{name, type: custom|lastPageNumber|chapterNumber|fileName|creationDate|modificationDate|outputDate|runningHeader, text?, format?, style?, use?: firstOnPage|lastOnPage, character?, section?, extension?, before?, after?} — creates or replaces by name",
            has_doc,
            define
        ),
        cmd!(
            "variable.delete",
            "Delete Text Variable",
            [],
            None,
            "{name} — instances become plain text of their current value",
            has_doc,
            |s, p| {
                let name = str_param(p, "name").ok_or_else(|| bad("variable.delete", "missing name"))?.to_string();
                s.edit(|d, _| {
                    let i = d.variable(&name).ok_or_else(|| bad("variable.delete", format!("no variable `{name}`")))?;
                    // Replace instances with their value (first page context), then shift higher indices down.
                    let value = d.variable_value(i, Some(0)).unwrap_or_default();
                    let n = d.text_variables.len();
                    let ids: Vec<_> = d.stories.keys().copied().collect();
                    // Instances in table cells and notes too (C8-layout-read-3: they kept the old
                    // index, which then named the next variable).
                    let target = var_char(i).unwrap_or(' ').to_string();
                    let shifts: Vec<(String, String)> =
                        (i + 1..n).map(|k| (var_char(k).unwrap_or(' ').to_string(), var_char(k - 1).unwrap_or(' ').to_string())).collect();
                    for sid in ids {
                        let Some(st) = d.story_mut(sid) else { continue };
                        st.for_each_text_mut(&mut |st| {
                            if !st.text.chars().any(|c| kition_layout_doc::vars::var_index(c).is_some_and(|k| k >= i)) {
                                return;
                            }
                            while let Some(pos) = st.text.find(&target) {
                                st.replace(pos..pos + target.len(), &value);
                            }
                            for (from, to) in &shifts {
                                while let Some(pos) = st.text.find(from.as_str()) {
                                    st.replace(pos..pos + from.len(), to);
                                }
                            }
                        });
                    }
                    d.text_variables.remove(i);
                    Ok(Value::Null)
                })
            }
        ),
        cmd!(
            "variable.insert",
            "Insert Variable",
            ["Type", "Text Variables"],
            None,
            "{name} — at the text insertion point",
            super::has_text,
            |s, p| {
                let name = str_param(p, "name").ok_or_else(|| bad("variable.insert", "missing name"))?;
                let i = s.doc()?.doc.variable(name).ok_or_else(|| bad("variable.insert", format!("no variable `{name}`")))?;
                let c = var_char(i).ok_or_else(|| bad("variable.insert", "too many variables"))?;
                s.execute("text.insert", &json!({"text": c.to_string()}))
            }
        ),
    ]
}

fn define(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("variable.define", "missing name"))?.to_string();
    let fmt = || str_param(p, "format").unwrap_or("MM/dd/yy").to_string();
    let kind = match str_param(p, "type").unwrap_or("custom") {
        "custom" => VarKind::Custom { text: str_param(p, "text").unwrap_or("").into() },
        "lastPageNumber" => VarKind::LastPageNumber { section: p.get("section").and_then(Value::as_bool).unwrap_or(false) },
        "chapterNumber" => VarKind::ChapterNumber,
        "fileName" => VarKind::FileName { extension: p.get("extension").and_then(Value::as_bool).unwrap_or(false) },
        "creationDate" => VarKind::CreationDate { format: fmt() },
        "modificationDate" => VarKind::ModificationDate { format: fmt() },
        "outputDate" => VarKind::OutputDate { format: fmt() },
        "runningHeader" => VarKind::RunningHeader {
            style: str_param(p, "style").ok_or_else(|| bad("variable.define", "runningHeader needs `style`"))?.into(),
            use_: if str_param(p, "use") == Some("lastOnPage") { kition_layout_doc::vars::Use::LastOnPage } else { Default::default() },
            character: p.get("character").and_then(Value::as_bool).unwrap_or(false),
        },
        t => return Err(bad("variable.define", format!("unknown type `{t}`"))),
    };
    let mut v = TextVariable::new(&name, kind);
    v.before = str_param(p, "before").unwrap_or("").into();
    v.after = str_param(p, "after").unwrap_or("").into();
    s.edit(|d, _| {
        let i = match d.variable(&name) {
            Some(i) => {
                d.text_variables[i] = v.clone();
                i
            }
            None => {
                if d.text_variables.len() >= kition_layout_doc::vars::VAR_MAX {
                    return Err(bad("variable.define", "too many variables"));
                }
                d.text_variables.push(v.clone());
                d.text_variables.len() - 1
            }
        };
        Ok(json!({"index": i}))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_header_on_parent_page_follows_headings() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"pages": 3, "facingPages": false})).unwrap();
        s.execute("style.paragraph.add", &json!({"name": "Heading"})).ok();
        // Headings on pages 1 and 3 (page 2 carries page 1's).
        for (page, text) in [(0, "Alpha"), (2, "Epsilon")] {
            let sp = s.doc().unwrap().doc.page_loc(page).unwrap().0;
            let r = s.execute("frame.add", &json!({"spread": sp, "rect": [72, 200, 400, 260], "content": "text", "text": text})).unwrap();
            let sid = s.doc().unwrap().doc.item(kition_layout_doc::ItemId(r["id"].as_u64().unwrap())).unwrap().text_frame().unwrap().story;
            s.execute("text.select", &json!({"story": sid.0, "anchor": 0, "focus": 1})).unwrap();
            s.execute("style.paragraph.apply", &json!({"name": "Heading"})).unwrap();
        }
        s.execute("variable.define", &json!({"name": "Chapter Head", "type": "runningHeader", "style": "Heading"})).unwrap();
        s.execute("variable.define", &json!({"name": "Total", "type": "custom", "text": "of "})).unwrap();
        // A header frame on the parent with "<running header> · <page> / <last page>".
        let r = s
            .execute("frame.add", &json!({"spread": {"kind": "parent", "index": 0}, "rect": [72, 30, 500, 60], "content": "text", "text": ""}))
            .unwrap();
        let fid = kition_layout_doc::ItemId(r["id"].as_u64().unwrap());
        let sid = s.doc().unwrap().doc.item(fid).unwrap().text_frame().unwrap().story;
        s.execute("text.select", &json!({"story": sid.0, "anchor": 0, "focus": 0})).unwrap();
        s.execute("variable.insert", &json!({"name": "Chapter Head"})).unwrap();
        s.execute("text.insert", &json!({"text": " · "})).unwrap();
        s.execute("variable.insert", &json!({"name": "Last Page Number"})).unwrap();
        let d = s.doc().unwrap().doc.clone();
        assert!(matches!(d.find(fid).unwrap().spread, kition_layout_doc::SpreadRef::Parent(0)));
        // Visible glyphs of the header composed for each page: "Alpha · 3" vs "Epsilon · 3".
        let shown = |page: usize| -> usize {
            let cs = s.cache.get(&d, sid, Some(&d.page_name(page)));
            cs.frames.iter().flat_map(|f| f.lines.iter()).map(|l| l.glyphs.iter().filter(|g| g.visible && g.adv > 0.0).count()).sum()
        };
        let (p1, p2, p3) = (shown(0), shown(1), shown(2));
        assert_eq!(p1, p2, "page 2 carries page 1's header");
        assert_eq!(p3, p1 + 2, "Epsilon is two letters longer than Alpha");
        let idx = s.cache.running_index(&d);
        assert_eq!(idx.value("Heading", false, Default::default(), 0), "Alpha");
        assert_eq!(idx.value("Heading", false, Default::default(), 1), "Alpha");
        assert_eq!(idx.value("Heading", false, Default::default(), 2), "Epsilon");
        let list = s.execute("variable.list", &json!({})).unwrap();
        assert!(list.as_array().unwrap().iter().any(|v| v["name"] == "Last Page Number" && v["value"] == "3"));
        // Deleting a variable flattens its instances to text.
        s.execute("variable.delete", &json!({"name": "Last Page Number"})).unwrap();
        assert!(s.doc().unwrap().doc.story(sid).unwrap().text.ends_with(" · 3"));
    }
}
