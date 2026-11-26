//! Paragraph / character styles and swatches.

use kition_layout_color::{Swatch, SwatchValue};
use kition_layout_doc::{CharAttrs, CharacterStyle, ParaAttrs, ParagraphStyle, Styles};
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc, has_text_or_frames, ok, str_param};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "style.compositeFont.list", "List Composite Fonts", [], None, "{} → composite font definitions", has_doc, |s, _| {
            serde_json::to_value(&s.doc()?.doc.styles.composite_fonts).map_err(|e| bad("style.compositeFont.list", e.to_string()))
        }),
        cmd!(
            "style.compositeFont.set",
            "Set Composite Font",
            [],
            None,
            "{name, entries: [{name?, characters?, family, style?, relativeSize?, horizontalScale?, verticalScale?, baselineShift?, scaleOption?}]} — replaces a composite font definition; empty characters selects the base entry",
            has_doc,
            |s, p| {
                let f: kition_layout_doc::cjk::CompositeFont =
                    serde_json::from_value(p.clone()).map_err(|e| bad("style.compositeFont.set", e.to_string()))?;
                if f.name.trim().is_empty()
                    || f.entries.is_empty()
                    || f.entries.len() > 1024
                    || !f.entries.iter().any(|e| e.characters.is_empty())
                    || f.entries.iter().any(|e| {
                        e.family.trim().is_empty()
                            || ![e.relative_size, e.horizontal_scale, e.vertical_scale].iter().all(|v| v.is_finite() && *v > 0.0 && *v <= 100.0)
                            || !e.baseline_shift.is_finite()
                            || e.baseline_shift.abs() > 100.0
                    })
                {
                    return Err(bad("style.compositeFont.set", "invalid name, base entry, font, scale or baseline shift"));
                }
                s.edit(|d, _| {
                    let fonts = &mut d.styles_mut().composite_fonts;
                    if let Some(old) = fonts.iter_mut().find(|old| old.name == f.name) {
                        *old = f.clone();
                    } else {
                        fonts.push(f.clone());
                    }
                    Ok(json!({"name": f.name}))
                })
            }
        ),
        cmd!(
            "style.exportTag",
            "Export Tagging",
            [],
            None,
            "{style, character?: bool, tag?: p|h1…h6|blockquote|pre|li|… (span|em|strong|code|sup|sub… for characters; \"\" = automatic), class?} → the tagging",
            has_doc,
            |s, p| {
                let name = str_param(p, "style").ok_or_else(|| bad("style.exportTag", "`style` required"))?.to_string();
                let character = p.get("character").and_then(Value::as_bool).unwrap_or(false);
                let exists = if character { s.doc()?.doc.styles.char_style(&name).is_some() } else { s.doc()?.doc.styles.para(&name).is_some() };
                if !exists {
                    return Err(bad("style.exportTag", format!("no style `{name}`")));
                }
                let key = format!("{}:{name}", if character { "c" } else { "p" });
                let (tag, class) = (str_param(p, "tag").map(str::to_string), str_param(p, "class").map(str::to_string));
                s.edit(|d, _| {
                    let st = d.styles_mut();
                    let e = st.export_tags.entry(key.clone()).or_default();
                    if let Some(t) = tag.clone() {
                        e.tag = t;
                    }
                    if let Some(c) = class.clone() {
                        e.class = c;
                    }
                    let out = serde_json::to_value(&*e).unwrap_or_default();
                    if e.tag.is_empty() && e.class.is_empty() {
                        st.export_tags.remove(&key);
                    }
                    Ok(out)
                })
            }
        ),
        cmd!("style.paragraph.apply", "Apply Paragraph Style", [], None, "{name, clearOverrides?: bool}", has_text_or_frames, apply_para),
        cmd!("style.character.apply", "Apply Character Style", [], None, "{name}", has_text_or_frames, apply_char),
        cmd!(
            "style.paragraph.add",
            "New Paragraph Style…",
            [],
            None,
            "{name, basedOn?, nextStyle?, para?: {…}, chars?: {…}, fromSelection?: bool}",
            has_doc,
            create_para
        ),
        cmd!(
            "style.paragraph.edit",
            "Paragraph Style Options…",
            [],
            None,
            "{name, rename?, basedOn?, nextStyle?, para?: {… ruleAbove?/ruleBelow?: only the named rule fields change}, chars?: {…}, fromSelection?: bool (Redefine Style: the style takes the selected text's look; `para` and `chars` go over it)}",
            has_doc,
            edit_para
        ),
        cmd!("style.paragraph.delete", "Delete Paragraph Style", [], None, "{name, replaceWith?}", has_doc, delete_para),
        cmd!(
            "style.character.delete",
            "Delete Character Style",
            [],
            None,
            "{name, replaceWith?: style (default \"[None]\")} — text, styles based on it, and nested or GREP styles that used it take `replaceWith`",
            has_doc,
            delete_char
        ),
        cmd!(
            "style.character.add",
            "New Character Style…",
            [],
            None,
            "{name, basedOn?: style | \"[None]\", chars?: {…}} — the style sets only `chars` → {name} (made unique)",
            has_doc,
            create_char
        ),
        cmd!(
            "style.character.edit",
            "Character Style Options…",
            [],
            None,
            "{name, rename?, basedOn?: style | \"[None]\" | null, chars?: {attr: value | null (no longer set)}, fromSelection?: bool (Redefine Style: the style takes the selected text's look where it differs)} — renaming updates every use",
            has_doc,
            edit_char
        ),
        cmd!(query "style.list", "List Styles", [], None, "{} → paragraph and character style names", has_doc, |s, _| {
            let st = s.doc()?;
            Ok(json!({
                "paragraph": st.doc.styles.paragraph.iter().map(|p| &p.name).collect::<Vec<_>>(),
                "character": st.doc.styles.character.iter().map(|p| &p.name).collect::<Vec<_>>(),
                "object": st.doc.styles.object.iter().map(|p| &p.name).collect::<Vec<_>>(),
                "cell": st.doc.styles.cell.iter().map(|p| &p.name).collect::<Vec<_>>(),
                "table": st.doc.styles.table.iter().map(|p| &p.name).collect::<Vec<_>>(),
            }))
        }),
        cmd!("style.object.apply", "Apply Object Style", [], None, "{name, ids?}", super::has_selection, apply_object),
        cmd!("style.object.add", "New Object Style…", [], None, "{name, fromSelection?: true, fill?, paragraphStyle?}", has_doc, create_object),
        cmd!(
            "style.object.edit",
            "Object Style Options…",
            [],
            None,
            "{name, rename?, basedOn?: style | null, fill?: swatch | null (left out of the style), fillTint?: 0..1, stroke?: swatch | null, strokeWeight?, strokeTint?: 0..1, paragraphStyle?: style | null, textFrame?: {columns?, gutter?, inset?: [top, left, bottom, right], verticalJustification?: top|center|bottom|justify} | null, fromSelection?: bool (Redefine Style: the style takes the first selected object's fill, stroke and text frame options)} — objects using the style (or one based on it) follow, except where they override it; renaming updates every use",
            has_doc,
            edit_object
        ),
        cmd!(
            "style.object.delete",
            "Delete Object Style",
            [],
            None,
            "{name, replaceWith?: style (default \"[None]\")} — objects using it, styles based on it and the default frame styles take `replaceWith`; the objects keep their look",
            has_doc,
            delete_object
        ),
        cmd!(
            "swatch.add",
            "New Color Swatch…",
            [],
            None,
            "{name?, color: \"#rrggbb\"|{c,m,y,k}(0..100)|[r,g,b], spot?: bool, gradient?: {kind?: linear|radial, stops: [{location: 0–100, color: \"#rrggbb\"|{c,m,y,k}|colour swatch name, midpoint?: 13–87}]} (a gradient swatch instead of a colour)}",
            has_doc,
            create_swatch
        ),
        cmd!(
            "object.gradient",
            "Gradient",
            [],
            None,
            "{kind?: linear|radial, stops?: [{location: 0–100, color: \"#rrggbb\"|{c,m,y,k}|[r,g,b], opacity?: 0–100, midpoint?: 13–87}], reverse?: bool, angle?: degrees, from?: [x,y], to?: [x,y] (spread coords: the Gradient Swatch tool's drag), ids?} → {swatch, kind, stops} — edits the fill's gradient (an unnamed gradient unless it equals a gradient swatch)",
            super::has_selection,
            apply_gradient
        ),
        cmd!(
            "style.group",
            "Move to Group",
            [],
            None,
            "{kind: paragraph|character, names: [style names], group: \"Heads\" | \"Heads/Display\" | \"\" (out of any group)} — styles are named Group/Name; every use is renamed → {renamed: {old: new}}",
            has_doc,
            group_styles
        ),
        cmd!(
            "swatch.load",
            "Load Swatches…",
            [],
            None,
            "{path?|base64?, replace?: bool} — colour swatches from a swatch exchange (.ase) file; names already in the document are kept unless `replace` → {added, replaced}",
            has_doc,
            load_swatches
        ),
        cmd!(
            noundo "swatch.save",
            "Save Swatches for Exchange…",
            [],
            None,
            "{path?, names?: [swatch names] (default: all colour swatches)} → {base64} or writes `path` (.ase)",
            has_doc,
            save_swatches
        ),
        cmd!(
            "object.color",
            "Apply Color",
            [],
            None,
            "{color: \"#rrggbb\"|{c,m,y,k}(0..100)|[r,g,b](0..255), target?: fill|stroke, ids?} — an unnamed colour (not in the Swatches panel until Add to Swatches) on the selection",
            super::has_selection,
            apply_color
        ),
        cmd!(
            "swatch.addToSwatches",
            "Add to Swatches",
            [],
            None,
            "{swatch?: name (default: the selection's fill), target?: fill|stroke, name?: new name} — makes an unnamed colour a swatch",
            has_doc,
            add_to_swatches
        ),
        cmd!("swatch.addUnnamed", "Add Unnamed Colors", [], None, "{} — every unnamed colour used becomes a swatch", has_doc, |s, _| {
            s.edit(|d, _| {
                let mut n = 0;
                for w in &mut d.swatches {
                    if w.hidden {
                        w.hidden = false;
                        n += 1;
                    }
                }
                Ok(json!({"added": n}))
            })
        }),
        cmd!(
            "swatch.delete",
            "Delete Swatch",
            [],
            None,
            "{name, replaceWith?: swatch} — with `replaceWith`, every use of the swatch (objects, text, styles, tints) takes that swatch",
            has_doc,
            delete_swatch
        ),
        cmd!(
            "swatch.set",
            "Swatch Options…",
            [],
            None,
            "{name, rename?, color?: \"#rrggbb\"|{c,m,y,k}(0..100)|[r,g,b], spot?: bool, tint?: 0..1 (a tint swatch), gradient?: {kind?, stops: [{location, color, midpoint?}]} (a gradient swatch)} — changes a swatch; everything using it follows (renaming updates every use)",
            has_doc,
            set_swatch
        ),
        cmd!(query "ink.list", "Ink Manager", [], None, "{} → {allToProcess, inks: [{name, process: bool (printed as process), alias?}]} — the spot inks", has_doc, |s, _| {
            let d = &s.doc()?.doc;
            let inks: Vec<Value> = d
                .swatches
                .iter()
                .filter(|w| matches!(w.value, kition_layout_color::swatch::SwatchValue::Color { color_type: kition_layout_color::swatch::ColorType::Spot, .. }))
                .map(|w| {
                    let alias = d.inks.aliases.iter().find(|(a, _)| *a == w.name).map(|(_, t)| t.clone());
                    json!({"name": w.name, "process": d.inks.resolve(&w.name).1, "alias": alias})
                })
                .collect();
            Ok(json!({"allToProcess": d.inks.all_to_process, "inks": inks}))
        }),
        cmd!(
            "ink.options",
            "Ink Manager",
            [],
            None,
            "{allToProcess?: bool, ink?: spot swatch, toProcess?: bool, alias?: spot swatch | null} — output: spots to process, ink aliases",
            has_doc,
            |s, p| {
                let all = p.get("allToProcess").and_then(Value::as_bool);
                let ink = str_param(p, "ink").map(str::to_string);
                let to_process = p.get("toProcess").and_then(Value::as_bool);
                let alias = p.get("alias").cloned();
                s.edit(|d, _| {
                    if let Some(v) = all {
                        d.inks.all_to_process = v;
                    }
                    if let Some(name) = &ink {
                        let is_spot = |n: &str| {
                            d.swatch(n).is_some_and(|w| {
                                matches!(
                                    w.value,
                                    kition_layout_color::swatch::SwatchValue::Color { color_type: kition_layout_color::swatch::ColorType::Spot, .. }
                                )
                            })
                        };
                        if !is_spot(name) {
                            return Err(bad("ink.options", format!("`{name}` isn't a spot ink")));
                        }
                        if let Some(Value::String(to)) = &alias
                            && (!is_spot(to) || to == name)
                        {
                            return Err(bad("ink.options", format!("can't alias to `{to}`")));
                        }
                        if let Some(v) = to_process {
                            d.inks.to_process.retain(|x| x != name);
                            if v {
                                d.inks.to_process.push(name.clone());
                            }
                        }
                        match alias {
                            Some(Value::String(to)) => {
                                d.inks.aliases.retain(|(a, _)| a != name);
                                d.inks.aliases.push((name.clone(), to));
                            }
                            Some(Value::Null) => d.inks.aliases.retain(|(a, _)| a != name),
                            _ => {}
                        }
                    }
                    ok()
                })
            }
        ),
        cmd!(
            "swatch.newColorGroup",
            "New Color Group",
            [],
            None,
            "{name?, swatches?: [names]} — a Swatches panel folder (the swatches move into it) → {name}",
            has_doc,
            |s, p| {
                let names: Vec<String> = p
                    .get("swatches")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let want = str_param(p, "name").map(str::to_string);
                s.edit(|d, _| {
                    for n in &names {
                        if d.swatch(n).is_none_or(|w| w.locked) {
                            return Err(bad("swatch.newColorGroup", format!("`{n}` can't go in a group")));
                        }
                    }
                    let taken = |n: &str| d.color_groups.iter().any(|g| g.name == n);
                    let name = match want {
                        Some(n) if taken(&n) => return Err(bad("swatch.newColorGroup", format!("a group named `{n}` exists"))),
                        Some(n) => n,
                        None => (1..=d.color_groups.len() + 1)
                            .map(|i| format!("Color Group {i}"))
                            .find(|n| !taken(n))
                            .ok_or_else(|| bad("swatch.newColorGroup", "no free group name"))?,
                    };
                    for g in &mut d.color_groups {
                        g.swatches.retain(|w| !names.contains(w));
                    }
                    d.color_groups.push(kition_layout_doc::ColorGroup { name: name.clone(), swatches: names });
                    Ok(json!({"name": name}))
                })
            }
        ),
        cmd!("swatch.moveToGroup", "Move to Color Group", [], None, "{swatches: [names], group: name | null (top level)}", has_doc, |s, p| {
            let names: Vec<String> = p
                .get("swatches")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let group = str_param(p, "group").map(str::to_string);
            s.edit(|d, _| {
                if let Some(g) = &group
                    && !d.color_groups.iter().any(|x| x.name == *g)
                {
                    return Err(bad("swatch.moveToGroup", format!("no color group `{g}`")));
                }
                for g in &mut d.color_groups {
                    g.swatches.retain(|w| !names.contains(w));
                }
                if let Some(g) = group.and_then(|g| d.color_groups.iter_mut().find(|x| x.name == g)) {
                    g.swatches.extend(names.iter().filter(|n| d.swatches.iter().any(|w| w.name == **n && !w.locked)).cloned());
                }
                ok()
            })
        }),
        cmd!("swatch.ungroupColorGroup", "Ungroup Color Group", [], None, "{name} — its swatches go back to the top level", has_doc, |s, p| {
            let name = str_param(p, "name").unwrap_or("").to_string();
            s.edit(|d, _| {
                let n = d.color_groups.len();
                d.color_groups.retain(|g| g.name != name);
                if d.color_groups.len() == n {
                    return Err(bad("swatch.ungroupColorGroup", format!("no color group `{name}`")));
                }
                ok()
            })
        }),
        cmd!("swatch.renameColorGroup", "Color Group Options", [], None, "{name, to}", has_doc, |s, p| {
            let (name, to) = (str_param(p, "name").unwrap_or("").to_string(), str_param(p, "to").unwrap_or("").trim().to_string());
            s.edit(|d, _| {
                if to.is_empty() || d.color_groups.iter().any(|g| g.name == to) {
                    return Err(bad("swatch.renameColorGroup", format!("can't rename to `{to}`")));
                }
                let g = d
                    .color_groups
                    .iter_mut()
                    .find(|g| g.name == name)
                    .ok_or_else(|| bad("swatch.renameColorGroup", format!("no color group `{name}`")))?;
                g.name = to;
                ok()
            })
        }),
    ]
}

