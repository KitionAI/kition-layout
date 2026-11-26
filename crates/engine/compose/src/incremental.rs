//! Incremental composition: an edit composes the story again from the paragraph it changed, and
//! stops as soon as the layout is back in step with the previous composition.
//!
//! A composition of a story that qualifies ([`eligible`]) leaves a [`Trace`]: for each paragraph,
//! the composer's state when it first reached the paragraph (cursor, list counter, style table
//! length, lines and decorations so far) and whether that first layout of the paragraph was its
//! final one (`clean`: no keep option re-laid it or an earlier paragraph afterwards). The next
//! composition of the story, given the previous one and its trace ([`Prev`]):
//!
//! - **resumes** at the last clean paragraph before the first changed one whose frames are all
//!   unchanged: everything before it is copied, not composed;
//! - **converges** at the first paragraph from which the rest of the story is unchanged and the
//!   composer is in exactly the state it was in there before (same frame, column, baseline,
//!   pending space, list counter, style table, keep chain, and the same frames from here on):
//!   the composer is deterministic, so the rest of the old composition is copied, moved by the
//!   byte and paragraph count the edit added.
//!
//! The result is the same, value for value, as composing the whole story again (tests compare the
//! two over a series of edits). Stories with features whose layout reaches across paragraphs in
//! ways the trace does not record (tables, footnotes, anchored objects, variables, spanning or
//! split columns, named lists, shading, vertical justification) are always composed in full.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use kition_layout_doc::{CharFormat, Document, ListType, SpanColumns, Story, VerticalJustification};

use crate::{Band, ComposeOptions, ComposedStory, Cursor, FrameSpec, ParaInfo, RunStyle, Snapshot};

thread_local! {
    /// Paragraphs this thread has laid out (each time the composer starts one).
    static LAID: Cell<u64> = const { Cell::new(0) };
}

/// How many paragraphs the calling thread has laid out so far: tests bound the work an edit costs
/// with it (a typed character lays out the paragraphs it changed), not with a clock.
pub fn paragraphs_laid_on_this_thread() -> u64 {
    LAID.with(Cell::get)
}

pub(crate) fn count_laid() {
    LAID.with(|n| n.set(n.get().saturating_add(1)));
}

/// The composer's state when it first reached a paragraph (see the module docs).
#[derive(Clone, Debug)]
pub(crate) struct Mark {
    fi: usize,
    col: usize,
    last_baseline: Option<f64>,
    last_descent: f64,
    last_reference: f64,
    pending: f64,
    band_para: usize,
    band_line0: usize,
    list_counter: u32,
    lines: usize,
    decos: usize,
    styles: usize,
    restores: usize,
}

impl Mark {
    /// `None` for state the trace does not record (balancing bands, split columns).
    pub(crate) fn of(s: &Snapshot) -> Option<Mark> {
        let c = &s.cur;
        if c.band.above.is_some() || c.split.is_some() || !c.limits.is_empty() || !c.split_limits.is_empty() || s.tables > 0 {
            return None;
        }
        Some(Mark {
            fi: c.fi,
            col: c.col,
            last_baseline: c.last_baseline,
            last_descent: c.last_descent,
            last_reference: c.last_reference,
            pending: c.pending,
            band_para: c.band.para,
            band_line0: c.band.line0,
            list_counter: s.list_counter,
            lines: s.lines,
            decos: s.decos,
            styles: s.styles,
            restores: s.restores,
        })
    }

    pub(crate) fn snapshot(&self, pi: usize) -> Snapshot {
        Snapshot {
            cur: self.cursor(pi),
            list_counter: self.list_counter,
            lines: self.lines,
            decos: self.decos,
            tables: 0,
            styles: self.styles,
            restores: self.restores,
        }
    }

    pub(crate) fn cursor(&self, pi: usize) -> Cursor {
        Cursor {
            fi: self.fi,
            col: self.col,
            last_baseline: self.last_baseline,
            last_descent: self.last_descent,
            last_reference: self.last_reference,
            pending: self.pending,
            pi,
            band: Band { para: self.band_para, line0: self.band_line0, above: None },
            limits: Rc::default(),
            split: None,
            split_limits: Rc::default(),
        }
    }

    pub(crate) fn fi(&self) -> usize {
        self.fi
    }

    pub(crate) fn lines(&self) -> usize {
        self.lines
    }

    pub(crate) fn decos(&self) -> usize {
        self.decos
    }

    pub(crate) fn styles(&self) -> usize {
        self.styles
    }

