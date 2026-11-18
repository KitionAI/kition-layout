use super::*;
use kition_layout_engine::doc::StoryId;

/// A text write addressed by byte offsets: its story and the window `[start, end)` it replaces
/// (`start == end` for an insertion).
pub(super) struct Window {
    story: StoryId,
    start: usize,
    end: usize,
}

/// The offset window of `command`, for the commands whose handler takes the story and both
/// offsets from their parameters. Selection-addressed writes return `None`.
pub(super) fn window(command: &str, params: &Value) -> Option<Window> {
    let story = StoryId(params.get("story").and_then(Value::as_u64)?);
    let offset = |name: &str| params.get(name).and_then(Value::as_u64).and_then(|value| usize::try_from(value).ok());
    let (start, end) = match command {
        "story.replaceRange" => {
            let start = offset("start").unwrap_or(0);
            (start, offset("end").unwrap_or(start))
        }
        "spelling.change" => (offset("start").unwrap_or(0), offset("end").unwrap_or(0)),
        "anchored.insert" => {
            let pos = offset("pos")?;
            (pos, pos)
        }
        _ => return None,
    };
    Some(Window { story, start: start.min(end), end: start.max(end) })
}

/// The character index of byte `offset` in `text`, clamped to the text and floored to a
/// character boundary.
fn char_index(text: &str, offset: usize) -> usize {
    let mut at = offset.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    text.get(..at).map_or(0, |prefix| prefix.chars().count())
}

/// The text formatting runs (`[{insert, attributes}]`) hold, `None` when a run inserts anything
/// but text.
fn runs_text(runs: &[Value]) -> Option<String> {
    runs.iter().map(|run| run.get("insert")?.as_str()).collect()
}

/// Whether every change from `before` to `now` lies at or after character `end` of `before`,
/// with the earliest start any alignment of the difference allows (an insertion into repeated
/// text could have been made earlier than a plain comparison places it). `None` when the plain
/// texts are equal: the change was formatting only, whose place a change set does not tell.
fn only_after(before: &str, now: &str, end: usize) -> Option<bool> {
    if before == now {
        return None;
    }
    let (b, n): (Vec<char>, Vec<char>) = (before.chars().collect(), now.chars().collect());
    let prefix = b.iter().zip(&n).take_while(|(x, y)| x == y).count();
    let suffix = b.iter().rev().zip(n.iter().rev()).take_while(|(x, y)| x == y).count();
    let earliest = prefix.min(b.len().min(n.len()).saturating_sub(suffix));
    Some(earliest >= end)
}