fn attrs<T: Default>(v: Option<&Value>, set: impl Fn(&mut T, &str, &Value) -> std::result::Result<(), String>) -> Result<T> {
    let mut a = T::default();
    if let Some(o) = v.and_then(Value::as_object) {
        for (k, v) in o {
            set(&mut a, k, v).map_err(|e| bad("style", e))?;
        }
    }
    Ok(a)
}

fn apply_para(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("style.paragraph.apply", "missing name"))?.to_string();
    let clear = p.get("clearOverrides").and_then(Value::as_bool).unwrap_or(false);
    if s.doc()?.doc.styles.para(&name).is_none() {
        return Err(bad("style.paragraph.apply", format!("no paragraph style `{name}`")));
    }
    let targets = super::text::format_targets_pub(s);
    s.edit(|d, _| {
        for t in &targets {
            let r = &t.range;
            if let Some(st) = d.text_story_mut(t.story, t.cell) {
                st.format_paras(r.clone(), |f| {
                    f.style = name.clone();
                    if clear {
                        f.para = ParaAttrs::default();
                        f.chars = CharAttrs::default();
                    }
                });
                if clear {
                    let (a, b) = (st.para_ranges()[st.para_at(r.start)].start, st.para_ranges()[st.para_at(r.end)].end);
                    st.format_chars(a..b, |f| f.over = CharAttrs::default());
                }
            }
        }
        ok()
    })
}

fn apply_char(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("style.character.apply", "missing name"))?.to_string();
    if s.doc()?.doc.styles.char_style(&name).is_none() {
        return Err(bad("style.character.apply", format!("no character style `{name}`")));
    }
    let targets = super::text::format_targets_pub(s);
    s.edit(|d, _| {
        for t in &targets {
            if let Some(st) = d.text_story_mut(t.story, t.cell) {
                st.format_chars(t.range.clone(), |f| f.style = name.clone());
            }
        }
        ok()
    })
}

fn create_para(s: &mut Session, p: &Value) -> Result<Value> {
    let base = str_param(p, "name").unwrap_or("Paragraph Style 1").to_string();
    let mut para: ParaAttrs = attrs(p.get("para"), |a: &mut ParaAttrs, k, v| a.set_json(k, v))?;
    let mut chars: CharAttrs = attrs(p.get("chars"), |a: &mut CharAttrs, k, v| a.set_json(k, v))?;
    if p.get("fromSelection").and_then(Value::as_bool).unwrap_or(false)
        && let Ok(cur) = s.execute("text.selectionAttrs", &json!({}))
        && !cur.is_null()
    {
        if let Ok(pp) = serde_json::from_value::<kition_layout_doc::ParaProps>(cur["para"].clone()) {
            let mut full = ParaProps_to_attrs(&pp);
            full.merge(&para);
            para = full;
        }
        if let Ok(cp) = serde_json::from_value::<kition_layout_doc::CharProps>(cur["chars"].clone()) {
            let mut full = CharProps_to_attrs(&cp);
            full.merge(&chars);
            chars = full;
        }
    }
    let based_on = str_param(p, "basedOn").map(str::to_string);
    let next = str_param(p, "nextStyle").map(str::to_string);
    s.edit(|d, _| {
        let name = Styles::unique_name(|n| d.styles.para(n).is_some(), &base);
        if let Some(b) = &based_on
            && d.styles.para(b).is_none()
        {
            return Err(bad("style.paragraph.add", format!("no style `{b}`")));
        }
        d.styles_mut().paragraph.push(ParagraphStyle { name: name.clone(), based_on, next_style: next, para, chars, shortcut: String::new() });
        Ok(json!({"name": name}))
    })
}

#[allow(non_snake_case)]
fn ParaProps_to_attrs(p: &kition_layout_doc::ParaProps) -> ParaAttrs {
    kition_layout_doc::ParaProps::common([p])
}
#[allow(non_snake_case)]
fn CharProps_to_attrs(p: &kition_layout_doc::CharProps) -> CharAttrs {
    kition_layout_doc::CharProps::common([p])
}

fn edit_para(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("style.paragraph.edit", "missing name"))?.to_string();
    let chars: CharAttrs = attrs(p.get("chars"), |a: &mut CharAttrs, k, v| a.set_json(k, v))?;
    let rename = str_param(p, "rename").map(str::to_string);
    let based = p.get("basedOn").cloned();
    let next = p.get("nextStyle").cloned();
    let picked =
        if p.get("fromSelection").and_then(Value::as_bool).unwrap_or(false) { Some(selection_look(s, "style.paragraph.edit")?) } else { None };
    s.edit(|d, _| {
        if let Some(to) = rename.as_deref()
            && to != name
            && d.styles.para(to).is_some()
        {
            return Err(bad("style.paragraph.edit", format!("a paragraph style `{to}` already exists")));
        }
        if let Some(Value::String(b)) = &based
            && d.styles.para_based_on_cycles(&name, b)
        {
            return Err(bad("style.paragraph.edit", "based-on would create a cycle"));
        }
        // A rule object changes only the rule fields it names, over the style's resolved rule.
        let (current, current_chars) = d.styles.resolve_para_style(&name);
        let para: ParaAttrs = attrs(p.get("para"), |a: &mut ParaAttrs, k, v| a.set_json_over(k, v, &current))?;
        let st = d.styles_mut().para_mut(&name).ok_or_else(|| bad("style.paragraph.edit", format!("no style `{name}`")))?;
        // Redefine Style: what the selected text has where the style differs.
        if let Some((look_para, look_chars)) = &picked {
            st.para.merge(&current.delta(look_para));
            st.chars.merge(&current_chars.delta(look_chars));
        }
        st.para.merge(&para);
        st.chars.merge(&chars);
        match based {
            Some(Value::String(b)) => st.based_on = Some(b),
            Some(Value::Null) => st.based_on = None,
            _ => {}
        }
        match next {
            Some(Value::String(n)) => st.next_style = Some(n),
            Some(Value::Null) => st.next_style = None,
            _ => {}
        }
        if picked.is_some() {
            // The overrides that now say what the style says are no overrides any more.
            drop_redundant_overrides(d, &name);
        }
        if let Some(n) = rename.clone() {
            // Every use: other styles, stories, table cells, footnotes, object styles.
            rename_style(d, true, &name, &n);
        }
        ok()
    })
}

