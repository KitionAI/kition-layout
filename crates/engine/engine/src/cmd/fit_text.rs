//! Object › Fitting › Fit Text to Frame: the text of a frame's story set at the largest size at
//! which it all fits its frames, with no word broken across lines or set past the frame's edge
//! (posters, social images).

use kition_layout_doc::{Content, Document, Leading, StoryId};
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_text_or_frames, ids_param};
use crate::{Result, Session};

/// Point sizes the search stays within.
const MIN_SIZE: f64 = 1.0;
const MAX_SIZE: f64 = 1296.0;
/// Bisection steps (the scale is then within about one part in a million).
const STEPS: usize = 24;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "frame.fitText",
        "Fit Text to Frame",
        ["Object", "Fitting"],
        None,
        "{ids?: text frame ids (default: the selection, or the text being edited), min?: pt (default 1), max?: pt (default 1296)} — scales every point size (and fixed leading) of each frame's story by one factor, the largest at which the story fits its frames without breaking a word → {stories: [{story, scale, size}]}",
        has_text_or_frames,
        fit_text
    )]
}

fn fit_text(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "frame.fitText";
    let min = p.get("min").and_then(Value::as_f64).unwrap_or(MIN_SIZE).clamp(0.1, MAX_SIZE);
    let max = p.get("max").and_then(Value::as_f64).unwrap_or(MAX_SIZE).clamp(min, MAX_SIZE);
    let st = s.doc()?;
    let d = st.doc.clone();
    let mut stories: Vec<StoryId> = Vec::new();
    match ids_param(p, "ids") {
        Some(ids) => {
            for id in ids {
                let it = d.item(id).ok_or_else(|| bad(ID, format!("no object with id {}", id.0)))?;
                let Content::Text(t) = &it.content else { return Err(bad(ID, format!("{} is not a text frame", id.0))) };
                stories.push(t.story);
            }
        }
        None => {
            if let Some(ts) = &st.selection.text {
                stories.push(ts.story);
            }
            stories.extend(st.selection.items.iter().filter_map(|id| d.item(*id).and_then(|it| it.text_frame()).map(|t| t.story)));
        }
    }
    stories.sort_by_key(|s| s.0);
    stories.dedup();
    if stories.is_empty() {
        return Err(super::refused(ID, "needTextFrame", "select a text frame"));
    }
    let mut found = Vec::new();
    for sid in stories {
        let story = d.story(sid).ok_or_else(|| bad(ID, "the frame has no story"))?;
        if story.text.is_empty() {
            continue;
        }
        let (small, large) = sizes(&d, sid);
        if small <= 0.0 || large <= 0.0 {
            continue;
        }
        // The scale may take the smallest size down to `min` and the largest up to `max`.
        let (lo_limit, hi_limit) = (min / small, max / large);
        if lo_limit > hi_limit {
            continue;
        }
        let mut probe = 0u64;
        let mut fits = |k: f64| {
            probe += 1;
            let candidate = scaled(&d, sid, k, probe);
            fits_frames(s, &candidate, sid)
        };
        let k = if fits(1.0f64.clamp(lo_limit, hi_limit)) {
            // Grow until it no longer fits, then bisect.
            let (mut lo, mut hi) = (1.0f64.clamp(lo_limit, hi_limit), hi_limit);
            let mut step = lo;
            while step < hi_limit {
                step = (step * 2.0).min(hi_limit);
                if fits(step) {
                    lo = step;
                } else {
                    hi = step;
                    break;
                }
            }
            bisect(&mut fits, lo, hi)
        } else if fits(lo_limit) {
            bisect(&mut fits, lo_limit, 1.0f64.clamp(lo_limit, hi_limit))
        } else {
            // Even at the smallest size the text does not fit: set it at the smallest.
            lo_limit
        };
        found.push((sid, k, large * k));
    }
    if found.is_empty() {
        return Ok(json!({ "stories": [] }));
    }
    let report: Vec<Value> = found.iter().map(|(sid, k, size)| json!({ "story": sid.0, "scale": k, "size": (size * 10.0).floor() / 10.0 })).collect();
    s.edit(|d, _| {
        for (sid, k, _) in &found {
            scale_story(d, *sid, *k);
        }
        Ok(())
    })?;
    Ok(json!({ "stories": report }))
}