    /// The same mark for a paragraph `dp` places on, in frame `fi` with `dl` more lines and `dd`
    /// more decorations there, after `dr` more keep re-lays.
    fn moved(&self, dp: isize, at: usize, dl: isize, dd: isize, dr: isize) -> Mark {
        let here = self.fi == at;
        Mark {
            band_para: self.band_para.saturating_add_signed(dp),
            lines: if here { self.lines.saturating_add_signed(dl) } else { self.lines },
            decos: if here { self.decos.saturating_add_signed(dd) } else { self.decos },
            restores: self.restores.saturating_add_signed(dr),
            ..self.clone()
        }
    }
}

/// What a composition leaves for the next one of the same story (see the module docs).
#[derive(Clone, Debug, Default)]
pub struct Trace {
    pub(crate) ranges: Vec<Range<usize>>,
    /// Where each character run starts (and the text's end).
    pub(crate) runs: Vec<usize>,
    pub(crate) marks: Vec<Mark>,
    pub(crate) clean: Vec<bool>,
    pub(crate) info: Vec<ParaInfo>,
    pub(crate) force_col: Vec<bool>,
    pub(crate) line_cap: Vec<Option<usize>>,
    pub(crate) restores: usize,
    /// Per frame, the decorations before the column rules (added after the paragraphs).
    pub(crate) rules_at: Vec<usize>,
    pub(crate) specs: Vec<FrameSpec>,
}

/// The previous composition of a story, with the story it was composed from and its trace.
#[derive(Clone, Copy)]
pub struct Prev<'a> {
    pub story: &'a Story,
    pub out: &'a ComposedStory,
    pub trace: &'a Trace,
}

/// Whether compositions of `story` leave a trace (and can reuse one).
pub(crate) fn eligible(doc: &Document, story: &Story, frames: &[FrameSpec], opts: &ComposeOptions) -> bool {
    if opts.page_name.is_some() || opts.page.is_some() || opts.running.is_some() || opts.label.is_some() || opts.xrefs.is_some() {
        return false;
    }
    if !story.tables.is_empty() || !story.notes.is_empty() || !story.endnotes.is_empty() || !story.xrefs.is_empty() || !story.objects.is_empty() {
        return false;
    }
    if crate::vars::has_vars(story) || frames.iter().any(|f| f.opts.vertical_justification != VerticalJustification::Top) {
        return false;
    }
    let mut last: Option<(&kition_layout_doc::ParaFormat, bool)> = None;
    story.paras.iter().all(|p| {
        if let Some((q, ok)) = last
            && q == p
        {
            return ok;
        }
        let (pp, _) = doc.styles.resolve_para(p);
        let ok = pp.span_columns == SpanColumns::Single && !pp.shading_on && !(pp.list_type == ListType::Numbers && !pp.list_name.is_empty());
        last = Some((p, ok));
        ok
    })
}

/// Where each character run of `story` starts, and its text's end.
pub(crate) fn run_starts(story: &Story) -> Vec<usize> {
    let mut v = Vec::with_capacity(story.chars.len() + 1);
    let mut at = 0usize;
    for r in &story.chars {
        v.push(at);
        at = at.saturating_add(r.len);
    }
    v.push(at);
    v
}

/// The character runs over `r` (clipped, empty pieces left out).
fn runs_over<'a>(story: &'a Story, starts: &'a [usize], r: Range<usize>) -> impl Iterator<Item = (usize, &'a CharFormat)> + 'a {
    let first = starts.partition_point(|&s| s <= r.start).saturating_sub(1);
    story.chars.iter().zip(starts.iter().zip(starts.iter().skip(1))).skip(first).take_while(move |(_, (s, _))| **s < r.end).filter_map(
        move |(run, (s, e))| {
            let len = (*e).min(r.end).saturating_sub((*s).max(r.start));
            (len > 0).then_some((len, &run.format))
        },
    )
}

/// A paragraph's text, format and character runs (with the separator after it) are the same.
fn same_para(a: (&Story, &[usize], &[Range<usize>]), ai: usize, b: (&Story, &[usize], &[Range<usize>]), bi: usize) -> bool {
    let (Some(ar), Some(br)) = (a.2.get(ai), b.2.get(bi)) else { return false };
    if a.0.paras.get(ai) != b.0.paras.get(bi) || a.0.text.get(ar.clone()) != b.0.text.get(br.clone()) {
        return false;
    }
    let with_sep = |s: &Story, r: &Range<usize>| r.start..(r.end + 1).min(s.text.len());
    runs_over(a.0, a.1, with_sep(a.0, ar)).eq(runs_over(b.0, b.1, with_sep(b.0, br)))
}