/// Character style `name` redefined from `look` (the selected text in a paragraph of style
/// `paragraph`): every attribute the style sets takes the text's value, and so does every one
/// where the text differs from its paragraph's look.
fn redefined_chars(d: &kition_layout_doc::Document, name: &str, look: &kition_layout_doc::CharProps, paragraph: &str) -> Result<CharAttrs> {
    const ID: &str = "style.character.edit";
    let mut out = d.styles.resolve_para_style(paragraph).1.delta(look);
    let values = serde_json::to_value(look).map_err(|e| bad(ID, e.to_string()))?;
    let set = d.styles.char_style_attrs(name);
    for key in set.keys() {
        let camel = snake_to_camel(key);
        if let Some(value) = values.get(&camel) {
            out.set_json(&camel, value).map_err(|e| bad(ID, e))?;
        }
    }
    Ok(out)
}

fn snake_to_camel(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut upper = false;
    for c in snake.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The resolved paragraph and character attributes at the text selection (Redefine Style's source).
fn selection_look(s: &mut Session, id: &str) -> Result<(kition_layout_doc::ParaProps, kition_layout_doc::CharProps)> {
    let at = s.execute("text.selectionAttrs", &json!({}))?;
    if at.is_null() {
        return Err(bad(id, "select text to redefine the style from"));
    }
    let para = serde_json::from_value(at["para"].clone()).map_err(|e| bad(id, e.to_string()))?;
    let chars = serde_json::from_value(at["chars"].clone()).map_err(|e| bad(id, e.to_string()))?;
    Ok((para, chars))
}

/// In every paragraph of style `name`, drops the paragraph overrides equal to the style's values,
/// and the character overrides of runs without a character style that equal them (each run wholly
/// inside such paragraphs).
fn drop_redundant_overrides(d: &mut kition_layout_doc::Document, name: &str) {
    let (para, chars) = d.styles.resolve_para_style(name);
    for sid in d.stories.keys().copied().collect::<Vec<_>>() {
        let Some(story) = d.story_mut(sid) else { continue };
        story.for_each_text_mut(&mut |st| {
            let ranges = st.para_ranges();
            let styled: Vec<bool> = st.paras.iter().map(|f| f.style == name).collect();
            if !styled.iter().any(|on| *on) {
                return;
            }
            for f in st.paras.iter_mut().filter(|f| f.style == name) {
                f.para = f.para.diff_from(&para);
            }
            let mut at = 0usize;
            for run in &mut st.chars {
                let end = at.saturating_add(run.len);
                let inside = ranges.iter().zip(&styled).filter(|(r, _)| r.start < end.max(at + 1) && r.end > at).all(|(_, on)| *on);
                if inside && run.format.style == kition_layout_doc::NO_CHAR_STYLE {
                    run.format.over = run.format.over.diff_from(&chars);
                }
                at = end;
            }
            st.rev += 1;
        });
    }
}

/// Rename a paragraph or character style everywhere it's used.
fn rename_style(d: &mut kition_layout_doc::Document, para: bool, from: &str, to: &str) {
    let st = d.styles_mut();
    if para {
        for s in &mut st.paragraph {
            if s.name == from {
                s.name = to.to_string();
            }
            for r in [&mut s.based_on, &mut s.next_style] {
                if r.as_deref() == Some(from) {
                    *r = Some(to.to_string());
                }
            }
        }
        for o in &mut st.object {
            if o.paragraph_style.as_deref() == Some(from) {
                o.paragraph_style = Some(to.to_string());
            }
        }
    } else {
        for s in &mut st.character {
            if s.name == from {
                s.name = to.to_string();
            }
            if s.based_on.as_deref() == Some(from) {
                s.based_on = Some(to.to_string());
            }
        }
        for s in &mut st.paragraph {
            rename_char_refs(&mut s.para, from, to);
        }
        if d.footnote_options.ref_char_style == from {
            d.footnote_options.ref_char_style = to.to_string();
        }
    }
    for sid in d.stories.keys().copied().collect::<Vec<_>>() {
        let Some(story) = d.story_mut(sid) else { continue };
        story.for_each_text_mut(&mut |st| {
            if para {
                for f in st.paras.iter_mut().filter(|f| f.style == from) {
                    f.style = to.to_string();
                }
            } else {
                for r in st.chars.iter_mut().filter(|r| r.format.style == from) {
                    r.format.style = to.to_string();
                }
                for f in &mut st.paras {
                    rename_char_refs(&mut f.para, from, to);
                }
            }
            st.rev += 1;
        });
    }
}

/// Point the nested, nested line and GREP styles of paragraph attributes at a renamed character style.
fn rename_char_refs(p: &mut ParaAttrs, from: &str, to: &str) {
    let names = p.nested_styles.iter_mut().flatten().map(|n| &mut n.style);
    let names = names.chain(p.nested_line_styles.iter_mut().flatten().map(|n| &mut n.style));
    for n in names.chain(p.grep_styles.iter_mut().flatten().map(|g| &mut g.style)) {
        if n == from {
            *n = to.to_string();
        }
    }
}

fn group_styles(s: &mut Session, p: &Value) -> Result<Value> {
    let para = match str_param(p, "kind").unwrap_or("paragraph") {
        "paragraph" => true,
        "character" => false,
        k => return Err(bad("style.group", format!("unknown kind `{k}`"))),
    };
    let names: Vec<String> =
        p.get("names").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let group = str_param(p, "group").unwrap_or("").trim_matches('/').to_string();
    s.edit(|d, _| {
        let mut renamed = serde_json::Map::new();
        for n in &names {
            if n.starts_with('[') {
                return Err(bad("style.group", format!("`{n}` is built in")));
            }
            let exists = if para { d.styles.para(n).is_some() } else { d.styles.character.iter().any(|c| c.name == *n) };
            if !exists {
                return Err(bad("style.group", format!("no style `{n}`")));
            }
            let base = n.rsplit('/').next().unwrap_or(n);
            let mut to = if group.is_empty() { base.to_string() } else { format!("{group}/{base}") };
            let taken = |d: &kition_layout_doc::Document, x: &str| {
                if para { d.styles.para(x).is_some() } else { d.styles.character.iter().any(|c| c.name == x) }
            };
            if to != *n {
                to = Styles::unique_name(|x| taken(d, x), &to);
                rename_style(d, para, n, &to);
                renamed.insert(n.clone(), json!(to));
            }
        }
        Ok(json!({"renamed": renamed}))
    })
}

fn delete_para(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").unwrap_or("").to_string();
    let repl = str_param(p, "replaceWith").unwrap_or(kition_layout_doc::BASIC_PARAGRAPH).to_string();
    if name.starts_with('[') {
        return Err(bad("style.paragraph.delete", "built-in styles can't be deleted"));
    }
    s.edit(|d, _| {
        d.styles_mut().paragraph.retain(|x| x.name != name);
        // Every use names the replacement: story, cell and note text, based-on and next styles,
        // object styles (C8-layout-read-3: cell and note text kept the deleted name).
        rename_style(d, true, &name, &repl);
        ok()
    })
}

fn delete_char(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "style.character.delete";
    let name = str_param(p, "name").ok_or_else(|| bad(ID, "missing name"))?.to_string();
    let none = kition_layout_doc::NO_CHAR_STYLE;
    let repl = str_param(p, "replaceWith").unwrap_or(none).to_string();
    if name.starts_with('[') {
        return Err(bad(ID, "built-in styles can't be deleted"));
    }
    if repl == name {
        return Err(bad(ID, "a style can't replace itself"));
    }
    let d = &s.doc()?.doc;
    if d.styles.char_style(&name).is_none() {
        return Err(bad(ID, format!("no character style `{name}`")));
    }
    if repl != none && d.styles.char_style(&repl).is_none() {
        return Err(bad(ID, format!("no character style `{repl}`")));
    }
    s.edit(|d, _| {
        d.styles_mut().character.retain(|x| x.name != name);
        // Every use now names the replacement; a style based on [None] is based on nothing.
        rename_style(d, false, &name, &repl);
        for st in &mut d.styles_mut().character {
            if st.based_on.as_deref() == Some(none) {
                st.based_on = None;
            }
        }
        Ok(json!({"deleted": name, "replacedWith": repl}))
    })
}

fn create_char(s: &mut Session, p: &Value) -> Result<Value> {
    let base = str_param(p, "name").unwrap_or("Character Style 1").to_string();
    let chars: CharAttrs = attrs(p.get("chars"), |a: &mut CharAttrs, k, v| a.set_json(k, v))?;
    // Based on [None] is based on nothing.
    let based_on = str_param(p, "basedOn").filter(|b| *b != kition_layout_doc::NO_CHAR_STYLE).map(str::to_string);
    s.edit(|d, _| {
        if let Some(b) = &based_on
            && d.styles.char_style(b).is_none()
        {
            return Err(bad("style.character.add", format!("no character style `{b}`")));
        }
        let name = Styles::unique_name(|n| d.styles.char_style(n).is_some(), &base);
        d.styles_mut().character.push(CharacterStyle { name: name.clone(), based_on, chars, shortcut: String::new() });
        Ok(json!({"name": name}))
    })
}

fn edit_char(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("style.character.edit", "missing name"))?.to_string();
    if name == kition_layout_doc::NO_CHAR_STYLE {
        return Err(bad("style.character.edit", "[None] can't be edited"));
    }
    let chars: CharAttrs = attrs(p.get("chars"), |a: &mut CharAttrs, k, v| a.set_json(k, v))?;
    // `null` unsets an attribute (the style no longer sets it).
    let unset: Vec<String> = p
        .get("chars")
        .and_then(Value::as_object)
        .map(|o| o.iter().filter(|(_, v)| v.is_null()).map(|(k, _)| k.clone()).collect())
        .unwrap_or_default();
    let rename = str_param(p, "rename").filter(|n| *n != name).map(|n| n.trim().to_string());
    let based: Option<Option<String>> = match p.get("basedOn") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(b)) if b == kition_layout_doc::NO_CHAR_STYLE => Some(None),
        Some(Value::String(b)) => Some(Some(b.clone())),
        Some(_) => return Err(bad("style.character.edit", "`basedOn` is a style name or null")),
    };
    let picked = if p.get("fromSelection").and_then(Value::as_bool).unwrap_or(false) {
        let at = s.execute("text.selectionAttrs", &json!({}))?;
        let paragraph = at.get("paragraphStyle").and_then(Value::as_str).unwrap_or(kition_layout_doc::BASIC_PARAGRAPH).to_string();
        Some((selection_look(s, "style.character.edit")?.1, paragraph))
    } else {
        None
    };
    s.edit(|d, _| {
        if let Some(Some(b)) = &based {
            if d.styles.char_style(b).is_none() {
                return Err(bad("style.character.edit", format!("no character style `{b}`")));
            }
            if d.styles.char_based_on_cycles(&name, b) {
                return Err(bad("style.character.edit", "based-on would create a cycle"));
            }
        }
        if let Some(n) = &rename
            && (n.is_empty() || n.starts_with('[') || d.styles.char_style(n).is_some())
        {
            return Err(bad("style.character.edit", format!("can't rename to `{n}`")));
        }
        // Redefine Style: the attributes the style sets take the selected text's values, and so
        // do those where the text differs from its paragraph's look.
        let redefined = match &picked {
            Some((look, paragraph)) => Some(redefined_chars(d, &name, look, paragraph)?),
            None => None,
        };
        let st = d.styles_mut().char_style_mut(&name).ok_or_else(|| bad("style.character.edit", format!("no character style `{name}`")))?;
        if let Some(r) = &redefined {
            st.chars.merge(r);
        }
        st.chars.merge(&chars);
        for k in &unset {
            st.chars.set_json(k, &Value::Null).map_err(|e| bad("style.character.edit", e))?;
        }
        if let Some(b) = based.clone() {
            st.based_on = b;
        }
        if let Some(n) = &rename {
            // Every use: other styles, stories, table cells, nested and GREP styles, footnotes.
            rename_style(d, false, &name, n);
        }
        ok()
    })
}

