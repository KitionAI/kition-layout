//! A story's text as one rich `text` field (design note section 2.4).
//!
//! The engine keeps a story as one `String` with byte-length character runs and one paragraph
//! format per paragraph (the last paragraph has no `\n`). The replica's text is the engine's text
//! plus one final `\n`, so every paragraph, the last included, ends with a break that carries its
//! format ([`crate::marks`]); the bridge adds the final break on write and drops it on read. A
//! text that does not end with `\n` is refused at read as damaged (rule V11): the bridge never
//! appends one, because two replicas appending would give two.
//!
//! Inline objects (footnotes, anchors, cross-references, index entries, anchored objects,
//! editorial notes) keep the engine's placeholder characters; a top-level story's placeholders
//! carry the `inline` mark naming their `Inline` object, and a paragraph that anchors a table
//! carries the `table` mark on its break. Stories nested in other values (table cells, footnote
//! text) keep their own inline vectors and tables as data: their placeholders carry no mark and
//! their `table` marks hold the engine's table number.

use std::collections::{BTreeMap, HashMap};

use kition_kit::kdm::ObjectId;
use kition_kit::session::TextRun;
use kition_layout_doc::{BASIC_PARAGRAPH, CharFormat, CharRun, NO_CHAR_STYLE, ParaFormat, Story};
use serde_json::{Map, Value, json};

use crate::ids::{CHARACTER, FAMILIES, InlineKind, PARAGRAPH};
use crate::marks::{self, C_STYLE, INLINE, P_STYLE, TABLE};
use crate::refs::{swatches_in, swatches_out};
use crate::{KdmResult, kdm_err};

/// Names to objects, for writing.
pub(crate) struct NamesOut {
    pub styles: [HashMap<String, ObjectId>; FAMILIES],
    pub swatches: HashMap<String, ObjectId>,
}

/// Objects to names, for reading.
#[derive(Default)]
pub(crate) struct NamesIn {
    pub styles: [HashMap<ObjectId, String>; FAMILIES],
    pub swatches: HashMap<ObjectId, String>,
}

/// The name a style reference value reads as in family `family`: the style's current name, the
/// name an unresolved reference kept (`{"name": …}`), or `fallback` (no reference, or a style
/// that is gone: rule V7's default, applied at read because a transaction cannot read marks).
pub(crate) fn style_name(names: &NamesIn, family: usize, v: Option<&Value>, fallback: &str) -> String {
    match v {
        Some(Value::String(id)) => names.styles[family].get(&ObjectId::from_string(id.as_str())).cloned().unwrap_or_else(|| fallback.to_owned()),
        Some(Value::Object(map)) => map.get("name").and_then(Value::as_str).unwrap_or(fallback).to_owned(),
        _ => fallback.to_owned(),
    }
}

/// The value a style reference by `name` is written as: the style's object id, or `{"name": …}`
/// when no style of the family has that name.
pub(crate) fn style_value(names: &NamesOut, family: usize, name: &str) -> Value {
    match names.styles[family].get(name) {
        Some(object) => Value::String(object.as_str().to_owned()),
        None => json!({ "name": name }),
    }
}

/// How a story's anchors are written: the `table` mark's value for an engine table number, and
/// each placeholder character's `Inline` object in text order (`None` for a nested story).
pub(crate) struct Anchors<'a> {
    pub table: &'a dyn Fn(u64) -> Option<Value>,
    pub inlines: Option<&'a [Option<ObjectId>]>,
}

/// The table mark of a nested story: the engine's own table number.
pub(crate) fn nested_table(table: u64) -> Option<Value> {
    Some(Value::from(table))
}

fn char_map(format: &CharFormat, names: &NamesOut) -> KdmResult<BTreeMap<String, Value>> {
    let mut out = BTreeMap::new();
    if format.style != NO_CHAR_STYLE || names.styles[CHARACTER].contains_key(&format.style) {
        out.insert(C_STYLE.to_owned(), style_value(names, CHARACTER, &format.style));
    }
    let mut over = Value::Object(marks::char_marks(&format.over)?);
    swatches_out(&mut over, &names.swatches);
    if let Value::Object(map) = over {
        out.extend(map);
    }
    Ok(out)
}

fn para_map(para: &ParaFormat, names: &NamesOut, anchors: &Anchors<'_>) -> KdmResult<BTreeMap<String, Value>> {
    let mut out = BTreeMap::new();
    out.insert(P_STYLE.to_owned(), style_value(names, PARAGRAPH, &para.style));
    let mut groups = Value::Object(marks::para_marks(&para.para)?);
    swatches_out(&mut groups, &names.swatches);
    if let Value::Object(map) = groups {
        out.extend(map);
    }
    if let Some(table) = para.table
        && let Some(v) = (anchors.table)(table)
    {
        out.insert(TABLE.to_owned(), v);
    }
    Ok(out)
}