impl LayoutDocument {
    pub(super) fn text_premises(&self, command: &str, params: &Value, under: &mut Under) -> Result<(), String> {
        let state = self.active()?;
        if let Some(window) = window(command, params) {
            // A byte offset is a positional address: the write rests on the text before and in
            // its window, told by `covered`, and on the story's existence, not on later text.
            if let Some(story) = state.doc.story(window.story) {
                under.objects.push(json!(window.story.0));
                let held = story.text.get(window.start.min(story.text.len())..window.end.min(story.text.len())).unwrap_or_default();
                // Anchored objects, notes and tables in the window are deleted with it.
                let fields: Vec<&str> = if held.chars().any(|c| ('\u{E000}'..='\u{F8FF}').contains(&c)) { vec!["inlines", "tables"] } else { vec![] };
                under.premises.push(Premise::on(window.story.0).fields(fields).positions(Vec::<String>::new()));
            }
            under.covers = Some(Covers { fields: vec!["text".to_owned()] });
            return Ok(());
        }
        let formatting = command.starts_with("type.")
            || matches!(command, "style.paragraph.apply" | "style.character.apply" | "style.breakLink" | "condition.apply");
        let editing = command.starts_with("text.")
            || matches!(
                command,
                "footnote.insert"
                    | "endnote.insert"
                    | "note.new"
                    | "anchor.create"
                    | "xref.insert"
                    | "anchored.insert"
                    | "variables.insert"
                    | "xml.tagText"
                    | "index.addReference"
            );
        let mut stories = Vec::new();
        if command == "frame.fitText" {
            stories.extend(state.selection.text.map(|text| text.story));
            let items = params
                .get("ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_u64).map(ItemId).collect())
                .unwrap_or_else(|| state.selection.items.clone());
            stories.extend(items.into_iter().filter_map(|id| state.doc.item(id)?.text_frame().map(|frame| frame.story)));
        }
        if formatting {
            for target in kition_layout_engine::cmd::text::format_targets_pub(&self.session) {
                stories.push(target.story);
            }
        }
        if editing {
            stories.extend(state.selection.text.map(|text| text.story));
        }
        if matches!(command, "story.setText" | "story.replaceRange" | "story.setDirection" | "type.fillWithPlaceholder") || editing {
            stories.extend(kition_layout_engine::cmd::text::story_of(&self.session, params));
        }
        stories.sort();
        stories.dedup();
        for sid in stories {
            under.premises.push(Premise::on(sid.0));
            if matches!(command, "frame.fitText" | "text.placeCaret" | "text.release" | "text.extendTo" | "text.selectWord" | "text.move") {
                self.composition_premises(sid, under)?;
            }
        }
        Ok(())
    }

    /// `context.covered` for an offset window: the story was under the window at `basis` when
    /// every change of its text since then lies at or after the window's end (in the text at
    /// `basis`). A change before or inside the window, or one whose place cannot be told
    /// (formatting only, or a long text the change set gives only as an excerpt), is not.
    pub(super) fn text_covered(&self, window: &Window, asks: &[CoveredAsk]) -> Result<CoveredAt, String> {
        let state = self.active()?;
        let story = numeric_id(&json!(window.story.0));
        // The object model's text: the engine's text and the final paragraph break.
        let now = state.doc.story(window.story).map(|story| super::history::modelled(&story.text));
        let unknown = || CoveredAt {
            under: vec![],
            covered: asks.iter().map(|ask| Covered { id: ask.id.clone(), covered: None, ..Default::default() }).collect(),
        };
        let mut then = true;
        for ask in asks.iter().filter(|ask| numeric_id(&ask.id) == story) {
            if ask.created {
                then = false;
                continue;
            }
            if ask.deleted {
                continue;
            }
            let Some(before) = ask.before.get("text") else { continue };
            // A text over 80 characters is given as `{excerpt, length}`: the one text this
            // process kept that fits it (C8-layout review 3).
            let before = match before {
                Value::String(text) => Some(text.clone()),
                // A text with formatting is given as its runs: their inserted text, in order.
                Value::Array(runs) => runs_text(runs),
                shown => super::history::recall(&state.doc, window.story.0, shown),
            };
            let (Some(before), Some(now)) = (before.as_deref(), now.as_deref()) else { return Ok(unknown()) };
            if !ask.unknown.is_empty() {
                return Ok(unknown());
            }
            let end = char_index(before.strip_suffix('\n').unwrap_or(before), window.end);
            match only_after(before, now, end) {
                Some(after) => then &= after,
                None => return Ok(unknown()),
            }
        }
        let under = if then { vec![json!(window.story.0)] } else { vec![] };
        let covered = asks
            .iter()
            .map(|ask| Covered { id: ask.id.clone(), covered: Some(then && numeric_id(&ask.id) == story), ..Default::default() })
            .collect();
        Ok(CoveredAt { under, covered })
    }
}

#[cfg(test)]
mod tests {
    use super::only_after;

    #[test]
    fn changes_are_placed_at_their_earliest_possible_start() {
        assert_eq!(only_after("0123456789\n", "012345678Late9\n", 6), Some(true));
        assert_eq!(only_after("0123456789\n", "012345X6789\n", 6), Some(true));
        assert_eq!(only_after("0123456789\n", "01234X6789\n", 6), Some(false));
        assert_eq!(only_after("aaaaaaaaaa\n", "aaaaaaaaaaa\n", 6), Some(false));
        assert_eq!(only_after("0123456789\n", "0123456789\n", 6), None);
    }
}