/// A gradient swatch's gradient from `{kind?: linear|radial, stops: [{location: 0–100, color:
/// "#rrggbb" | {c,m,y,k} | a colour swatch's name (its colour is copied), midpoint?: 13–87 (to the
/// next stop)}]}` (phase 11): two to 64 stops, sorted by location.
fn parse_gradient(d: &kition_layout_doc::Document, v: &Value, id: &str) -> Result<kition_layout_color::Gradient> {
    use kition_layout_color::{GradientKind, GradientStop};
    let kind = match v.get("kind").and_then(Value::as_str) {
        None | Some("linear") => GradientKind::Linear,
        Some("radial") => GradientKind::Radial,
        Some(k) => return Err(bad(id, format!("unknown gradient kind {k}"))),
    };
    let list = v.get("stops").and_then(Value::as_array).ok_or_else(|| bad(id, "a gradient needs `stops`"))?;
    if list.len() < 2 || list.len() > 64 {
        return Err(bad(id, "a gradient has 2 to 64 stops"));
    }
    let mut stops = Vec::with_capacity(list.len());
    for stop in list {
        let color = match stop.get("color") {
            Some(Value::String(name)) if !name.starts_with('#') => d
                .swatch(name)
                .filter(|w| !matches!(w.value, SwatchValue::Gradient { .. } | SwatchValue::None))
                .and_then(|_| kition_layout_color::swatch::resolve(&d.swatches, name, 1.0))
                .ok_or_else(|| bad(id, format!("no colour swatch `{name}`")))?,
            Some(c) => parse_color(c).ok_or_else(|| bad(id, "a stop needs a colour"))?,
            None => return Err(bad(id, "a stop needs a colour")),
        };
        let number = |k: &str, default: f64, min: f64, max: f64| -> Result<f32> {
            match stop.get(k) {
                None | Some(Value::Null) => Ok(default as f32 / 100.0),
                Some(x) => Ok(x
                    .as_f64()
                    .filter(|n| n.is_finite() && (min..=max).contains(n))
                    .ok_or_else(|| bad(id, format!("{k} is from {min} to {max}")))? as f32
                    / 100.0),
            }
        };
        stops.push(GradientStop {
            offset: number("location", 0.0, 0.0, 100.0)?,
            color,
            opacity: 1.0,
            midpoint: number("midpoint", 50.0, 13.0, 87.0)?,
        });
    }
    let mut g = kition_layout_color::Gradient { kind, stops };
    g.sort();
    Ok(g)
}

fn create_swatch(s: &mut Session, p: &Value) -> Result<Value> {
    // A gradient swatch (phase 11).
    if let Some(spec) = p.get("gradient").filter(|v| !v.is_null()) {
        let gradient = parse_gradient(&s.doc()?.doc, spec, "swatch.add")?;
        let base = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or("New Gradient Swatch").to_string();
        if base.starts_with('[') {
            return Err(bad("swatch.add", "a swatch name can't start with ["));
        }
        return s.edit(|d, _| {
            let name = Styles::unique_name(|n| d.swatch(n).is_some(), &base);
            d.swatches.push(Swatch { name: name.clone(), value: SwatchValue::Gradient { gradient }, locked: false, named: true, hidden: false });
            Ok(json!({"name": name}))
        });
    }
    let c = p.get("color").ok_or_else(|| bad("swatch.add", "missing color"))?;
    let color = parse_color(c).ok_or_else(|| bad("swatch.add", "bad color"))?;
    let spot = p.get("spot").and_then(Value::as_bool).unwrap_or(false);
    let name = str_param(p, "name").map(str::to_string).unwrap_or_else(|| match color {
        kition_layout_color::Color::Cmyk { c, m, y, k } => kition_layout_color::swatch::cmyk_name(c, m, y, k),
        other => {
            let [r, g, b] = other.to_rgb();
            format!("R={} G={} B={}", (r * 255.0).round(), (g * 255.0).round(), (b * 255.0).round())
        }
    });
    s.edit(|d, _| {
        let name = Styles::unique_name(|n| d.swatch(n).is_some(), &name);
        d.swatches.push(Swatch {
            name: name.clone(),
            value: SwatchValue::Color {
                color,
                color_type: if spot { kition_layout_color::ColorType::Spot } else { kition_layout_color::ColorType::Process },
            },
            locked: false,
            named: true,
            hidden: false,
        });
        Ok(json!({"name": name}))
    })
}

fn delete_swatch(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "swatch.delete";
    let name = str_param(p, "name").unwrap_or("").to_string();
    let replace = str_param(p, "replaceWith").map(str::to_string);
    s.edit(|d, _| {
        if d.swatch(&name).is_some_and(|w| w.locked) {
            return Err(bad(ID, "special swatches can't be deleted"));
        }
        if let Some(to) = &replace {
            if *to == name {
                return Err(bad(ID, "a swatch can't replace itself"));
            }
            if d.swatch(to).is_none_or(|w| w.hidden) {
                return Err(bad(ID, format!("no swatch `{to}`")));
            }
            if d.swatch(&name).is_none() {
                return Err(bad(ID, format!("no swatch `{name}`")));
            }
            replace_swatch_uses(d, &name, to);
        }
        d.swatches.retain(|w| w.name != name);
        for g in &mut d.color_groups {
            g.swatches.retain(|w| *w != name);
        }
        Ok(json!({"deleted": name, "replacedWith": replace}))
    })
}

/// Swatch Options: a swatch's name, colour (and process or spot), or a tint swatch's tint.
fn set_swatch(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "swatch.set";
    let name = str_param(p, "name").ok_or_else(|| bad(ID, "missing name"))?.to_string();
    let rename = str_param(p, "rename").map(|n| n.trim().to_string()).filter(|n| *n != name);
    let color = match p.get("color") {
        None | Some(Value::Null) => None,
        Some(c) => Some(parse_color(c).ok_or_else(|| bad(ID, "bad color"))?),
    };
    let spot = p.get("spot").and_then(Value::as_bool);
    let tint = match p.get("tint") {
        None | Some(Value::Null) => None,
        Some(t) => Some(t.as_f64().filter(|t| (0.0..=1.0).contains(t)).ok_or_else(|| bad(ID, "tint is from 0 to 1"))? as f32),
    };
    let gradient = match p.get("gradient") {
        None | Some(Value::Null) => None,
        Some(g) => Some(parse_gradient(&s.doc()?.doc, g, ID)?),
    };
    s.edit(|d, _| {
        let w = d.swatch(&name).filter(|w| !w.hidden).ok_or_else(|| bad(ID, format!("no swatch `{name}`")))?;
        if w.locked && (rename.is_some() || !matches!(w.value, SwatchValue::Color { .. } | SwatchValue::Paper { .. })) {
            return Err(bad(ID, "special swatches keep their name"));
        }
        if w.locked && (color.is_some() || spot.is_some() || tint.is_some()) && !matches!(w.value, SwatchValue::Paper { .. }) {
            return Err(bad(ID, "special swatches can't be changed"));
        }
        if let Some(n) = &rename
            && (n.is_empty() || n.starts_with('[') || d.swatch(n).is_some())
        {
            return Err(bad(ID, format!("can't rename to `{n}`")));
        }
        let auto_name = !w.named;
        let w = d.swatches.iter_mut().find(|w| w.name == name).ok_or_else(|| bad(ID, format!("no swatch `{name}`")))?;
        match &mut w.value {
            SwatchValue::Color { color: c, color_type } => {
                if let Some(next) = color {
                    *c = next;
                }
                if let Some(on) = spot {
                    *color_type = if on { kition_layout_color::ColorType::Spot } else { kition_layout_color::ColorType::Process };
                }
                if tint.is_some() {
                    return Err(bad(ID, "only a tint swatch has a tint"));
                }
            }
            SwatchValue::Paper { color: c } => {
                if let Some(next) = color {
                    *c = next;
                }
                if spot.is_some() || tint.is_some() {
                    return Err(bad(ID, "Paper is a colour only"));
                }
            }
            SwatchValue::Tint { tint: t, .. } => {
                if color.is_some() || spot.is_some() {
                    return Err(bad(ID, "a tint swatch takes its colour from its base"));
                }
                if let Some(next) = tint {
                    *t = next;
                }
            }
            // A gradient swatch's stops and kind (phase 11): everything using it follows.
            SwatchValue::Gradient { gradient: g } => {
                if color.is_some() || spot.is_some() || tint.is_some() {
                    return Err(bad(ID, "a gradient swatch takes its colours from its stops"));
                }
                if let Some(next) = &gradient {
                    *g = next.clone();
                }
            }
            _ => {
                if color.is_some() || spot.is_some() || tint.is_some() {
                    return Err(bad(ID, "this swatch has no colour to change"));
                }
            }
        }
        if gradient.is_some() && !matches!(w.value, SwatchValue::Gradient { .. }) {
            return Err(bad(ID, "only a gradient swatch has a gradient"));
        }
        // A swatch named after its values follows them (InDesign's Name with Color Value).
        let to = match (&rename, color) {
            (Some(n), _) => Some(n.clone()),
            (None, Some(c)) if auto_name => {
                let n = value_name(c);
                (n != name && d.swatch(&n).is_none()).then_some(n)
            }
            _ => None,
        };
        if let Some(to) = &to {
            if let Some(w) = d.swatches.iter_mut().find(|w| w.name == name) {
                w.name = to.clone();
                w.named = w.named || rename.is_some();
            }
            replace_swatch_uses(d, &name, to);
        }
        Ok(json!({"name": to.unwrap_or(name)}))
    })
}

/// Points every use of swatch `from` at swatch `to`: objects' fills and strokes, text and the
/// text formats of styles, object styles, table cells, tint swatches and colour groups.
fn replace_swatch_uses(d: &mut kition_layout_doc::Document, from: &str, to: &str) {
    let swap = |name: &mut String| {
        if name == from {
            *name = to.to_string();
        }
    };
    let chars = |a: &mut CharAttrs| {
        for field in [&mut a.fill, &mut a.stroke, &mut a.underline_color, &mut a.strikethrough_color].into_iter().flatten() {
            if field == from {
                *field = to.to_string();
            }
        }
    };
    for sp in d.spreads.iter_mut().chain(d.parents.iter_mut()) {
        let sp = std::sync::Arc::make_mut(sp);
        for top in &mut sp.items {
            rename_in(std::sync::Arc::make_mut(top), from, to);
        }
    }
    for sid in d.stories.keys().copied().collect::<Vec<_>>() {
        let Some(story) = d.story_mut(sid) else { continue };
        story.for_each_text_mut(&mut |st| {
            for run in &mut st.chars {
                chars(&mut run.format.over);
            }
            for f in &mut st.paras {
                chars(&mut f.chars);
            }
            for t in st.tables.values_mut() {
                for c in &mut std::sync::Arc::make_mut(t).cells {
                    if c.fill == from {
                        c.fill = to.to_string();
                    }
                }
            }
            st.rev += 1;
        });
    }
    let styles = d.styles_mut();
    for st in &mut styles.paragraph {
        chars(&mut st.chars);
    }
    for st in &mut styles.character {
        chars(&mut st.chars);
    }
    for st in &mut styles.object {
        if let Some(f) = &mut st.fill {
            swap(&mut f.swatch);
        }
        if let Some(k) = &mut st.stroke {
            swap(&mut k.swatch);
        }
    }
    for w in &mut d.swatches {
        if let SwatchValue::Tint { base, .. } = &mut w.value {
            swap(base);
        }
    }
    for g in &mut d.color_groups {
        for w in &mut g.swatches {
            swap(w);
        }
    }
}

