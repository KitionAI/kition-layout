//! The story texts this kit process has held (C8-layout review 3). A change set gives a text over
//! 80 characters as `{excerpt, length}`, so the guard's `context.covered` cannot rebuild the text a
//! byte-offset write was observed against. Every state an agent can observe passed through this
//! process (opened, changed by a command, inspected, or read for derived values), so its long
//! texts are kept here, a few per story, and the text at `basis` is the one kept text the excerpt
//! and length fit. None or several fit: not known, and the write refuses as before.
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use kition_layout_engine::doc::Document;
use serde_json::Value;

/// Texts kept per story.
const PER_STORY: usize = 8;
/// Bytes kept in all; beyond, everything is forgotten (a write over a long text then refuses).
const BUDGET: usize = 64 << 20;
/// A change set gives a text whole up to this many characters.
const SHOWN: usize = 80;

#[derive(Default)]
struct Kept {
    texts: HashMap<(String, u64), VecDeque<Arc<str>>>,
    bytes: usize,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

/// The object model's text of a story: the engine's text and the final paragraph break.
pub(crate) fn modelled(text: &str) -> String {
    format!("{text}\n")
}

/// Keeps every long story text of `doc` this process has not kept yet.
pub(crate) fn remember(doc: &Document) {
    let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    let kept = kept.get_or_insert_with(Kept::default);
    for story in doc.stories.values() {
        if story.text.chars().nth(SHOWN).is_none() {
            continue;
        }
        let text = modelled(&story.text);
        let texts = kept.texts.entry((doc.title.clone(), story.id.0)).or_default();
        if texts.iter().any(|known| **known == *text) {
            continue;
        }
        kept.bytes = kept.bytes.saturating_add(text.len());
        texts.push_front(Arc::from(text));
        if texts.len() > PER_STORY
            && let Some(old) = texts.pop_back()
        {
            kept.bytes = kept.bytes.saturating_sub(old.len());
        }
    }
    if kept.bytes > BUDGET {
        *kept = Kept::default();
    }
}

/// The one kept text of story `story` that a change set's `{excerpt, length}` describes, if exactly
/// one fits.
pub(super) fn recall(doc: &Document, story: u64, shown: &Value) -> Option<String> {
    let excerpt = shown.get("excerpt")?.as_str()?;
    let length = usize::try_from(shown.get("length")?.as_u64()?).ok()?;
    let lead = excerpt.starts_with('…');
    let tail = excerpt.ends_with('…') && excerpt.chars().count() > 1;
    let mut body = excerpt;
    if lead {
        body = body.strip_prefix('…')?;
    }
    if tail {
        body = body.strip_suffix('…')?;
    }
    let kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    let texts = kept.as_ref()?.texts.get(&(doc.title.clone(), story))?;
    let mut fitting = texts
        .iter()
        .filter(|text| text.chars().count() == length && text.contains(body) && (lead || text.starts_with(body)) && (tail || text.ends_with(body)));
    let one = fitting.next()?;
    fitting.next().is_none().then(|| one.to_string())
}