/// Where a composition can start and stop reusing the previous one.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plan {
    /// Paragraph to compose from (everything before it is copied).
    pub resume: usize,
    /// From this paragraph on, the story is the previous one's, `dp` paragraphs further on.
    pub suffix_from: usize,
    pub dp: isize,
    /// From this frame on, the frames are the previous ones (`usize::MAX`: the thread changed).
    pub specs_from: usize,
}

pub(crate) fn plan(p: &Prev<'_>, story: &Story, runs: &[usize], ranges: &[Range<usize>], frames: &[FrameSpec]) -> Option<Plan> {
    if p.story.id != story.id || p.story.direction != story.direction || p.story.vertical != story.vertical {
        return None;
    }
    let t = p.trace;
    let old = (p.story, t.runs.as_slice(), t.ranges.as_slice());
    let new = (story, runs, ranges);
    let (on, nn) = (t.ranges.len(), ranges.len());
    let common = on.min(nn);
    let prefix = (0..common).find(|&i| !same_para(old, i, new, i)).unwrap_or(common);
    let suffix = (0..common - prefix).find(|&k| !same_para(old, on - 1 - k, new, nn - 1 - k)).unwrap_or(common - prefix);
    // Frames: the first that changed, and from where all the rest are the same.
    let first_diff = t.specs.iter().zip(frames).position(|(a, b)| a != b).unwrap_or(t.specs.len().min(frames.len()));
    let specs_from =
        if t.specs.len() != frames.len() { usize::MAX } else { t.specs.iter().zip(frames).rposition(|(a, b)| a != b).map_or(0, |i| i + 1) };
    // The last clean paragraph at or before the first change, reached in unchanged frames.
    let top = prefix.min(t.marks.len().saturating_sub(1)).min(t.info.len());
    let resume = (0..=top).rev().find(|&k| k == 0 || t.clean.get(k) == Some(&true) && t.marks.get(k).is_some_and(|m| m.fi < first_diff)).unwrap_or(0);
    Some(Plan { resume, suffix_from: nn - suffix, dp: nn as isize - on as isize, specs_from })
}

/// The composer's state at a paragraph, for [`converges`].
pub(crate) struct Here<'a> {
    pub pi: usize,
    pub cur: &'a Cursor,
    pub list_counter: u32,
    pub styles: &'a [RunStyle],
    pub restores: usize,
    pub cap: usize,
    pub info: &'a [ParaInfo],
    pub force_col: &'a [bool],
    pub line_cap: &'a [Option<usize>],
    pub story: &'a Story,
    pub ranges: &'a [Range<usize>],
}

/// The previous composition's paragraph the composer is back in step with at `h.pi`, if any.
pub(crate) fn converges(p: &Prev<'_>, plan: &Plan, h: &Here<'_>) -> Option<usize> {
    if h.pi < plan.suffix_from || h.cur.fi < plan.specs_from {
        return None;
    }
    let t = p.trace;
    let op = h.pi.checked_add_signed(-plan.dp)?;
    let m = t.marks.get(op)?;
    if t.clean.get(op) != Some(&true) {
        return None;
    }
    let c = h.cur;
    let same = |a: f64, b: f64| a.to_bits() == b.to_bits();
    if c.fi != m.fi
        || c.col != m.col
        || c.last_baseline.map(f64::to_bits) != m.last_baseline.map(f64::to_bits)
        || !same(c.last_descent, m.last_descent)
        || !same(c.last_reference, m.last_reference)
        || !same(c.pending, m.pending)
        || c.band.line0 != m.band_line0
        || c.band.above.is_some()
        || c.split.is_some()
        || h.list_counter != m.list_counter
        || h.styles.len() != m.styles
        || p.out.styles.get(..m.styles) != Some(h.styles)
    {
        return None;
    }
    // A break character at the end of the paragraph before moves this one on.
    let last_char = |s: &Story, r: Option<&Range<usize>>| r.and_then(|r| s.text.get(r.clone())).and_then(|x| x.chars().next_back());
    let before_new = h.pi.checked_sub(1).and_then(|j| last_char(h.story, h.ranges.get(j)));
    let before_old = op.checked_sub(1).and_then(|j| last_char(p.story, t.ranges.get(j)));
    if before_new.and_then(crate::break_start) != before_old.and_then(crate::break_start) {
        return None;
    }
    // The keep-with-next chain the keep options look back along.
    let (mut j, mut oj) = (h.pi, op);
    loop {
        match (j.checked_sub(1), oj.checked_sub(1)) {
            (None, None) => break,
            (Some(a), Some(b)) => {
                let (Some(ia), Some(ib)) = (h.info.get(a), t.info.get(b)) else { return None };
                if ia != ib || h.force_col.get(a) != t.force_col.get(b) || h.line_cap.get(a) != t.line_cap.get(b) {
                    return None;
                }
                if ia.keep_with_next == 0 {
                    break;
                }
                (j, oj) = (a, b);
            }
            _ => return None,
        }
    }
    // Keep re-lays are capped: both layouts must stay under the cap.
    let old_cap = (4 * t.ranges.len() + 64).max(1);
    let to_come = t.restores.checked_sub(m.restores)?;
    if t.restores >= old_cap || h.restores.saturating_add(to_come) >= h.cap {
        return None;
    }
    Some(op)
}