pub fn parse_color(v: &Value) -> Option<kition_layout_color::Color> {
    use kition_layout_color::Color;
    match v {
        Value::String(s) => Color::from_hex(s),
        Value::Array(a) if a.len() >= 3 => {
            let f = |i: usize| a[i].as_f64().map(|x| if x > 1.0 { x / 255.0 } else { x } as f32);
            Some(Color::rgb(f(0)?, f(1)?, f(2)?))
        }
        Value::Object(o) => {
            // Percentages, 0 to 100 (`c: 1` is 1 %), as the Swatches panel shows them.
            let g = |k: &str| o.get(k).and_then(Value::as_f64).filter(|x| (0.0..=100.0).contains(x)).map(|x| (x / 100.0) as f32);
            Some(Color::cmyk(g("c")?, g("m")?, g("y")?, g("k")?))
        }
        _ => None,
    }
}

fn apply_object(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("style.object.apply", "missing name"))?.to_string();
    // With what it leaves out taken from the styles it is based on.
    let os = s.doc()?.doc.styles.resolved_object_style(&name).ok_or_else(|| bad("style.object.apply", format!("no object style `{name}`")))?;
    let ids = super::targets(s, p)?;
    s.edit(|d, _| {
        let mut stories = Vec::new();
        for id in &ids {
            let Some(it) = d.item_mut(*id) else { continue };
            it.object_style = name.clone();
            if let Some(f) = &os.fill {
                it.fill = f.clone();
            }
            if let Some(st) = &os.stroke {
                it.stroke = st.clone();
            }
            if let Some(tf) = it.text_frame_mut() {
                if let Some(o) = &os.text_frame {
                    tf.options = o.clone();
                }
                stories.push(tf.story);
            }
        }
        if let Some(ps) = &os.paragraph_style {
            for sid in stories {
                if let Some(st) = d.story_mut(sid) {
                    let len = st.len();
                    st.format_paras(0..len, |f| f.style = ps.clone());
                }
            }
        }
        Ok(Value::Null)
    })
}

fn create_object(s: &mut Session, p: &Value) -> Result<Value> {
    let base = str_param(p, "name").unwrap_or("Object Style 1").to_string();
    let from_sel = p.get("fromSelection").and_then(Value::as_bool).unwrap_or(true);
    let src = if from_sel { s.doc()?.selection.items.first().and_then(|i| s.doc().ok()?.doc.item(*i).cloned()) } else { None };
    let mut os = kition_layout_doc::ObjectStyle::default();
    if let Some(it) = &src {
        os.fill = Some(it.fill.clone());
        os.stroke = Some(it.stroke.clone());
        os.text_frame = it.text_frame().map(|t| t.options.clone());
    }
    if let Some(sw) = str_param(p, "fill") {
        os.fill = Some(kition_layout_doc::Fill::swatch(sw));
    }
    if let Some(ps) = str_param(p, "paragraphStyle") {
        os.paragraph_style = Some(ps.to_string());
    }
    s.edit(|d, _| {
        let name = Styles::unique_name(|n| d.styles.object_style(n).is_some(), &base);
        os.name = name.clone();
        d.styles_mut().object.push(os.clone());
        Ok(json!({"name": name}))
    })
}

/// Object Style Options: what the style includes (fill, stroke, paragraph style, text frame
/// options), its name and the style it is based on, or the first selected object's look (Redefine
/// Style). Objects using the style, or one based on it, take each changed part they do not
/// override; a text frame's story takes a new paragraph style where it used the old one.
fn edit_object(s: &mut Session, p: &Value) -> Result<Value> {
    use kition_layout_doc::{Fill, NO_OBJECT_STYLE, Stroke, TextFrameOptions};
    const ID: &str = "style.object.edit";
    let name = str_param(p, "name").ok_or_else(|| bad(ID, "missing name"))?.to_string();
    let st = s.doc()?;
    let d = &st.doc;
    let style = d.styles.object_style(&name).cloned().ok_or_else(|| bad(ID, format!("no object style `{name}`")))?;
    if name == NO_OBJECT_STYLE {
        return Err(bad(ID, "[None] can't be changed"));
    }
    let rename = str_param(p, "rename").map(|n| n.trim().to_string()).filter(|n| *n != name);
    if let Some(n) = &rename {
        if name.starts_with('[') {
            return Err(bad(ID, "built-in styles keep their name"));
        }
        if n.is_empty() || n.starts_with('[') || d.styles.object_style(n).is_some() {
            return Err(bad(ID, format!("can't rename to `{n}`")));
        }
    }
    let based = match p.get("basedOn") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(b)) if b == NO_OBJECT_STYLE => Some(None),
        Some(Value::String(b)) => {
            if d.styles.object_style(b).is_none() {
                return Err(bad(ID, format!("no object style `{b}`")));
            }
            if d.styles.object_based_on_cycles(&name, b) {
                return Err(bad(ID, "a style can't be based on itself or on a style based on it"));
            }
            Some(Some(b.clone()))
        }
        Some(_) => return Err(bad(ID, "basedOn is a style name or null")),
    };
    let swatch = |key: &str| -> Result<Option<Option<String>>> {
        match p.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(w)) if d.swatch(w).is_some_and(|w| !w.hidden) => Ok(Some(Some(w.clone()))),
            Some(other) => Err(bad(ID, format!("no swatch {other}"))),
        }
    };
    let (fill, stroke) = (swatch("fill")?, swatch("stroke")?);
    let unit = |key: &str| -> Result<Option<f32>> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => Ok(Some(v.as_f64().filter(|t| (0.0..=1.0).contains(t)).ok_or_else(|| bad(ID, format!("{key} is from 0 to 1")))? as f32)),
        }
    };
    let (fill_tint, stroke_tint) = (unit("fillTint")?, unit("strokeTint")?);
    let weight = match p.get("strokeWeight") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|w| w.is_finite() && (0.0..=1000.0).contains(w)).ok_or_else(|| bad(ID, "strokeWeight is from 0 to 1000"))?),
    };
    let para = match p.get("paragraphStyle") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(ps)) if d.styles.para(ps).is_some() => Some(Some(ps.clone())),
        Some(other) => return Err(bad(ID, format!("no paragraph style {other}"))),
    };
    let frame = match p.get("textFrame") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::Object(patch)) => {
            let mut v = serde_json::to_value(style.text_frame.clone().unwrap_or_default()).map_err(|e| bad(ID, e.to_string()))?;
            if let Some(o) = v.as_object_mut() {
                for (k, x) in patch {
                    o.insert(k.clone(), x.clone());
                }
            }
            let o: TextFrameOptions = serde_json::from_value(v).map_err(|e| bad(ID, format!("textFrame: {e}")))?;
            if !(1..=40).contains(&o.columns) || !o.gutter.is_finite() || o.gutter < 0.0 || o.inset.iter().any(|i| !i.is_finite() || *i < 0.0) {
                return Err(bad(ID, "textFrame: 1 to 40 columns, a gutter and insets of 0 or more"));
            }
            Some(Some(o))
        }
        Some(_) => return Err(bad(ID, "textFrame is an object or null")),
    };
    // Redefine Style: the first selected object's look.
    let from = if p.get("fromSelection").and_then(Value::as_bool).unwrap_or(false) {
        let id = st.selection.items.first().copied().ok_or_else(|| super::refused(ID, "needSelection", "select an object"))?;
        Some(d.item(id).cloned().ok_or_else(|| bad(ID, "the selected object is gone"))?)
    } else {
        None
    };
    s.edit(|d, _| {
        let before: Vec<(String, kition_layout_doc::ObjectStyle)> =
            d.styles.object.iter().filter_map(|o| d.styles.resolved_object_style(&o.name).map(|r| (o.name.clone(), r))).collect();
        {
            let styles = d.styles_mut();
            let o = styles.object.iter_mut().find(|o| o.name == name).ok_or_else(|| bad(ID, "no such style"))?;
            if let Some(it) = &from {
                o.fill = Some(it.fill.clone());
                o.stroke = Some(it.stroke.clone());
                if let Some(t) = it.text_frame() {
                    o.text_frame = Some(t.options.clone());
                }
            }
            if let Some(b) = &based {
                o.based_on = b.clone();
            }
            match &fill {
                Some(None) => o.fill = None,
                Some(Some(w)) => o.fill.get_or_insert_with(Fill::none).swatch = w.clone(),
                None => {}
            }
            if let Some(t) = fill_tint {
                o.fill.get_or_insert_with(Fill::none).tint = t;
            }
            match &stroke {
                Some(None) => o.stroke = None,
                Some(Some(w)) => o.stroke.get_or_insert_with(Stroke::default).swatch = w.clone(),
                None => {}
            }
            if let Some(t) = stroke_tint {
                o.stroke.get_or_insert_with(Stroke::default).tint = t;
            }
            if let Some(w) = weight {
                o.stroke.get_or_insert_with(Stroke::default).weight = w;
            }
            if let Some(ps) = &para {
                o.paragraph_style = ps.clone();
            }
            if let Some(t) = &frame {
                o.text_frame = t.clone();
            }
        }
        follow_object_styles(d, &before);
        if let Some(to) = &rename {
            rename_object_style(d, &name, to);
        }
        Ok(json!({"name": rename.clone().unwrap_or_else(|| name.clone())}))
    })
}

/// After object styles changed (`before`: each style resolved before), the objects using a style
/// whose resolved look changed take each changed part they did not override; a text frame's story
/// takes a new paragraph style where its paragraphs used the old one (or any, when it had none).
fn follow_object_styles(d: &mut kition_layout_doc::Document, before: &[(String, kition_layout_doc::ObjectStyle)]) {
    let mut changed: Vec<(String, kition_layout_doc::ObjectStyle, kition_layout_doc::ObjectStyle)> = Vec::new();
    for (name, old) in before {
        if let Some(new) = d.styles.resolved_object_style(name)
            && new != *old
        {
            changed.push((name.clone(), old.clone(), new));
        }
    }
    if changed.is_empty() {
        return;
    }
    let mut users: Vec<(kition_layout_doc::ItemId, usize)> = Vec::new();
    for sp in d.spreads.iter().chain(d.parents.iter()) {
        for top in &sp.items {
            top.walk(&mut |it| {
                if let Some(at) = changed.iter().position(|(n, _, _)| *n == it.object_style) {
                    users.push((it.id, at));
                }
            });
        }
    }
    let mut stories = Vec::new();
    for (id, at) in users {
        let Some((_, old, new)) = changed.get(at) else { continue };
        let Some(it) = d.item_mut(id) else { continue };
        if let Some(f) = &new.fill
            && old.fill.as_ref().is_none_or(|o| *o == it.fill)
        {
            it.fill = f.clone();
        }
        if let Some(k) = &new.stroke
            && old.stroke.as_ref().is_none_or(|o| *o == it.stroke)
        {
            it.stroke = k.clone();
        }
        if let Some(tf) = it.text_frame_mut() {
            if let Some(o) = &new.text_frame
                && old.text_frame.as_ref().is_none_or(|x| *x == tf.options)
            {
                tf.options = o.clone();
            }
            if new.paragraph_style != old.paragraph_style
                && let Some(ps) = &new.paragraph_style
            {
                stories.push((tf.story, old.paragraph_style.clone(), ps.clone()));
            }
        }
    }
    for (sid, old, ps) in stories {
        if let Some(st) = d.story_mut(sid) {
            let len = st.len();
            let ranges = st.para_ranges();
            let olds: Vec<bool> = st.paras.iter().map(|f| old.as_ref().is_none_or(|o| f.style == *o)).collect();
            for (range, take) in ranges.into_iter().zip(olds) {
                if take {
                    st.format_paras(range.start..range.end.min(len), |f| f.style = ps.clone());
                }
            }
        }
    }
}

