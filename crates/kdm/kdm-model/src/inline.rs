//! Inline objects (design note section 2.4): footnotes, endnotes, text anchors,
//! cross-references, index entries, anchored objects and editorial notes, one `Inline` object
//! each, anchored in the story's text by the `inline` mark on its placeholder character. The
//! engine keeps them in one vector per kind, one entry per placeholder in text order.

use std::collections::HashMap;
use std::sync::Arc;

use kition_kit::kdm::ObjectId;
use kition_kit::session::TextRun;
use kition_layout_doc::{AnchoredObject, CrossRef, EditorialNote, Footnote, IndexRef, Item, Story, TextAnchor};
use serde_json::{Map, Value, json};

use crate::fields::{Fields, In, Out, as_object};
use crate::ids::{InlineKey, InlineKind};
use crate::text::{Anchors, nested_extras, nested_story, nested_table, story_runs};
use crate::{KdmResult, kdm_err};

/// One inline object of a story, borrowed from the engine's vectors.
#[derive(Clone, Copy)]
pub(crate) enum InlineRef<'a> {
    Object(&'a AnchoredObject),
    Footnote(&'a Footnote),
    Endnote(&'a Footnote),
    CrossReference(&'a CrossRef),
    TextAnchor(&'a TextAnchor),
    IndexEntry(&'a IndexRef),
    EditorialNote(&'a EditorialNote),
}

impl InlineRef<'_> {
    fn same(&self, other: &InlineRef<'_>) -> bool {
        match (self, other) {
            (InlineRef::Object(a), InlineRef::Object(b)) => std::ptr::eq(*a, *b) || a == b,
            (InlineRef::Footnote(a), InlineRef::Footnote(b)) | (InlineRef::Endnote(a), InlineRef::Endnote(b)) => std::ptr::eq(*a, *b) || a == b,
            (InlineRef::CrossReference(a), InlineRef::CrossReference(b)) => a == b,
            (InlineRef::TextAnchor(a), InlineRef::TextAnchor(b)) => a == b,
            (InlineRef::IndexEntry(a), InlineRef::IndexEntry(b)) => a == b,
            (InlineRef::EditorialNote(a), InlineRef::EditorialNote(b)) => a == b,
            _ => false,
        }
    }
}

/// Whether two inline objects write the same registers.
pub(crate) fn same_inline(a: &InlineRef<'_>, b: &InlineRef<'_>) -> bool {
    a.same(b)
}

/// The inline object of each placeholder character of `story`, in text order (`None` where the
/// engine's vector has no entry for it), with its key.
pub(crate) fn inline_list(story: &Story) -> Vec<Option<(InlineKey, InlineRef<'_>)>> {
    let mut counts: HashMap<InlineKind, usize> = HashMap::new();
    let mut out = Vec::new();
    for c in story.text.chars() {
        let Some(kind) = InlineKind::of_char(c) else { continue };
        let n = counts.entry(kind).or_insert(0);
        let at = *n;
        *n += 1;
        let entry: Option<(u64, InlineRef<'_>)> = match kind {
            InlineKind::AnchoredObject => story.objects.get(at).map(|o| (o.item.id.0, InlineRef::Object(o))),
            InlineKind::Footnote => story.notes.get(at).map(|f| (f.id, InlineRef::Footnote(f))),
            InlineKind::Endnote => story.endnotes.get(at).map(|f| (f.id, InlineRef::Endnote(f))),
            InlineKind::CrossReference => story.xrefs.get(at).map(|x| (at as u64, InlineRef::CrossReference(x))),
            InlineKind::TextAnchor => story.anchors.get(at).map(|a| (a.id, InlineRef::TextAnchor(a))),
            InlineKind::IndexEntry => story.index_refs.get(at).map(|i| (at as u64, InlineRef::IndexEntry(i))),
            InlineKind::EditorialNote => story.editorial.get(at).map(|e| (e.id, InlineRef::EditorialNote(e))),
        };
        out.push(entry.map(|(local, r)| (InlineKey { kind, local }, r)));
    }
    out
}

/// Each placeholder's object, given the story's inline objects by key.
pub(crate) fn placeholder_objects(list: &[Option<(InlineKey, InlineRef<'_>)>], objects: &[(InlineKey, ObjectId)]) -> Vec<Option<ObjectId>> {
    let mut by_key: HashMap<InlineKey, Vec<&ObjectId>> = HashMap::new();
    for (key, object) in objects.iter().rev() {
        by_key.entry(*key).or_default().push(object);
    }
    list.iter().map(|entry| entry.as_ref().and_then(|(key, _)| by_key.get_mut(key).and_then(Vec::pop).cloned())).collect()
}

/// An inline object's registers and text.
pub(crate) fn inline_fields(r: &InlineRef<'_>, out: &Out<'_>) -> KdmResult<(Fields, Vec<TextRun>)> {
    let nested = |story: &Story| story_runs(story, &out.names, &Anchors { table: &nested_table, inlines: None });
    let (kind, item, data, runs) = match r {
        InlineRef::Object(o) => {
            let item = out.ids.items.object(o.item.id).map(|o| Value::String(o.as_str().to_owned())).unwrap_or(Value::Null);
            (InlineKind::AnchoredObject, item, json!({ "position": serde_json::to_value(&o.position).map_err(kdm_err)? }), Vec::new())
        }
        InlineRef::Footnote(f) => (InlineKind::Footnote, Value::Null, json!({ "story": nested_extras(&f.text)? }), nested(&f.text)?),
        InlineRef::Endnote(f) => (InlineKind::Endnote, Value::Null, json!({ "story": nested_extras(&f.text)? }), nested(&f.text)?),
        InlineRef::CrossReference(x) => {
            let mut data = Map::new();
            data.insert("format".into(), Value::String(x.format.clone()));
            if let Some(target) = out.anchors.get(&x.target) {
                data.insert("target".into(), Value::String(target.as_str().to_owned()));
            }
            (InlineKind::CrossReference, Value::Null, Value::Object(data), Vec::new())
        }
        InlineRef::TextAnchor(a) => (InlineKind::TextAnchor, Value::Null, json!({ "name": a.name }), Vec::new()),
        InlineRef::IndexEntry(i) => (InlineKind::IndexEntry, Value::Null, serde_json::to_value(i).map_err(kdm_err)?, Vec::new()),
        InlineRef::EditorialNote(e) => {
            let runs = if e.text.is_empty() { Vec::new() } else { vec![TextRun::new(e.text.clone())] };
            (InlineKind::EditorialNote, Value::Null, json!({ "author": e.author }), runs)
        }
    };
    let mut data = data;
    crate::refs::swatches_out(&mut data, &out.names.swatches);
    Ok((vec![("kind", Value::String(kind.name().to_owned())), ("item", item), ("data", data)], runs))
}

/// An inline object read back (an anchored object without its item, which the caller fills in).
pub(crate) enum InlineValue {
    Object(Option<ObjectId>, Value),
    Footnote(Footnote),
    Endnote(Footnote),
    CrossReference(CrossRef),
    TextAnchor(TextAnchor),
    IndexEntry(IndexRef),
    EditorialNote(EditorialNote),
}

/// Reads an inline object of `kind`; `local` is its minted id (kinds with ids of their own).
pub(crate) fn inline_from(kind: InlineKind, local: u64, get: &dyn Fn(&str) -> Value, runs: &[TextRun], inn: &In) -> KdmResult<InlineValue> {
    let mut data = get("data");
    crate::refs::swatches_in(&mut data, &inn.names.swatches);
    let story = |data: &Value| -> KdmResult<kition_layout_doc::Story> {
        if runs.is_empty() {
            let mut s = Story::new(kition_layout_doc::StoryId(0));
            if let Some(Value::Object(extras)) = data.get("story")
                && let Some(id) = extras.get("id").and_then(Value::as_u64)
            {
                s.id = kition_layout_doc::StoryId(id);
            }
            return Ok(s);
        }
        nested_story(runs, data.get("story").unwrap_or(&Value::Null), &inn.names)
    };
    Ok(match kind {
        InlineKind::AnchoredObject => InlineValue::Object(as_object(&get("item")), data.get("position").cloned().unwrap_or(Value::Null)),
        InlineKind::Footnote => InlineValue::Footnote(Footnote { id: local, text: story(&data)? }),
        InlineKind::Endnote => InlineValue::Endnote(Footnote { id: local, text: story(&data)? }),
        InlineKind::CrossReference => {
            let target = data.get("target").and_then(as_object).and_then(|o| inn.anchors.get(&o).copied()).unwrap_or(0);
            InlineValue::CrossReference(CrossRef { target, format: data.get("format").and_then(Value::as_str).unwrap_or_default().to_owned() })
        }
        InlineKind::TextAnchor => {
            InlineValue::TextAnchor(TextAnchor { id: local, name: data.get("name").and_then(Value::as_str).unwrap_or_default().to_owned() })
        }
        InlineKind::IndexEntry => InlineValue::IndexEntry(serde_json::from_value(data).map_err(kdm_err)?),
        InlineKind::EditorialNote => InlineValue::EditorialNote(EditorialNote {
            id: local,
            author: data.get("author").and_then(Value::as_str).unwrap_or_default().to_owned(),
            text: runs.iter().map(|r| r.insert.as_str()).collect(),
        }),
    })
}

/// Pushes `value` onto the story vector of its kind.
pub(crate) fn push_inline(story: &mut Story, value: InlineValue, item: Option<Item>) -> KdmResult<()> {
    match value {
        InlineValue::Object(_, position) => {
            let Some(item) = item else { return Err(kdm_err("an anchored object without its item")) };
            let position = if position.is_null() { Default::default() } else { serde_json::from_value(position).map_err(kdm_err)? };
            story.objects.push(Arc::new(AnchoredObject { item, position }));
        }
        InlineValue::Footnote(f) => story.notes.push(Arc::new(f)),
        InlineValue::Endnote(f) => story.endnotes.push(Arc::new(f)),
        InlineValue::CrossReference(x) => story.xrefs.push(Arc::new(x)),
        InlineValue::TextAnchor(a) => story.anchors.push(Arc::new(a)),
        InlineValue::IndexEntry(i) => story.index_refs.push(Arc::new(i)),
        InlineValue::EditorialNote(e) => story.editorial.push(Arc::new(e)),
    }
    Ok(())
}