/// Copies the previous composition from paragraph `op` on into `out` (the composer is at `pi`,
/// in frame `fi`): its lines and decorations, moved by the bytes and paragraphs the edit added,
/// the rest of its style table and its overset point. Returns the lines and decorations frame `fi`
/// has more than the previous composition had there at `op`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn copy_rest(
    p: &Prev<'_>,
    plan: &Plan,
    op: usize,
    pi: usize,
    fi: usize,
    out: &mut ComposedStory,
    styles: &mut Vec<RunStyle>,
    ranges: &[Range<usize>],
) -> (isize, isize) {
    let t = p.trace;
    let Some(m) = t.marks.get(op) else { return (0, 0) };
    let db = ranges.get(pi).map_or(0, |r| r.start as isize) - t.ranges.get(op).map_or(0, |r| r.start as isize);
    let shift = |b: usize| b.saturating_add_signed(db);
    let (dl, dd) = out.frames.get(fi).map_or((0, 0), |f| (f.lines.len() as isize - m.lines as isize, f.decos.len() as isize - m.decos as isize));
    for (f, ft) in out.frames.iter_mut().enumerate().skip(fi) {
        let Some(src) = p.out.frames.get(f) else { break };
        let (l0, d0) = if f == fi { (m.lines, m.decos) } else { (0, 0) };
        let d1 = t.rules_at.get(f).copied().unwrap_or(src.decos.len()).min(src.decos.len());
        ft.lines.extend(src.lines.get(l0..).unwrap_or_default().iter().map(|l| {
            let mut l = l.clone();
            l.para = l.para.saturating_add_signed(plan.dp);
            l.range = shift(l.range.start)..shift(l.range.end);
            l.keep_violation = false;
            for g in &mut l.glyphs {
                g.byte = shift(g.byte);
            }
            l
        }));
        ft.decos.extend(src.decos.get(d0..d1.max(d0)).unwrap_or_default().iter().cloned());
    }
    styles.extend(p.out.styles.get(m.styles..).unwrap_or_default().iter().cloned());
    out.overset_at = p.out.overset_at.map(shift);
    (dl, dd)
}

/// The trace of a composition that converged at `pi` (previous paragraph `op`, frame `fi`): the
/// composer's own before `pi`, the previous one's after it, moved as [`copy_rest`] moved the lines.
#[allow(clippy::too_many_arguments)]
pub(crate) fn join(p: &Prev<'_>, plan: &Plan, op: usize, pi: usize, fi: usize, dl: isize, dd: isize, own: Trace) -> Trace {
    let t = p.trace;
    let Some(m) = t.marks.get(op) else { return own };
    let dr = own.restores as isize - m.restores as isize;
    let mut tr = own;
    tr.marks.truncate(pi);
    tr.marks.extend(t.marks.get(op..).unwrap_or_default().iter().map(|x| x.moved(plan.dp, fi, dl, dd, dr)));
    tr.clean.truncate(pi);
    tr.clean.extend(t.clean.get(op..).unwrap_or_default().iter().copied());
    tr.info.truncate(pi);
    tr.info.extend(t.info.get(op..).unwrap_or_default().iter().cloned());
    tr.force_col.truncate(pi);
    tr.force_col.extend(t.force_col.get(op..).unwrap_or_default().iter().copied());
    tr.line_cap.truncate(pi);
    tr.line_cap.extend(t.line_cap.get(op..).unwrap_or_default().iter().copied());
    tr.restores = t.restores.saturating_add_signed(dr);
    tr
}