/// Every use of object style `from` takes the name `to`: objects, styles based on it, the default
/// frame styles.
fn rename_object_style(d: &mut kition_layout_doc::Document, from: &str, to: &str) {
    let mut users = Vec::new();
    for sp in d.spreads.iter().chain(d.parents.iter()) {
        for top in &sp.items {
            top.walk(&mut |it| {
                if it.object_style == from {
                    users.push(it.id);
                }
            });
        }
    }
    for id in users {
        if let Some(it) = d.item_mut(id) {
            it.object_style = to.to_string();
        }
    }
    let styles = d.styles_mut();
    for o in &mut styles.object {
        if o.name == from {
            o.name = to.to_string();
        }
        if o.based_on.as_deref() == Some(from) {
            o.based_on = Some(to.to_string());
        }
    }
    for default in [&mut styles.default_text_frame, &mut styles.default_graphic_frame] {
        if default == from {
            *default = to.to_string();
        }
    }
}

/// Delete Object Style: its objects, the styles based on it and the default frame styles take
/// `replaceWith`; the objects keep their look.
fn delete_object(s: &mut Session, p: &Value) -> Result<Value> {
    use kition_layout_doc::NO_OBJECT_STYLE;
    const ID: &str = "style.object.delete";
    let name = str_param(p, "name").ok_or_else(|| bad(ID, "missing name"))?.to_string();
    let to = str_param(p, "replaceWith").unwrap_or(NO_OBJECT_STYLE).to_string();
    let d = &s.doc()?.doc;
    if d.styles.object_style(&name).is_none() {
        return Err(bad(ID, format!("no object style `{name}`")));
    }
    if name.starts_with('[') {
        return Err(bad(ID, "built-in styles can't be deleted"));
    }
    if to == name || d.styles.object_style(&to).is_none() {
        return Err(bad(ID, format!("no object style `{to}` to replace it")));
    }
    s.edit(|d, _| {
        let mut users = Vec::new();
        for sp in d.spreads.iter().chain(d.parents.iter()) {
            for top in &sp.items {
                top.walk(&mut |it| {
                    if it.object_style == name {
                        users.push(it.id);
                    }
                });
            }
        }
        let objects = users.len();
        for id in users {
            if let Some(it) = d.item_mut(id) {
                it.object_style = to.clone();
            }
        }
        let styles = d.styles_mut();
        styles.object.retain(|o| o.name != name);
        let base = if to == NO_OBJECT_STYLE { None } else { Some(to.clone()) };
        let fallback: Vec<String> = styles.object.iter().map(|o| o.name.clone()).collect();
        for o in &mut styles.object {
            if o.based_on.as_deref() == Some(name.as_str()) {
                // The replacement, unless that would base a style on itself.
                o.based_on = base.clone().filter(|b| *b != o.name && fallback.contains(b));
            }
        }
        for default in [&mut styles.default_text_frame, &mut styles.default_graphic_frame] {
            if *default == name {
                *default = to.clone();
            }
        }
        Ok(json!({"deleted": name, "replacedWith": to, "objects": objects}))
    })
}

/// The swatch name of a colour value ("C=… M=… Y=… K=…" / "R=… G=… B=…").
fn value_name(color: kition_layout_color::Color) -> String {
    match color {
        kition_layout_color::Color::Cmyk { c, m, y, k } => kition_layout_color::swatch::cmyk_name(c, m, y, k),
        other => {
            let [r, g, b] = other.to_rgb();
            format!("R={} G={} B={}", (r * 255.0).round(), (g * 255.0).round(), (b * 255.0).round())
        }
    }
}

fn load_swatches(s: &mut Session, p: &Value) -> Result<Value> {
    let bytes = match (str_param(p, "path"), str_param(p, "base64")) {
        (_, Some(b)) => super::file::base64_decode(b),
        (Some(path), None) => crate::disk::read(path).map_err(|e| bad("swatch.load", format!("{path}: {e}")))?,
        _ => return Err(bad("swatch.load", "give `path` or `base64`")),
    };
    let incoming = kition_layout_color::ase::read(&bytes).map_err(|e| bad("swatch.load", e.to_string()))?;
    let replace = p.get("replace").and_then(Value::as_bool).unwrap_or(false);
    s.edit(|d, _| {
        let (mut added, mut replaced) = (0, 0);
        for w in incoming {
            match d.swatches.iter_mut().find(|x| x.name == w.name) {
                Some(x) if x.locked => {}
                Some(x) => {
                    if replace {
                        x.value = w.value;
                        x.hidden = false;
                        replaced += 1;
                    }
                }
                None => {
                    d.swatches.push(w);
                    added += 1;
                }
            }
        }
        Ok(json!({"added": added, "replaced": replaced}))
    })
}

fn save_swatches(s: &mut Session, p: &Value) -> Result<Value> {
    let d = &s.doc()?.doc;
    let names: Option<Vec<String>> =
        p.get("names").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect());
    let chosen: Vec<Swatch> = d.swatches.iter().filter(|w| names.as_ref().is_none_or(|n| n.contains(&w.name))).cloned().collect();
    let bytes = kition_layout_color::ase::write(&chosen);
    match str_param(p, "path") {
        Some(path) => {
            crate::disk::write(path, &bytes).map_err(|e| bad("swatch.save", format!("{path}: {e}")))?;
            Ok(json!({"path": path, "bytes": bytes.len()}))
        }
        None => Ok(json!({"base64": super::file::base64_encode(&bytes)})),
    }
}

fn gradient_json(name: &str, g: &kition_layout_color::Gradient) -> Value {
    let stops: Vec<Value> = g
        .stops
        .iter()
        .map(|s| json!({"location": (s.offset as f64 * 1000.0).round() / 10.0, "color": s.color.to_hex(), "opacity": (s.opacity * 100.0).round(), "midpoint": (s.midpoint * 100.0).round()}))
        .collect();
    json!({"swatch": name, "kind": if g.kind == kition_layout_color::GradientKind::Radial { "radial" } else { "linear" }, "stops": stops})
}

fn apply_gradient(s: &mut Session, p: &Value) -> Result<Value> {
    use kition_layout_color::{GradientKind, GradientStop};
    let kind = match str_param(p, "kind") {
        Some("radial") => Some(GradientKind::Radial),
        Some("linear") => Some(GradientKind::Linear),
        Some(k) => return Err(bad("object.gradient", format!("unknown kind {k}"))),
        None => None,
    };
    let stops = match p.get("stops").and_then(Value::as_array) {
        Some(a) => {
            let mut v = Vec::new();
            for st in a {
                let color = st.get("color").and_then(parse_color).ok_or_else(|| bad("object.gradient", "a stop needs a color"))?;
                let pct = |k: &str, d: f64| st.get(k).and_then(Value::as_f64).unwrap_or(d).clamp(0.0, 100.0) as f32 / 100.0;
                v.push(GradientStop {
                    offset: pct("location", 0.0),
                    color,
                    opacity: pct("opacity", 100.0),
                    midpoint: pct("midpoint", 50.0).clamp(0.13, 0.87),
                });
            }
            if v.len() < 2 {
                return Err(bad("object.gradient", "a gradient needs at least two stops"));
            }
            Some(v)
        }
        None => None,
    };
    let reverse = p.get("reverse").and_then(Value::as_bool).unwrap_or(false);
    let angle = p.get("angle").and_then(Value::as_f64);
    let (from, to) = (super::point_param(p, "from"), super::point_param(p, "to"));
    let ids = super::targets(s, p)?;
    s.edit(|d, _| {
        let mut out = Value::Null;
        for id in &ids {
            let Some(it) = d.item(*id) else { continue };
            let cur_name = it.fill.swatch.clone();
            let cur = kition_layout_color::swatch::resolve_gradient(&d.swatches, &cur_name).cloned();
            let mut g = cur.clone().unwrap_or_default();
            if let Some(k) = kind {
                g.kind = k;
            }
            if let Some(st) = &stops {
                g.stops = st.clone();
                g.sort();
            }
            if reverse {
                g.reverse();
            }
            // Keep the swatch while the gradient is unchanged, else reuse an equal gradient swatch
            // or add an unnamed one.
            let name = if cur.as_ref() == Some(&g) {
                cur_name
            } else if let Some(w) = d.swatches.iter().find(|w| matches!(&w.value, SwatchValue::Gradient { gradient } if *gradient == g)) {
                w.name.clone()
            } else {
                let name = Styles::unique_name(|n| d.swatch(n).is_some(), "Gradient");
                d.swatches.push(Swatch {
                    name: name.clone(),
                    value: SwatchValue::Gradient { gradient: g.clone() },
                    locked: false,
                    named: false,
                    hidden: true,
                });
                name
            };
            let inv = d.find(*id).map(|l| d.parent_xf(&l)).zip(d.item(*id).map(|i| i.xf)).map(|(pxf, xf)| (pxf * xf).inverse());
            let Some(it) = d.item_mut(*id) else { continue };
            it.fill.swatch = name.clone();
            it.fill.tint = 1.0;
            if let Some(a) = angle {
                it.fill.gradient_angle = Some(a);
                it.fill.gradient_vector = None;
            }
            if let (Some(a), Some(b), Some(inv)) = (from, to, inv) {
                let (a, b) = (inv * a, inv * b);
                if (b - a).hypot() > 1e-6 {
                    it.fill.gradient_vector = Some([a.x, a.y, b.x, b.y]);
                }
            }
            if out.is_null() {
                out = gradient_json(&name, &g);
            }
        }
        Ok(out)
    })
}

fn apply_color(s: &mut Session, p: &Value) -> Result<Value> {
    let color = p.get("color").and_then(parse_color).ok_or_else(|| bad("object.color", "missing or bad color"))?;
    let stroke = str_param(p, "target") == Some("stroke");
    let ids = super::targets(s, p)?;
    s.edit(|d, _| {
        let name = color_swatch(d, color);
        for id in &ids {
            if let Some(it) = d.item_mut(*id) {
                if stroke {
                    it.stroke.swatch = name.clone();
                    it.stroke.tint = 1.0;
                } else {
                    it.fill.swatch = name.clone();
                    it.fill.tint = 1.0;
                }
            }
        }
        Ok(json!({"swatch": name}))
    })
}