/// Builds runs, merging adjacent characters with the same marks.
#[derive(Default)]
struct Runs {
    out: Vec<TextRun>,
}

impl Runs {
    fn push(&mut self, text: &str, attributes: &BTreeMap<String, Value>) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.out.last_mut()
            && last.attributes == *attributes
        {
            last.insert.push_str(text);
            return;
        }
        self.out.push(TextRun { insert: text.to_owned(), attributes: attributes.clone() });
    }
}

/// `story`'s text as the replica holds it (with the final `\n`), as runs.
pub(crate) fn story_runs(story: &Story, names: &NamesOut, anchors: &Anchors<'_>) -> KdmResult<Vec<TextRun>> {
    let char_maps: Vec<BTreeMap<String, Value>> = story.chars.iter().map(|r| char_map(&r.format, names)).collect::<KdmResult<_>>()?;
    let para_maps: Vec<BTreeMap<String, Value>> = story.paras.iter().map(|p| para_map(p, names, anchors)).collect::<KdmResult<_>>()?;
    let empty = BTreeMap::new();
    let mut runs = Runs::default();
    let text = &story.text;
    let mut run = 0usize;
    let mut run_end = story.chars.first().map_or(usize::MAX, |r| r.len);
    let mut para = 0usize;
    let mut placeholder = 0usize;
    let mut plain_start = 0usize;
    let char_attrs = |run: usize| char_maps.get(run).or(char_maps.last()).unwrap_or(&empty);
    for (i, c) in text.char_indices() {
        while i >= run_end && run + 1 < story.chars.len() {
            runs.push(&text[plain_start..i], char_attrs(run));
            plain_start = i;
            run += 1;
            run_end = run_end.saturating_add(story.chars[run].len);
        }
        let special = c == '\n' || (anchors.inlines.is_some() && InlineKind::of_char(c).is_some());
        if !special {
            continue;
        }
        runs.push(&text[plain_start..i], char_attrs(run));
        let mut attrs = char_attrs(run).clone();
        if c == '\n' {
            attrs.extend(para_maps.get(para).cloned().unwrap_or_default());
            para += 1;
        } else if let Some(list) = anchors.inlines {
            if let Some(Some(object)) = list.get(placeholder) {
                attrs.insert(INLINE.to_owned(), Value::String(object.as_str().to_owned()));
            }
            placeholder += 1;
        }
        let end = i + c.len_utf8();
        runs.push(&text[i..end], &attrs);
        plain_start = end;
    }
    runs.push(&text[plain_start..], char_attrs(run));
    // The last paragraph's break.
    let last_run = story.chars.len().saturating_sub(1).max(run);
    let mut attrs = char_attrs(last_run).clone();
    attrs.extend(para_maps.get(para).or(para_maps.last()).cloned().unwrap_or_default());
    runs.push("\n", &attrs);
    Ok(runs.out)
}

/// A story's text read back: the engine's text, runs and paragraph formats, and for each
/// placeholder character (in text order) the `Inline` object its `inline` mark names.
pub(crate) struct Decoded {
    pub text: String,
    pub chars: Vec<CharRun>,
    pub paras: Vec<ParaFormat>,
    pub placeholders: Vec<(InlineKind, Option<ObjectId>)>,
}

/// Reads `runs` (a story's text field) back; `table` maps a `table` mark's value to the engine's
/// table number. Errs when the text does not end with `\n` (rule V11: damaged).
pub(crate) fn decode(runs: &[TextRun], names: &NamesIn, table: &dyn Fn(&Value) -> Option<u64>) -> KdmResult<Decoded> {
    if !runs.last().is_some_and(|r| r.insert.ends_with('\n')) {
        return Err(kdm_err("damaged: a story's text does not end with a paragraph break"));
    }
    let mut text = String::new();
    let mut chars: Vec<CharRun> = Vec::new();
    let mut paras = Vec::new();
    let mut placeholders = Vec::new();
    for run in runs {
        let mut attrs = run.attributes.clone();
        for v in attrs.values_mut() {
            swatches_in(v, &names.swatches);
        }
        let format = CharFormat { style: style_name(names, CHARACTER, attrs.get(C_STYLE), NO_CHAR_STYLE), over: marks::char_attrs(&attrs)? };
        match chars.last_mut() {
            Some(last) if last.format == format => last.len += run.insert.len(),
            _ => chars.push(CharRun { len: run.insert.len(), format }),
        }
        text.push_str(&run.insert);
        let breaks = run.insert.matches('\n').count();
        if breaks > 0 {
            let para = ParaFormat {
                style: style_name(names, PARAGRAPH, attrs.get(P_STYLE), BASIC_PARAGRAPH),
                para: marks::para_attrs(attrs.iter())?,
                chars: Default::default(),
                table: attrs.get(TABLE).and_then(table),
            };
            for _ in 0..breaks {
                paras.push(para.clone());
            }
        }
        let inline = attrs.get(INLINE).and_then(Value::as_str).map(ObjectId::from_string);
        for c in run.insert.chars() {
            if let Some(kind) = InlineKind::of_char(c) {
                placeholders.push((kind, inline.clone()));
            }
        }
    }
    text.pop();
    if let Some(last) = chars.last_mut() {
        last.len = last.len.saturating_sub(1);
    }
    Ok(Decoded { text, chars, paras, placeholders })
}