/// The largest `k` in `[lo, hi]` that fits, given that `lo` fits.
fn bisect(fits: &mut impl FnMut(f64) -> bool, mut lo: f64, mut hi: f64) -> f64 {
    for _ in 0..STEPS {
        if hi - lo <= lo * 1e-6 {
            break;
        }
        let mid = (lo + hi) / 2.0;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// The smallest and largest resolved point sizes in a story.
fn sizes(d: &Document, sid: StoryId) -> (f64, f64) {
    let Some(story) = d.story(sid) else { return (0.0, 0.0) };
    let (mut small, mut large) = (f64::INFINITY, 0.0f64);
    let mut start = 0;
    for run in &story.chars {
        let at = start;
        start += run.len;
        if run.len == 0 {
            continue;
        }
        let Some(pf) = story.paras.get(story.para_at(at)) else { continue };
        let (_, pc) = d.styles.resolve_para(pf);
        let size = d.styles.resolve_char(&pc, &run.format).size;
        if size.is_finite() && size > 0.0 {
            small = small.min(size);
            large = large.max(size);
        }
    }
    if large == 0.0 { (0.0, 0.0) } else { (small, large) }
}

/// Every point size and fixed leading of story `sid` times `k`, as local formatting.
fn scale_story(d: &mut Document, sid: StoryId, k: f64) {
    let styles = d.styles.clone();
    let Some(story) = d.story_mut(sid) else { return };
    let ranges = story.para_ranges();
    for (pi, r) in ranges.into_iter().enumerate() {
        let Some(pf) = story.paras.get(pi) else { continue };
        let (_, pc) = styles.resolve_para(pf);
        // The paragraph's separator takes the size of the text before it.
        let end = (r.end + 1).min(story.text.len());
        story.format_chars(r.start..end, |f| {
            let cp = styles.resolve_char(&pc, f);
            f.over.size = Some(((cp.size * k) * 1000.0).round() / 1000.0);
            if let Leading::Points(v) = cp.leading {
                f.over.leading = Some(Leading::Points(((v * k) * 1000.0).round() / 1000.0));
            }
        });
    }
}

/// A copy of `d` with story `sid` scaled by `k`; `probe` gives each copy its own revision so the
/// composition cache never answers one copy with another's lines.
fn scaled(d: &Document, sid: StoryId, k: f64, probe: u64) -> Document {
    let mut copy = d.clone();
    let base = d.story(sid).map_or(0, |st| st.rev);
    scale_story(&mut copy, sid, k);
    if let Some(st) = copy.story_mut(sid) {
        st.rev = base.wrapping_add(1 << 40).wrapping_add(probe);
    }
    copy
}

/// Whether story `sid` fits its frames in `d`: nothing overset, no word broken across lines.
fn fits_frames(s: &Session, d: &Document, sid: StoryId) -> bool {
    let cs = s.cache.get(d, sid, None);
    if cs.is_overset() {
        return false;
    }
    let Some(story) = d.story(sid) else { return false };
    let text = story.text.as_str();
    let breakable = |c: char| c.is_whitespace() || c == '-' || c == '\u{2014}' || c == '\u{2013}' || kition_layout_compose::upright_in_vertical(c);
    cs.frames.iter().flat_map(|f| &f.lines).all(|line| {
        // A word wider than the frame is set past its edge rather than overset.
        if line.end_x > line.x1 + 0.5 {
            return false;
        }
        if line.last_in_para || line.range.is_empty() {
            return true;
        }
        if line.hyphenated {
            return false;
        }
        let before = text.get(..line.range.end).and_then(|t| t.chars().next_back());
        let after = text.get(line.range.end..).and_then(|t| t.chars().next());
        before.is_none_or(breakable) || after.is_none_or(breakable)
    })
}

#[cfg(test)]
mod tests {
    use kition_layout_doc::{ItemId, StoryId};
    use serde_json::json;

    use crate::Session;

    fn frame(s: &mut Session, rect: [f64; 4], text: &str) -> (u64, StoryId) {
        let r = s.execute("frame.add", &json!({"rect": rect, "content": "text", "text": text, "caret": false})).unwrap();
        (r["id"].as_u64().unwrap(), StoryId(r["story"].as_u64().unwrap()))
    }

    fn size(s: &Session, sid: StoryId) -> f64 {
        let d = &s.doc().unwrap().doc;
        let story = d.story(sid).unwrap();
        let (_, pc) = d.styles.resolve_para(&story.paras[0]);
        d.styles.resolve_char(&pc, &story.chars[0].format).size
    }

    fn overset(s: &Session, sid: StoryId) -> bool {
        let st = s.doc().unwrap();
        s.cache.get(&st.doc, sid, None).is_overset()
    }

    /// An agent names the frame (`ids`) with nothing selected, as a check's fix does: it runs.
    #[test]
    fn named_frames_need_no_selection() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 1080, "height": 1440, "pages": 1, "facingPages": false})).unwrap();
        let (long, a) = frame(&mut s, [80.0, 80.0, 300.0, 120.0], &"Overset words that do not fit. ".repeat(30));
        s.execute("edit.deselectAll", &json!({})).ok();
        s.doc_mut().unwrap().selection = Default::default();
        assert!(overset(&s, a));
        s.execute("frame.fitText", &json!({"ids": [long]})).unwrap();
        assert!(!overset(&s, a));
    }

    #[test]
    fn grows_short_text_and_shrinks_long_text_to_fit() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 1080, "height": 1440, "pages": 1, "facingPages": false})).unwrap();
        let (small, a) = frame(&mut s, [80.0, 80.0, 1000.0, 600.0], "Make something matter");
        let r = s.execute("frame.fitText", &json!({"ids": [small]})).unwrap();
        let grown = size(&s, a);
        assert!(grown > 60.0, "{r}");
        assert!(!overset(&s, a));
        // A little larger no longer fits.
        let mut probe = Session::new();
        probe.execute("file.new", &json!({"width": 1080, "height": 1440, "pages": 1, "facingPages": false})).unwrap();
        let (p, b) = frame(&mut probe, [80.0, 80.0, 1000.0, 600.0], "Make something matter");
        probe.execute("select.set", &json!({"ids": [p]})).unwrap();
        probe.execute("text.char", &json!({"size": grown * 1.05})).unwrap();
        let crowded =
            overset(&probe, b) || probe.execute("frame.fitText", &json!({"ids": [p]})).unwrap()["stories"][0]["scale"].as_f64().unwrap() < 1.0;
        assert!(crowded, "the fitted size is the largest that fits");

        let long = "Words keep coming and the frame fills up quickly with them. ".repeat(20);
        let (big, c) = frame(&mut s, [80.0, 700.0, 400.0, 800.0], &long);
        assert!(overset(&s, c));
        s.execute("select.set", &json!({"ids": [big]})).unwrap();
        s.execute("frame.fitText", &json!({})).unwrap();
        assert!(!overset(&s, c));
        assert!(size(&s, c) < 12.0);
        // One undo step.
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(overset(&s, c));
    }

    #[test]
    fn never_breaks_a_word_to_fit() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 600, "height": 600, "pages": 1, "facingPages": false})).unwrap();
        let (f, sid) = frame(&mut s, [50.0, 50.0, 250.0, 550.0], "Supercalifragilistic");
        s.execute("frame.fitText", &json!({"ids": [f]})).unwrap();
        let st = s.doc().unwrap();
        let cs = s.cache.get(&st.doc, sid, None);
        assert_eq!(cs.line_count(), 1, "one word stays on one line");
        let line = &cs.frames[0].lines[0];
        assert!(line.end_x <= line.x1 + 0.5, "{} > {}", line.end_x, line.x1);
        // And it was made larger than the 12 pt it started at.
        assert!(st.doc.item(ItemId(f)).unwrap().bounds().width() > 0.0);
        let story = st.doc.story(sid).unwrap();
        let (_, pc) = st.doc.styles.resolve_para(&story.paras[0]);
        assert!(st.doc.styles.resolve_char(&pc, &story.chars[0].format).size > 12.0);
    }

    #[test]
    fn bad_input_is_an_error_and_empty_text_is_left_alone() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        let g = s.execute("frame.add", &json!({"rect": [10, 10, 100, 100], "content": "graphic"})).unwrap()["id"].as_u64().unwrap();
        assert!(s.execute("frame.fitText", &json!({"ids": [g]})).is_err());
        assert!(s.execute("frame.fitText", &json!({"ids": [999_999]})).is_err());
        let (e, _) = frame(&mut s, [10.0, 200.0, 100.0, 300.0], "");
        assert_eq!(s.execute("frame.fitText", &json!({"ids": [e]})).unwrap()["stories"], json!([]));
    }
}