/// The swatch holding exactly `color` (a process colour), or a new unnamed one (hidden from the
/// Swatches panel until Add to Swatches): its name.
pub(crate) fn color_swatch(d: &mut kition_layout_doc::Document, color: kition_layout_color::Color) -> String {
    let existing = d
        .swatches
        .iter()
        .find(|w| matches!(&w.value, SwatchValue::Color { color: c, color_type: kition_layout_color::ColorType::Process } if *c == color))
        .map(|w| w.name.clone());
    if let Some(name) = existing {
        return name;
    }
    let name = Styles::unique_name(|n| d.swatch(n).is_some(), &value_name(color));
    d.swatches.push(Swatch {
        name: name.clone(),
        value: SwatchValue::Color { color, color_type: kition_layout_color::ColorType::Process },
        locked: false,
        named: false,
        hidden: true,
    });
    name
}

fn add_to_swatches(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let name = match str_param(p, "swatch") {
        Some(n) => n.to_string(),
        None => {
            let id = st.selection.items.first().copied().ok_or_else(|| bad("swatch.addToSwatches", "select an object or give `swatch`"))?;
            let it = st.doc.item(id).ok_or_else(|| bad("swatch.addToSwatches", "no such item"))?;
            if str_param(p, "target") == Some("stroke") { it.stroke.swatch.clone() } else { it.fill.swatch.clone() }
        }
    };
    let rename = str_param(p, "name").map(str::to_string);
    s.edit(|d, _| {
        let w = d.swatches.iter_mut().find(|w| w.name == name).ok_or_else(|| bad("swatch.addToSwatches", format!("no swatch `{name}`")))?;
        if w.locked {
            return Err(bad("swatch.addToSwatches", "special swatches are already listed"));
        }
        w.hidden = false;
        let Some(new) = rename.filter(|n| *n != name) else { return Ok(json!({"name": name})) };
        if d.swatches.iter().any(|w| w.name == new) {
            return Err(bad("swatch.addToSwatches", format!("a swatch named `{new}` exists")));
        }
        let w = d.swatches.iter_mut().find(|w| w.name == name).ok_or_else(|| bad("swatch.addToSwatches", format!("no swatch `{name}`")))?;
        w.name = new.clone();
        w.named = true;
        for g in &mut d.color_groups {
            for w in &mut g.swatches {
                if *w == name {
                    *w = new.clone();
                }
            }
        }
        // Everything using the colour follows the rename.
        for sp in d.spreads.iter_mut().chain(d.parents.iter_mut()) {
            let sp = std::sync::Arc::make_mut(sp);
            for top in &mut sp.items {
                rename_in(std::sync::Arc::make_mut(top), &name, &new);
            }
        }
        Ok(json!({"name": new}))
    })
}

/// Point fills and strokes of an item (and its children) at a renamed swatch.
fn rename_in(it: &mut kition_layout_doc::Item, from: &str, to: &str) {
    if it.fill.swatch == from {
        it.fill.swatch = to.to_string();
    }
    if it.stroke.swatch == from {
        it.stroke.swatch = to.to_string();
    }
    if let Some(kids) = it.children_mut() {
        for k in kids {
            rename_in(std::sync::Arc::make_mut(k), from, to);
        }
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn style_groups_rename_every_use() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("style.paragraph.add", &json!({"name": "Head"})).unwrap();
        s.execute("style.paragraph.add", &json!({"name": "Sub", "basedOn": "Head"})).unwrap();
        let r = s.execute("frame.add", &json!({"rect": [72, 72, 300, 200], "content": "text", "text": "Hi"})).unwrap();
        let sid = r["story"].as_u64().unwrap();
        s.execute("text.select", &json!({"story": sid, "anchor": 0, "focus": 2})).unwrap();
        s.execute("style.paragraph.apply", &json!({"name": "Head"})).unwrap();
        let r = s.execute("style.group", &json!({"kind": "paragraph", "names": ["Head", "Sub"], "group": "Titles"})).unwrap();
        assert_eq!(r["renamed"]["Head"], "Titles/Head");
        let d = s.doc().unwrap().doc.clone();
        assert_eq!(d.story(kition_layout_doc::StoryId(sid)).unwrap().paras[0].style, "Titles/Head");
        assert_eq!(d.styles.para("Titles/Sub").unwrap().based_on.as_deref(), Some("Titles/Head"));
        // Out of the group again.
        s.execute("style.group", &json!({"kind": "paragraph", "names": ["Titles/Head"], "group": ""})).unwrap();
        assert!(s.doc().unwrap().doc.styles.para("Head").is_some());
        assert!(s.execute("style.group", &json!({"names": ["[Basic Paragraph]"], "group": "X"})).is_err());
    }

    #[test]
    fn deleting_a_character_style_hands_its_uses_to_the_replacement() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("style.character.add", &json!({"name": "Base", "chars": {"fontStyle": "Bold"}})).unwrap();
        s.execute("style.character.add", &json!({"name": "Emphasis", "basedOn": "Base", "chars": {"fontStyle": "Italic"}})).unwrap();
        s.execute("style.character.add", &json!({"name": "Loud", "basedOn": "Emphasis", "chars": {"size": 20}})).unwrap();
        s.execute("style.paragraph.add", &json!({"name": "Lead", "para": {"grepStyles": [{"style": "Emphasis", "pattern": "\\d+"}]}})).unwrap();
        let r = s.execute("frame.add", &json!({"rect": [72, 72, 300, 200], "content": "text", "text": "Hi there"})).unwrap();
        let sid = r["story"].as_u64().unwrap();
        s.execute("text.select", &json!({"story": sid, "anchor": 0, "focus": 2})).unwrap();
        s.execute("style.character.apply", &json!({"name": "Emphasis"})).unwrap();

        assert!(s.execute("style.character.delete", &json!({"name": "[None]"})).is_err());
        assert!(s.execute("style.character.delete", &json!({"name": "Nope"})).is_err());
        assert!(s.execute("style.character.delete", &json!({"name": "Emphasis", "replaceWith": "Nope"})).is_err());
        s.execute("style.character.delete", &json!({"name": "Emphasis"})).unwrap();
        let d = s.doc().unwrap().doc.clone();
        assert!(d.styles.char_style("Emphasis").is_none());
        assert_eq!(d.styles.char_style("Loud").unwrap().based_on, None, "based on [None] is based on nothing");
        let story = d.story(kition_layout_doc::StoryId(sid)).unwrap();
        assert!(story.chars.iter().all(|r| r.format.style != "Emphasis"));
        assert_eq!(d.styles.para("Lead").unwrap().para.grep_styles.as_ref().unwrap()[0].style, "[None]");
        // One undo step brings it back, uses and all.
        s.execute("edit.undo", &json!({})).unwrap();
        let d = s.doc().unwrap().doc.clone();
        assert!(d.styles.char_style("Emphasis").is_some());
        assert!(d.story(kition_layout_doc::StoryId(sid)).unwrap().chars.iter().any(|r| r.format.style == "Emphasis"));
        // With a replacement, uses and based-on styles take it.
        s.execute("style.character.delete", &json!({"name": "Emphasis", "replaceWith": "Base"})).unwrap();
        let d = s.doc().unwrap().doc.clone();
        assert_eq!(d.styles.char_style("Loud").unwrap().based_on.as_deref(), Some("Base"));
        assert!(d.story(kition_layout_doc::StoryId(sid)).unwrap().chars.iter().any(|r| r.format.style == "Base"));
    }

    #[test]
    fn character_style_edit_sets_unsets_rebases_and_renames_every_use() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("style.character.add", &json!({"name": "Base", "chars": {"fontStyle": "Bold"}})).unwrap();
        s.execute("style.character.add", &json!({"name": "Emphasis", "chars": {"fontStyle": "Italic", "tracking": 20}})).unwrap();
        s.execute(
            "style.paragraph.add",
            &json!({"name": "Lead", "para": {
                "nestedStyles": [{"style": "Emphasis", "through": true, "count": 1, "until": {"kind": "words"}}],
                "grepStyles": [{"style": "Emphasis", "pattern": "\\d+"}],
                "nestedLineStyles": [{"style": "Emphasis", "lines": 1}]}}),
        )
        .unwrap();
        s.execute("footnote.options", &json!({"refCharStyle": "Emphasis"})).unwrap();
        let r = s.execute("frame.add", &json!({"rect": [72, 72, 300, 200], "content": "text", "text": "Hi there"})).unwrap();
        let sid = r["story"].as_u64().unwrap();
        s.execute("text.select", &json!({"story": sid, "anchor": 0, "focus": 2})).unwrap();
        s.execute("style.character.apply", &json!({"name": "Emphasis"})).unwrap();

        s.execute(
            "style.character.edit",
            &json!({"name": "Emphasis", "rename": "Strong", "basedOn": "Base", "chars": {"size": 14, "fill": "[Paper]", "fontStyle": null}}),
        )
        .unwrap();
        let d = s.doc().unwrap().doc.clone();
        assert!(d.styles.char_style("Emphasis").is_none());
        let st = d.styles.char_style("Strong").unwrap();
        assert_eq!(st.based_on.as_deref(), Some("Base"));
        // Exactly the attributes set: `null` unset the font style, the rest stayed unset.
        assert_eq!(st.chars, CharAttrs { size: Some(14.0), fill: Some("[Paper]".into()), tracking: Some(20.0), ..Default::default() });
        let story = d.story(kition_layout_doc::StoryId(sid)).unwrap();
        assert!(story.chars.iter().any(|r| r.format.style == "Strong"));
        assert!(story.chars.iter().all(|r| r.format.style != "Emphasis"));
        let lead = d.styles.para("Lead").unwrap();
        assert_eq!(lead.para.nested_styles.as_ref().unwrap()[0].style, "Strong");
        assert_eq!(lead.para.grep_styles.as_ref().unwrap()[0].style, "Strong");
        assert_eq!(lead.para.nested_line_styles.as_ref().unwrap()[0].style, "Strong");
        assert_eq!(d.footnote_options.ref_char_style, "Strong");

        // Based On [None] (null or the name) clears it.
        s.execute("style.character.edit", &json!({"name": "Strong", "basedOn": null})).unwrap();
        assert_eq!(s.doc().unwrap().doc.styles.char_style("Strong").unwrap().based_on, None);
        s.execute("style.character.edit", &json!({"name": "Strong", "basedOn": "Base"})).unwrap();
        s.execute("style.character.edit", &json!({"name": "Strong", "basedOn": "[None]"})).unwrap();
        assert_eq!(s.doc().unwrap().doc.styles.char_style("Strong").unwrap().based_on, None);

        // Refused: a based-on cycle, an unknown parent, a taken or empty name, editing [None].
        s.execute("style.character.edit", &json!({"name": "Strong", "basedOn": "Base"})).unwrap();
        assert!(s.execute("style.character.edit", &json!({"name": "Base", "basedOn": "Strong"})).is_err());
        assert!(s.execute("style.character.edit", &json!({"name": "Strong", "basedOn": "Nope"})).is_err());
        assert!(s.execute("style.character.edit", &json!({"name": "Strong", "rename": "Base"})).is_err());
        assert!(s.execute("style.character.edit", &json!({"name": "Strong", "rename": "  "})).is_err());
        assert!(s.execute("style.character.edit", &json!({"name": "[None]", "chars": {"size": 9}})).is_err());
        assert!(s.doc().unwrap().doc.styles.char_style("[None]").unwrap().chars.is_empty());
    }

    #[test]
    fn new_character_style_checks_its_based_on() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        assert!(s.execute("style.character.add", &json!({"name": "Orphan", "basedOn": "Nope"})).is_err());
        s.execute("style.character.add", &json!({"name": "Plain", "basedOn": "[None]", "chars": {"size": 9}})).unwrap();
        let st = s.doc().unwrap().doc.styles.char_style("Plain").cloned().unwrap();
        assert_eq!((st.based_on, st.chars), (None, CharAttrs { size: Some(9.0), ..Default::default() }));
    }

    #[test]
    fn swatches_save_and_load_ase() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("swatch.add", &json!({"name": "Brand Teal", "color": "#108080"})).unwrap();
        let b64 = s.execute("swatch.save", &json!({"names": ["Brand Teal"]})).unwrap()["base64"].as_str().unwrap().to_string();
        s.execute("file.new", &json!({})).unwrap();
        let r = s.execute("swatch.load", &json!({"base64": b64})).unwrap();
        assert_eq!(r["added"], 1);
        assert!(s.doc().unwrap().doc.swatch("Brand Teal").is_some());
        let r = s.execute("swatch.load", &json!({"base64": b64})).unwrap();
        assert_eq!((r["added"].as_u64(), r["replaced"].as_u64()), (Some(0), Some(0)));
        assert!(s.execute("swatch.load", &json!({"base64": "AAAA"})).is_err());
    }

    #[test]
    fn unnamed_colors_then_add_to_swatches() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        let id = s.execute("frame.add", &json!({"rect": [0, 0, 50, 50]})).unwrap()["id"].as_u64().unwrap();
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        let listed = |s: &Session| s.doc().unwrap().doc.swatches.iter().filter(|w| !w.hidden).count();
        let before = listed(&s);
        let r = s.execute("object.color", &json!({"color": {"c": 10, "m": 20, "y": 30, "k": 0}})).unwrap();
        assert_eq!(r["swatch"], "C=10 M=20 Y=30 K=0");
        assert_eq!(listed(&s), before, "unnamed: not in the Swatches panel");
        // The same colour again reuses it; the stroke target works too.
        let n = s.doc().unwrap().doc.swatches.len();
        s.execute("object.color", &json!({"color": {"c": 10, "m": 20, "y": 30, "k": 0}, "target": "stroke"})).unwrap();
        assert_eq!(s.doc().unwrap().doc.swatches.len(), n);
        let d = &s.doc().unwrap().doc;
        let it = d.item(kition_layout_doc::ItemId(id)).unwrap();
        assert_eq!(it.fill.swatch, it.stroke.swatch);
        // Add to Swatches with a name: listed, and the object follows the rename.
        s.execute("swatch.addToSwatches", &json!({"name": "Sand"})).unwrap();
        assert_eq!(listed(&s), before + 1);
        let d = &s.doc().unwrap().doc;
        assert_eq!(d.item(kition_layout_doc::ItemId(id)).unwrap().fill.swatch, "Sand");
        // A colour equal to an existing swatch applies that swatch.
        s.execute("object.color", &json!({"color": {"c": 10, "m": 20, "y": 30, "k": 0}})).unwrap();
        assert_eq!(s.doc().unwrap().doc.item(kition_layout_doc::ItemId(id)).unwrap().fill.swatch, "Sand");
    }
}