/// The keys of a serialized story the bridge models itself.
const STORY_TEXT: &[&str] = &["text", "paras", "chars", "rev"];

/// What a nested story (a cell's or a footnote's) holds beyond its text, runs and paragraphs: kept
/// as data.
pub(crate) fn nested_extras(story: &Story) -> KdmResult<Value> {
    let mut v = serde_json::to_value(story).map_err(kdm_err)?;
    if let Some(map) = v.as_object_mut() {
        for key in STORY_TEXT {
            map.remove(*key);
        }
    }
    Ok(v)
}

/// A nested story from its text field and [`nested_extras`].
pub(crate) fn nested_story(runs: &[TextRun], extras: &Value, names: &NamesIn) -> KdmResult<Story> {
    let decoded = decode(runs, names, &|v| v.as_u64())?;
    let mut map = match extras {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    map.entry("id").or_insert(Value::from(0));
    map.entry("frames").or_insert(Value::Array(Vec::new()));
    map.insert("text".into(), Value::String(String::new()));
    map.insert("paras".into(), Value::Array(Vec::new()));
    map.insert("chars".into(), Value::Array(Vec::new()));
    let mut story: Story = serde_json::from_value(Value::Object(map)).map_err(kdm_err)?;
    story.text = decoded.text;
    story.chars = decoded.chars;
    story.paras = decoded.paras;
    Ok(story)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kition_layout_doc::StoryId;

    fn names() -> NamesOut {
        NamesOut { styles: Default::default(), swatches: HashMap::new() }
    }

    fn round_trip(story: &Story) -> Story {
        let runs = story_runs(story, &names(), &Anchors { table: &nested_table, inlines: None }).unwrap();
        assert!(runs.last().unwrap().insert.ends_with('\n'));
        nested_story(&runs, &nested_extras(story).unwrap(), &NamesIn::default()).unwrap()
    }

    #[test]
    fn a_story_reads_back_with_its_runs_and_paragraphs() {
        let mut s = Story::with_text(StoryId(7), "Hello world\nSecond para\n\nlast", ParaFormat::default());
        s.paras[1].para.set_json("leftIndent", &json!(18)).unwrap();
        s.paras[3].style = "Heading".into();
        // Bold "world".
        s.chars = vec![
            CharRun { len: 6, format: CharFormat::default() },
            CharRun { len: 5, format: CharFormat { style: "Strong".into(), over: Default::default() } },
            CharRun { len: s.text.len() - 11, format: CharFormat::default() },
        ];
        let back = round_trip(&s);
        assert_eq!(back.text, s.text);
        assert_eq!(back.paras, s.paras);
        assert_eq!(back.chars, s.chars);
    }

    #[test]
    fn an_empty_story_keeps_its_one_paragraph_and_format() {
        let mut s = Story::new(StoryId(1));
        s.chars[0].format.over.set_json("size", &json!(30)).unwrap();
        s.paras[0].style = "Title".into();
        let runs = story_runs(&s, &names(), &Anchors { table: &nested_table, inlines: None }).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].insert, "\n");
        let back = round_trip(&s);
        assert_eq!((back.text, back.chars, back.paras), (s.text.clone(), s.chars.clone(), s.paras.clone()));
    }

    #[test]
    fn a_text_without_its_final_break_is_damaged() {
        let runs = vec![TextRun::new("no break")];
        assert!(decode(&runs, &NamesIn::default(), &|_| None).is_err());
        assert!(decode(&[], &NamesIn::default(), &|_| None).is_err());
    }

    #[test]
    fn paragraph_marks_sit_on_the_break_only() {
        let mut s = Story::with_text(StoryId(2), "ab\ncd", ParaFormat::default());
        s.paras[0].para.set_json("spaceBefore", &json!(6)).unwrap();
        let runs = story_runs(&s, &names(), &Anchors { table: &nested_table, inlines: None }).unwrap();
        let with: Vec<&str> = runs.iter().filter(|r| r.attributes.contains_key("pSpacing")).map(|r| r.insert.as_str()).collect();
        assert_eq!(with, vec!["\n"]);
        assert!(runs.iter().filter(|r| r.attributes.contains_key(P_STYLE)).all(|r| r.insert.chars().all(|c| c == '\n')));
    }
}