#[cfg(test)]
mod group_sample_tests {
    use serde_json::json;

    use crate::Session;

    /// Grouping the sample's table styles keeps its table cells styled (no missing styles).
    #[test]
    fn grouping_reaches_table_cells() {
        let mut s = Session::new();
        s.execute("file.newSample", &json!({})).unwrap();
        let before = s.execute("preflight.run", &json!({})).unwrap()["errors"].as_u64().unwrap();
        s.execute("style.group", &json!({"kind": "paragraph", "names": ["Table Head", "Table Body"], "group": "Tables"})).unwrap();
        let after = s.execute("preflight.run", &json!({})).unwrap();
        assert_eq!(after["errors"].as_u64().unwrap(), before, "{after}");
    }
}

#[cfg(test)]
mod color_group_tests {
    use serde_json::json;

    use crate::Session;

    #[test]
    fn color_groups_hold_swatches_and_round_trip() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        let a = s.execute("swatch.add", &json!({"name": "Brand Red", "color": {"c": 0, "m": 100, "y": 100, "k": 0}})).unwrap()["name"]
            .as_str()
            .unwrap()
            .to_string();
        let b = s.execute("swatch.add", &json!({"name": "Brand Blue", "color": {"c": 100, "m": 60, "y": 0, "k": 0}})).unwrap()["name"]
            .as_str()
            .unwrap()
            .to_string();
        let g = s.execute("swatch.newColorGroup", &json!({"name": "Brand", "swatches": [a]})).unwrap();
        assert_eq!(g["name"], "Brand");
        assert!(s.execute("swatch.newColorGroup", &json!({"swatches": ["[Black]"]})).is_err(), "special swatches stay at the top");
        s.execute("swatch.moveToGroup", &json!({"swatches": [b], "group": "Brand"})).unwrap();
        let groups = |s: &Session| s.doc().unwrap().doc.color_groups.clone();
        assert_eq!(groups(&s)[0].swatches, [a.clone(), b.clone()]);
        let bytes = kition_layout_idml::export_idml(&s.doc().unwrap().doc);
        let back = kition_layout_idml::import_idml(&bytes).unwrap();
        assert_eq!(back.color_groups, groups(&s), "IDML ColorGroup");
        s.execute("swatch.delete", &json!({"name": b})).unwrap();
        assert_eq!(groups(&s)[0].swatches, [a.as_str()]);
        s.execute("swatch.renameColorGroup", &json!({"name": "Brand", "to": "Identity"})).unwrap();
        s.execute("swatch.ungroupColorGroup", &json!({"name": "Identity"})).unwrap();
        assert!(groups(&s).is_empty());
        assert!(s.doc().unwrap().doc.swatch(&a).is_some(), "ungrouping keeps the swatches");
    }
}

#[cfg(test)]
mod ink_tests {
    use serde_json::json;

    use crate::Session;

    #[test]
    fn ink_manager_converts_and_aliases_spots_in_pdf() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        for (n, c) in [("Pantone A", [0, 50, 100, 0]), ("Pantone B", [100, 0, 0, 0])] {
            s.execute("swatch.add", &json!({"name": n, "spot": true, "color": {"c": c[0], "m": c[1], "y": c[2], "k": c[3]}})).unwrap();
        }
        let id = s.execute("frame.add", &json!({"rect": [72, 72, 200, 200]})).unwrap()["id"].clone();
        s.execute("object.fill", &json!({"swatch": "Pantone A", "ids": [id]})).unwrap();
        let pdf = |s: &mut Session| {
            let b = s.execute("file.exportPdf", &json!({})).unwrap()["base64"].as_str().unwrap().to_string();
            String::from_utf8_lossy(&super::super::file::base64_decode(&b)).into_owned()
        };
        assert!(pdf(&mut s).contains("Pantone"), "a separation");
        s.execute("ink.options", &json!({"ink": "Pantone A", "alias": "Pantone B"})).unwrap();
        let p = pdf(&mut s);
        assert!(p.contains("Pantone#20B") || p.contains("Pantone B"), "aliased onto B");
        assert!(!p.contains("Pantone#20A") && !p.contains("/Pantone A"));
        s.execute("ink.options", &json!({"allToProcess": true})).unwrap();
        assert!(!pdf(&mut s).contains("Pantone"), "all spots to process");
        let l = s.execute("ink.list", &json!({})).unwrap();
        assert_eq!(l["inks"][0]["alias"], "Pantone B");
        let back = kition_layout_idml::import_idml(&kition_layout_idml::export_idml(&s.doc().unwrap().doc)).unwrap();
        assert!(back.inks.resolve("Pantone A").1, "IDML ConvertToProcess");
        assert!(back.inks.aliases.iter().any(|(a, b)| a == "Pantone A" && b == "Pantone B"), "IDML AliasInkName");
        assert!(s.execute("ink.options", &json!({"ink": "[Black]", "toProcess": true})).is_err());
    }
}

#[cfg(test)]
mod export_tag_tests {
    use serde_json::json;

    use crate::Session;

    #[test]
    fn export_tagging_shapes_epub_markup() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("style.paragraph.add", &json!({"name": "Chapter Title", "chars": {"size": 24}})).unwrap();
        s.execute("style.character.add", &json!({"name": "Key Term", "chars": {"fontStyle": "Bold"}})).unwrap();
        s.execute("style.exportTag", &json!({"style": "Chapter Title", "tag": "h1", "class": "chapter"})).unwrap();
        s.execute("style.exportTag", &json!({"style": "Key Term", "character": true, "tag": "strong"})).unwrap();
        assert!(s.execute("style.exportTag", &json!({"style": "Nope"})).is_err());
        let r = s.execute("frame.add", &json!({"rect": [72, 72, 400, 300], "content": "text", "text": "Origins\nA word"})).unwrap();
        let sid = r["story"].as_u64().unwrap();
        s.execute("text.select", &json!({"story": sid, "anchor": 0, "focus": 0})).unwrap();
        s.execute("style.paragraph.apply", &json!({"name": "Chapter Title"})).unwrap();
        s.execute("text.select", &json!({"story": sid, "anchor": 10, "focus": 14})).unwrap();
        s.execute("style.character.apply", &json!({"name": "Key Term"})).unwrap();
        let html = s.execute("file.exportHtml", &json!({})).unwrap()["text"].as_str().unwrap().to_string();
        assert!(html.contains("<h1 class=\"chapter\">Origins</h1>"), "{html}");
        assert!(html.contains("<strong class=\"key-term\">word</strong>"), "{html}");
        // Tagged PDF: a heading element and a paragraph element.
        let r = s.execute("file.exportPdf", &json!({"tagged": true, "compress": false})).unwrap();
        let pdf = super::super::file::base64_decode(r["base64"].as_str().unwrap());
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/S/H1") && text.contains("/S/P"), "structure elements");
        assert!(kition_layout_render::pdf_page_count(&pdf) == Some(1));
    }
}

#[cfg(test)]
mod cjk_tests {
    use super::*;
    #[test]
    fn cjk_composite_font_command_invalidates_layout_and_undo_restores_it() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        let r = s.execute("frame.add", &json!({"rect": [0, 0, 300, 150], "content": "text", "text": "AB"})).unwrap();
        let sid = kition_layout_doc::StoryId(r["story"].as_u64().unwrap());
        let definition = |scale| json!({"name": "Mixed", "entries": [{"family": "Source Serif 4", "relativeSize": scale}]});
        s.execute("style.compositeFont.set", &definition(1.0)).unwrap();
        s.execute("text.select", &json!({"story": sid.0, "anchor": 0, "focus": 2})).unwrap();
        s.execute("text.char", &json!({"fontFamily": "Mixed", "leadingAki": 0.25, "jidori": 4})).unwrap();
        let old = s.cache.get(&s.doc().unwrap().doc, sid, None);
        s.execute("style.compositeFont.set", &definition(2.0)).unwrap();
        let new = s.cache.get(&s.doc().unwrap().doc, sid, None);
        assert!(!std::sync::Arc::ptr_eq(&old, &new));
        assert!(new.frames[0].lines[0].end_x > old.frames[0].lines[0].end_x);
        s.execute("edit.undo", &json!({})).unwrap();
        let restored = s.cache.get(&s.doc().unwrap().doc, sid, None);
        assert_eq!(old.frames[0].lines[0].end_x, restored.frames[0].lines[0].end_x);
        assert!(s.execute("style.compositeFont.set", &definition(-1.0)).is_err());
    }
}
