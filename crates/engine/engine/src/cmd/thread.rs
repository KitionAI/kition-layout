//! Threading text frames by hand: `frame.thread`, `frame.unthread`, and `frame.loadText` (the
//! loaded text cursor the Selection tool's port click starts).

use kition_layout_doc::{Content, Document, ItemId, ParaFormat, Selection, SpreadRef, StoryId, TextFrame, TextFrameOptions};
use kition_layout_geom::Rect;
use kition_layout_tools::thread::{LoadedTextTool, Side};
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc, id_param, rect_param, spread_param, str_param};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "frame.thread",
            "Thread Text Frames",
            [],
            None,
            "{from: text frame id, to?: frame id (an empty or text frame), spread?, rect?: [x0,y0,x1,y1] (a new text frame there, spread coords), page?: n (1-based: a new text frame on the page's margins), side?: out|in (default out: the frame follows `from`; in: it goes before `from`), keepLoaded?: bool (load the text cursor again from the frame), autoflow?: bool (Shift-click: then add pages with frames on their margins, threaded, until the story fits)} — continue a story in another frame; a text frame's own text joins the story",
            has_doc,
            thread
        ),
        cmd!(
            "frame.unthread",
            "Break Thread",
            [],
            None,
            "{frame: text frame id, side?: out|in (default out: break after the frame; in: before it)} — the frames after the break become an empty story; the text stays and may overset",
            has_doc,
            unthread
        ),
        cmd!(
            noundo "view.showTextThreads",
            "Show Text Threads",
            ["View", "Extras"],
            Some("Cmd+Alt+Y"),
            "{on?: bool (default: toggle)} — lines between the threaded frames of the selected frames' stories → {on}",
            has_doc,
            show_threads
        ),
        cmd!(
            noundo "frame.loadText",
            "Load Text Cursor",
            [],
            None,
            "{frame: text frame id, side?: out|in (default out)} — load the text cursor from a frame's out or in port (the editor's port click); a click with the loadedText tool then threads",
            has_doc,
            load_text
        ),
    ]
}

fn side_param(cmd: &str, p: &Value) -> Result<Side> {
    match str_param(p, "side") {
        None => Ok(Side::Out),
        Some(s) => Side::parse(s).ok_or_else(|| bad(cmd, format!("unknown side `{s}` (out, in)"))),
    }
}

/// The story of text frame `id`.
fn story_of(d: &Document, cmd: &str, key: &str, id: ItemId) -> Result<StoryId> {
    let it = d.item(id).ok_or_else(|| bad(cmd, format!("no object with id {} (`{key}`)", id.0)))?;
    it.text_frame().map(|t| t.story).ok_or_else(|| bad(cmd, format!("`{key}` ({}) is not a text frame", id.0)))
}

/// Puts frame `id` first in `story`'s thread when it holds no text of its own (an empty frame, or
/// a text frame alone in an empty story, which goes); false, changing nothing, otherwise.
fn prepend(d: &mut Document, id: ItemId, story: StoryId) -> Result<bool> {
    let it = d.item(id).ok_or(kition_layout_doc::DocError::NoItem(id))?;
    let own = match &it.content {
        Content::Unassigned => None,
        Content::Text(t) => match d.story(t.story) {
            Some(st) if st.text.is_empty() && st.frames == [id] => Some(t.story),
            _ => return Ok(false),
        },
        _ => return Ok(false),
    };
    if let Some(own) = own {
        d.stories.remove(&own);
    }
    let options = match &d.item(id).map(|i| &i.content) {
        Some(Content::Text(t)) => t.options.clone(),
        _ => TextFrameOptions::default(),
    };
    let it = d.item_mut(id).ok_or(kition_layout_doc::DocError::NoItem(id))?;
    it.content = Content::Text(TextFrame { story, options });
    let st = d.story_mut(story).ok_or(kition_layout_doc::DocError::NoStory(story))?;
    st.frames.insert(0, id);
    st.rev += 1;
    Ok(true)
}

/// Unloads the text cursor when it is loaded.
fn unload(s: &mut Session) {
    if s.tool_id() == "loadedText" {
        s.set_tool("selection");
    }
}

fn overset(s: &Session, frame: ItemId) -> bool {
    let Ok(st) = s.doc() else { return false };
    let Some(sid) = st.doc.item(frame).and_then(|i| i.text_frame()).map(|t| t.story) else { return false };
    s.cache.get(&st.doc, sid, None).is_overset()
}

fn thread(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "frame.thread";
    let from = id_param(p, "from").ok_or_else(|| bad(CMD, "missing `from` (a text frame id)"))?;
    let side = side_param(CMD, p)?;
    let to = id_param(p, "to");
    let rect = rect_param(p, "rect");
    let page = p.get("page").and_then(Value::as_u64);
    // A new frame goes on the given spread, else on the source frame's.
    let spread = match p.get("spread") {
        None => s.doc()?.doc.find(from).map(|at| at.spread).ok_or_else(|| bad(CMD, format!("no object with id {} (`from`)", from.0)))?,
        Some(_) => spread_param(s, p, "spread")?,
    };
    if to.is_none() && rect.is_none() && page.is_none() {
        return Err(bad(CMD, "give `to` (a frame), `rect` (a new frame), or `page` (a new frame on the page)"));
    }
    let keep = p.get("keepLoaded").and_then(Value::as_bool).unwrap_or(false);
    let flow = p.get("autoflow").and_then(Value::as_bool).unwrap_or(false);
    let layer = s.doc()?.active_layer;
    let target = s.edit(|d, sel| {
        let story = story_of(d, CMD, "from", from)?;
        let target = match (to, rect, page) {
            (Some(to), _, _) => {
                if to == from {
                    return Err(bad(CMD, "a frame cannot be threaded to itself"));
                }
                let it = d.item(to).ok_or_else(|| bad(CMD, format!("no object with id {} (`to`)", to.0)))?;
                match &it.content {
                    Content::Unassigned => {}
                    Content::Text(t) if t.story == story => return Err(bad(CMD, "the frames are already in the same story")),
                    Content::Text(_) => {}
                    _ => return Err(bad(CMD, "`to` must be an empty frame or a text frame")),
                }
                to
            }
            (None, Some(r), _) => {
                let r = Rect::new(r.x0.min(r.x1), r.y0.min(r.y1), r.x0.max(r.x1), r.y0.max(r.y1));
                if !(r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite()) || r.width() < 1.0 || r.height() < 1.0 {
                    return Err(bad(CMD, "`rect` must be at least 1 × 1 pt"));
                }
                if d.spread(spread).is_none() {
                    return Err(bad(CMD, "no such spread"));
                }
                new_frame(d, spread, r, layer)?
            }
            (None, None, Some(n)) => {
                let abs = usize::try_from(n).ok().and_then(|n| n.checked_sub(1)).ok_or_else(|| bad(CMD, "`page` counts from 1"))?;
                let (si, pi) = d.page_loc(abs).ok_or_else(|| bad(CMD, format!("no page {n}")))?;
                let r = d
                    .spreads
                    .get(si)
                    .and_then(|sp| sp.pages.get(pi))
                    .map(kition_layout_doc::Page::margin_rect)
                    .ok_or_else(|| bad(CMD, format!("no page {n}")))?;
                new_frame(d, SpreadRef::Doc(si), r, layer)?
            }
            (None, None, None) => return Err(bad(CMD, "nothing to thread to")),
        };
        match side {
            Side::Out => d.thread(from, target)?,
            Side::In => match d.prev_frame(from) {
                // Between the frame before and this one.
                Some(prev) => d.thread(prev, target)?,
                // A new first frame: an empty one simply starts the story; one with its own
                // text puts it first and this frame's story follows.
                None => {
                    if !prepend(d, target, story)? {
                        d.thread(target, from)?;
                    }
                }
            },
        }
        // Autoflow: pages and frames after the last frame until the whole story fits.
        let added = if flow { super::place_text::autoflow(d, story, AUTOFLOW_PAGES)? } else { 0 };
        *sel = Selection { items: vec![target], ..Default::default() };
        Ok((target, added))
    })?;
    let (target, added) = target;
    unload(s);
    if keep {
        s.tool = Box::new(LoadedTextTool::new(target, Side::Out));
    }
    let st = s.doc()?;
    let story = st.doc.item(target).and_then(|i| i.text_frame()).map(|t| t.story.0);
    let frames: Vec<u64> = story.and_then(|sid| st.doc.story(StoryId(sid))).map(|st| st.frames.iter().map(|f| f.0).collect()).unwrap_or_default();
    Ok(json!({ "frame": target.0, "story": story, "frames": frames, "overset": overset(s, target), "pagesAdded": added }))
}

/// The most pages one autoflow adds.
const AUTOFLOW_PAGES: usize = 2000;

/// A new empty text frame at `r` on spread `sr`, in the default paragraph style.
fn new_frame(d: &mut Document, sr: SpreadRef, r: Rect, layer: kition_layout_doc::LayerId) -> Result<ItemId> {
    let para = ParaFormat { style: d.styles.default_paragraph.clone(), ..Default::default() };
    let (id, _) = d.add_text_frame(sr, r, layer, "", para)?;
    Ok(id)
}

fn unthread(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "frame.unthread";
    let frame = id_param(p, "frame").ok_or_else(|| bad(CMD, "missing `frame` (a text frame id)"))?;
    let side = side_param(CMD, p)?;
    let d = &s.doc()?.doc;
    story_of(d, CMD, "frame", frame)?;
    let after = match side {
        Side::Out => d.next_frame(frame).map(|_| frame),
        Side::In => d.prev_frame(frame),
    };
    unload(s);
    let Some(after) = after else { return Ok(json!({ "unthreaded": false })) };
    s.edit(|d, _| {
        d.unthread_after(after)?;
        Ok(())
    })?;
    let st = s.doc()?;
    let story = |id: ItemId| st.doc.item(id).and_then(|i| i.text_frame()).map(|t| t.story.0);
    Ok(json!({ "unthreaded": true, "story": story(after), "overset": overset(s, after) }))
}

fn show_threads(s: &mut Session, p: &Value) -> Result<Value> {
    let on = p.get("on").and_then(Value::as_bool).unwrap_or(!s.prefs.show_text_threads);
    s.prefs.show_text_threads = on;
    // A view change: the editor draws the overlays again.
    s.doc_mut()?.revision += 1;
    Ok(json!({ "on": on }))
}

fn load_text(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "frame.loadText";
    let frame = id_param(p, "frame").ok_or_else(|| bad(CMD, "missing `frame` (a text frame id)"))?;
    let side = side_param(CMD, p)?;
    story_of(&s.doc()?.doc, CMD, "frame", frame)?;
    s.doc_mut()?.selection = Selection { items: vec![frame], ..Default::default() };
    s.tool = Box::new(LoadedTextTool::new(frame, side));
    Ok(json!({ "frame": frame.0, "side": side.as_str(), "overset": overset(s, frame) }))
}

#[cfg(test)]
mod tests {
    use kition_layout_doc::{Content, ItemId, StoryId};
    use kition_layout_tools::{PointerEvent, PointerKind, ToolKey};
    use serde_json::json;

    use crate::{Session, ViewInfo};

    const LONG: &str = "Words keep coming and the frame fills up quickly with them. ";

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"pages": 2, "facingPages": false})).unwrap();
        s
    }

    fn text_frame(s: &mut Session, rect: [f64; 4], text: &str) -> u64 {
        s.execute("frame.add", &json!({"rect": rect, "content": "text", "text": text, "caret": false})).unwrap()["id"].as_u64().unwrap()
    }

    fn story(s: &Session, frame: u64) -> (StoryId, Vec<u64>, String) {
        let d = &s.doc().unwrap().doc;
        let sid = d.item(ItemId(frame)).unwrap().text_frame().unwrap().story;
        let st = d.story(sid).unwrap();
        (sid, st.frames.iter().map(|f| f.0).collect(), st.text.clone())
    }

    /// A frame holding a placed image (it cannot take text).
    fn image_frame(s: &mut Session, rect: [f64; 4]) -> u64 {
        let f = s.execute("frame.add", &json!({"rect": rect, "content": "graphic"})).unwrap()["id"].as_u64().unwrap();
        let png = kition_layout_render::Rendered { width: 4, height: 4, pixels: vec![200; 4 * 4 * 4] }.to_png();
        s.execute("place.load", &json!({"base64": crate::cmd::base64_encode(&png), "name": "a.png"})).unwrap();
        s.execute("place.drop", &json!({"frame": f})).unwrap();
        assert!(matches!(s.doc().unwrap().doc.item(ItemId(f)).unwrap().content, Content::Graphic(_)));
        f
    }

    fn overset(s: &Session, frame: u64) -> bool {
        super::overset(s, ItemId(frame))
    }

    #[test]
    fn threads_an_empty_frame_and_undoes_it_in_one_step() {
        let mut s = session();
        let a = text_frame(&mut s, [36.0, 36.0, 200.0, 120.0], &LONG.repeat(12));
        let b = s.execute("frame.add", &json!({"rect": [36, 200, 200, 600], "content": "unassigned"})).unwrap()["id"].as_u64().unwrap();
        assert!(overset(&s, a));
        let r = s.execute("frame.thread", &json!({"from": a, "to": b})).unwrap();
        assert_eq!(r["frames"], json!([a, b]));
        assert_eq!(r["overset"], false, "{r}");
        assert!(matches!(s.doc().unwrap().doc.item(ItemId(b)).unwrap().content, Content::Text(_)));
        assert_eq!(s.doc().unwrap().selection.items, vec![ItemId(b)]);
        s.doc().unwrap().doc.check().unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(story(&s, a).1, vec![a]);
        assert!(matches!(s.doc().unwrap().doc.item(ItemId(b)).unwrap().content, Content::Unassigned));
    }

    #[test]
    fn threading_a_text_frame_joins_its_text_to_the_story() {
        let mut s = session();
        let a = text_frame(&mut s, [36.0, 36.0, 200.0, 120.0], "first");
        let b = text_frame(&mut s, [36.0, 200.0, 200.0, 300.0], "second");
        let (sb, ..) = story(&s, b);
        s.execute("frame.thread", &json!({"from": a, "to": b})).unwrap();
        let (_, frames, text) = story(&s, a);
        assert_eq!((frames, text.as_str()), (vec![a, b], "first\nsecond"));
        assert!(s.doc().unwrap().doc.story(sb).is_none(), "the emptied story is gone");
        assert!(s.execute("frame.thread", &json!({"from": a, "to": b})).is_err(), "already in the story");
        assert!(s.execute("frame.thread", &json!({"from": a, "to": a})).is_err());
    }

    #[test]
    fn threads_into_a_new_frame_drawn_or_on_a_page() {
        let mut s = session();
        let a = text_frame(&mut s, [36.0, 36.0, 200.0, 120.0], &LONG.repeat(40));
        let r = s.execute("frame.thread", &json!({"from": a, "rect": [36, 200, 300, 500]})).unwrap();
        let b = r["frame"].as_u64().unwrap();
        let r = s.execute("frame.thread", &json!({"from": b, "page": 2})).unwrap();
        let c = r["frame"].as_u64().unwrap();
        assert_eq!(story(&s, a).1, vec![a, b, c]);
        let d = &s.doc().unwrap().doc;
        assert_eq!(d.page_of_item(ItemId(c)), Some(1), "on page 2");
        let margins = d.spreads[1].pages[0].margin_rect();
        assert_eq!(d.item(ItemId(c)).unwrap().bounds(), margins);
        d.check().unwrap();
        assert!(s.execute("frame.thread", &json!({"from": a, "page": 9})).is_err());
        assert!(s.execute("frame.thread", &json!({"from": a, "page": 0})).is_err());
        assert!(s.execute("frame.thread", &json!({"from": a, "rect": [10, 10, 10, 10]})).is_err());
        assert!(s.execute("frame.thread", &json!({"from": a})).is_err());
    }

    #[test]
    fn threads_before_a_frame_from_its_in_port() {
        let mut s = session();
        let a = text_frame(&mut s, [36.0, 300.0, 200.0, 400.0], "story");
        let b = s.execute("frame.add", &json!({"rect": [36, 36, 200, 120], "content": "unassigned"})).unwrap()["id"].as_u64().unwrap();
        // A new first frame.
        s.execute("frame.thread", &json!({"from": a, "to": b, "side": "in"})).unwrap();
        let (_, frames, text) = story(&s, a);
        assert_eq!((frames, text.as_str()), (vec![b, a], "story"));
        // Between the first frame and this one.
        let c = s.execute("frame.add", &json!({"rect": [300, 36, 500, 120], "content": "unassigned"})).unwrap()["id"].as_u64().unwrap();
        s.execute("frame.thread", &json!({"from": a, "to": c, "side": "in"})).unwrap();
        assert_eq!(story(&s, a).1, vec![b, c, a]);
        s.doc().unwrap().doc.check().unwrap();
        // A text frame threaded in front keeps its text first.
        let t = text_frame(&mut s, [300.0, 300.0, 500.0, 400.0], "lead");
        let solo = text_frame(&mut s, [300.0, 500.0, 500.0, 600.0], "tail");
        s.execute("frame.thread", &json!({"from": solo, "to": t, "side": "in"})).unwrap();
        assert_eq!(story(&s, solo).2, "lead\ntail");
        assert_eq!(story(&s, solo).1, vec![t, solo]);
        // A frame drawn in front of the first one starts the story, with no stray break.
        let r = s.execute("frame.thread", &json!({"from": t, "rect": [36, 500, 200, 600], "side": "in"})).unwrap();
        let first = r["frame"].as_u64().unwrap();
        assert_eq!(story(&s, solo).1, vec![first, t, solo]);
        assert_eq!(story(&s, solo).2, "lead\ntail");
        s.doc().unwrap().doc.check().unwrap();
    }

    #[test]
    fn unthreads_after_or_before_a_frame() {
        let mut s = session();
        let a = text_frame(&mut s, [36.0, 36.0, 200.0, 120.0], &LONG.repeat(20));
        let b = s.execute("frame.thread", &json!({"from": a, "rect": [36, 200, 200, 300]})).unwrap()["frame"].as_u64().unwrap();
        let c = s.execute("frame.thread", &json!({"from": b, "rect": [36, 400, 200, 500]})).unwrap()["frame"].as_u64().unwrap();
        let r = s.execute("frame.unthread", &json!({"frame": c, "side": "in"})).unwrap();
        assert_eq!(r["unthreaded"], true);
        assert_eq!(story(&s, a).1, vec![a, b]);
        assert_eq!(story(&s, c).2, "", "the frames after the break are an empty story");
        s.execute("frame.unthread", &json!({"frame": a})).unwrap();
        assert_eq!(story(&s, a).1, vec![a]);
        assert!(overset(&s, a), "the text stays and oversets");
        s.doc().unwrap().doc.check().unwrap();
        // Nothing to break: no change, no undo step.
        let undo = s.doc().unwrap().undo_steps();
        assert_eq!(s.execute("frame.unthread", &json!({"frame": a})).unwrap()["unthreaded"], false);
        assert_eq!(s.execute("frame.unthread", &json!({"frame": a, "side": "in"})).unwrap()["unthreaded"], false);
        assert_eq!(s.doc().unwrap().undo_steps(), undo);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(story(&s, a).1, vec![a, b]);
    }

    #[test]
    fn bad_parameters_are_errors_not_panics() {
        let mut s = session();
        let a = text_frame(&mut s, [36.0, 36.0, 200.0, 120.0], "x");
        let g = image_frame(&mut s, [36.0, 200.0, 200.0, 300.0]);
        for id in ["frame.thread", "frame.unthread", "frame.loadText"] {
            for p in [
                json!({}),
                json!(null),
                json!([1, 2]),
                json!("x"),
                json!({"from": 999, "frame": 999, "to": 998}),
                json!({"from": a, "frame": a, "side": "up", "to": g}),
            ] {
                assert!(s.execute(id, &p).is_err(), "{id} {p}");
            }
        }
        assert!(s.execute("frame.thread", &json!({"from": g, "to": a})).is_err(), "from a graphic frame");
        assert!(s.execute("frame.thread", &json!({"from": a, "to": g})).is_err(), "into a graphic frame");
        assert!(s.execute("frame.unthread", &json!({"frame": g})).is_err());
        s.doc().unwrap().doc.check().unwrap();
    }

    /// The editor's gesture: click the out port with the Selection tool, then click a frame.
    #[test]
    fn port_click_loads_the_cursor_and_a_click_threads() {
        let mut s = session();
        let a = text_frame(&mut s, [100.0, 100.0, 300.0, 200.0], &LONG.repeat(12));
        let b = s.execute("frame.add", &json!({"rect": [100, 300, 300, 500], "content": "unassigned"})).unwrap()["id"].as_u64().unwrap();
        s.execute("select.set", &json!({"ids": [a]})).unwrap();
        let view = ViewInfo::at_zoom(1.0);
        let off = s.layout().xf(kition_layout_doc::SpreadRef::Doc(0)).translation();
        let at = |x: f64, y: f64, kind| PointerEvent::new(kind, x + off.x, y + off.y);
        // The out port: 12 px above the bottom-right corner.
        s.pointer(&at(300.0, 188.0, PointerKind::Down), view).unwrap();
        s.pointer(&at(300.0, 188.0, PointerKind::Up), view).unwrap();
        assert_eq!(s.tool_id(), "loadedText");
        assert!(s.tool_busy(), "takes Escape");
        s.pointer(&at(200.0, 400.0, PointerKind::Down), view).unwrap();
        s.pointer(&at(200.0, 400.0, PointerKind::Up), view).unwrap();
        assert_eq!(story(&s, a).1, vec![a, b]);
        assert_eq!(s.tool_id(), "selection");
        // Load again from b and draw a frame on page 2.
        s.execute("frame.loadText", &json!({"frame": b})).unwrap();
        let off2 = s.layout().xf(kition_layout_doc::SpreadRef::Doc(1)).translation();
        let at2 = |x: f64, y: f64, kind| PointerEvent::new(kind, x + off2.x, y + off2.y);
        s.pointer(&at2(100.0, 100.0, PointerKind::Down), view).unwrap();
        s.pointer(&at2(250.0, 300.0, PointerKind::Drag), view).unwrap();
        s.pointer(&at2(250.0, 300.0, PointerKind::Up), view).unwrap();
        let (_, frames, _) = story(&s, a);
        assert_eq!(frames.len(), 3);
        let c = frames[2];
        assert_eq!(s.doc().unwrap().doc.page_of_item(ItemId(c)), Some(1));
        // A click on an empty page: a frame the column's width down to the bottom margin.
        s.execute("frame.loadText", &json!({"frame": c})).unwrap();
        s.pointer(&at2(200.0, 400.0, PointerKind::Down), view).unwrap();
        s.pointer(&at2(200.0, 400.0, PointerKind::Up), view).unwrap();
        let (_, frames, _) = story(&s, a);
        assert_eq!(frames.len(), 4);
        let d = &s.doc().unwrap().doc;
        let margins = d.spreads[1].pages[0].margin_rect();
        let r = d.item(ItemId(frames[3])).unwrap().bounds();
        assert_eq!((r.x0, r.y0, r.x1, r.y1), (margins.x0, 400.0, margins.x1, margins.y1));
        // Escape unloads.
        s.execute("frame.loadText", &json!({"frame": c})).unwrap();
        s.tool_key(ToolKey::Escape, Default::default(), view).unwrap();
        assert_eq!(s.tool_id(), "selection");
        s.doc().unwrap().doc.check().unwrap();
    }

    /// Double-clicking a port: the first click loads, the second lands on the frame itself, and
    /// the double-click breaks the thread.
    #[test]
    fn double_clicking_the_out_port_breaks_the_thread() {
        let mut s = session();
        let a = text_frame(&mut s, [100.0, 100.0, 300.0, 200.0], &LONG.repeat(12));
        s.execute("frame.thread", &json!({"from": a, "rect": [100, 300, 300, 500]})).unwrap();
        s.execute("select.set", &json!({"ids": [a]})).unwrap();
        let view = ViewInfo::at_zoom(1.0);
        let off = s.layout().xf(kition_layout_doc::SpreadRef::Doc(0)).translation();
        let at = |kind| PointerEvent::new(kind, 300.0 + off.x, 188.0 + off.y);
        for kind in [PointerKind::Down, PointerKind::Up, PointerKind::Down, PointerKind::Up, PointerKind::DoubleClick] {
            s.pointer(&at(kind), view).unwrap();
        }
        assert_eq!(story(&s, a).1, vec![a]);
        assert_eq!(s.tool_id(), "selection");
        // The Selection tool takes a double-click on a port too.
        let b = s.execute("frame.thread", &json!({"from": a, "rect": [100, 300, 300, 500]})).unwrap()["frame"].as_u64().unwrap();
        s.execute("select.set", &json!({"ids": [a]})).unwrap();
        s.pointer(&at(PointerKind::DoubleClick), view).unwrap();
        assert_eq!(story(&s, a).1, vec![a]);
        assert_ne!(story(&s, b).0, story(&s, a).0);
    }

    /// Alt-click (semi-autoflow): the cursor loads again from the frame just threaded.
    #[test]
    fn alt_click_keeps_the_cursor_loaded() {
        let mut s = session();
        let a = text_frame(&mut s, [100.0, 100.0, 300.0, 200.0], &LONG.repeat(30));
        s.execute("frame.loadText", &json!({"frame": a})).unwrap();
        let view = ViewInfo::at_zoom(1.0);
        let alt = kition_layout_tools::Mods { alt: true, ..Default::default() };
        // A frame down to page 1's bottom margin, then one on page 2.
        for spread in [0, 1] {
            let off = s.layout().xf(kition_layout_doc::SpreadRef::Doc(spread)).translation();
            for kind in [PointerKind::Down, PointerKind::Up] {
                s.pointer(&PointerEvent::new(kind, 200.0 + off.x, 300.0 + off.y).with_mods(alt), view).unwrap();
            }
            assert_eq!(s.tool_id(), "loadedText");
        }
        assert_eq!(story(&s, a).1.len(), 3);
        s.tool_key(ToolKey::Escape, Default::default(), view).unwrap();
        assert_eq!(s.tool_id(), "selection");
    }

    /// Shift-click (autoflow): one click threads a frame, then adds pages with frames until the
    /// story fits, in one undo step.
    #[test]
    fn shift_click_autoflows_until_the_story_fits() {
        let mut s = session();
        let a = text_frame(&mut s, [100.0, 100.0, 300.0, 200.0], &LONG.repeat(160));
        assert!(overset(&s, a));
        let pages = s.doc().unwrap().doc.page_count();
        s.execute("frame.loadText", &json!({"frame": a})).unwrap();
        let view = ViewInfo::at_zoom(1.0);
        let shift = kition_layout_tools::Mods { shift: true, ..Default::default() };
        let off = s.layout().xf(kition_layout_doc::SpreadRef::Doc(0)).translation();
        for kind in [PointerKind::Down, PointerKind::Up] {
            s.pointer(&PointerEvent::new(kind, 200.0 + off.x, 300.0 + off.y).with_mods(shift), view).unwrap();
        }
        assert_eq!(s.tool_id(), "selection", "autoflow unloads the cursor");
        let (_, frames, _) = story(&s, a);
        let last = *frames.last().unwrap();
        assert!(frames.len() > 2, "{frames:?}");
        assert!(!overset(&s, last), "the story fits");
        assert!(s.doc().unwrap().doc.page_count() > pages);
        s.doc().unwrap().doc.check().unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(story(&s, a).1, vec![a]);
        assert_eq!(s.doc().unwrap().doc.page_count(), pages);
        // The agent's form: `autoflow` on `frame.thread`.
        let r = s.execute("frame.thread", &json!({"from": a, "page": 2, "autoflow": true})).unwrap();
        assert_eq!(r["overset"], false, "{r}");
        assert!(r["pagesAdded"].as_u64().unwrap() > 0, "{r}");
    }

    /// View › Show Text Threads: a line from each frame's out port to the next one's in port, for
    /// the selected frame's story only.
    #[test]
    fn show_text_threads_draws_the_selected_story_threads() {
        let mut s = session();
        let a = text_frame(&mut s, [36.0, 36.0, 200.0, 120.0], &LONG.repeat(12));
        s.execute("frame.thread", &json!({"from": a, "rect": [36, 200, 200, 400]})).unwrap();
        let other = text_frame(&mut s, [300.0, 36.0, 500.0, 120.0], "alone");
        let view = ViewInfo::at_zoom(1.0);
        let lines = |s: &mut Session| s.overlays(view).into_iter().filter(|o| matches!(o, kition_layout_tools::Overlay::Line { .. })).count();
        s.execute("select.set", &json!({"ids": [a]})).unwrap();
        assert_eq!(lines(&mut s), 0, "off by default");
        assert_eq!(s.execute("view.showTextThreads", &json!({})).unwrap()["on"], true);
        assert_eq!(lines(&mut s), 1);
        s.execute("select.set", &json!({"ids": [other]})).unwrap();
        assert_eq!(lines(&mut s), 0, "a story in one frame has no thread");
        assert_eq!(s.execute("view.showTextThreads", &json!({"on": false})).unwrap()["on"], false);
    }

    /// A loaded cursor clicked on a graphic frame does nothing (and stays loaded).
    #[test]
    fn a_loaded_cursor_ignores_frames_that_cannot_take_text() {
        let mut s = session();
        let a = text_frame(&mut s, [100.0, 100.0, 300.0, 200.0], "x");
        image_frame(&mut s, [100.0, 300.0, 300.0, 500.0]);
        s.execute("frame.loadText", &json!({"frame": a, "side": "out"})).unwrap();
        let view = ViewInfo::at_zoom(1.0);
        let off = s.layout().xf(kition_layout_doc::SpreadRef::Doc(0)).translation();
        let at = |kind| PointerEvent::new(kind, 200.0 + off.x, 400.0 + off.y);
        s.pointer(&at(PointerKind::Down), view).unwrap();
        s.pointer(&at(PointerKind::Up), view).unwrap();
        assert_eq!(story(&s, a).1, vec![a]);
        assert_eq!(s.tool_id(), "loadedText");
    }

    /// Regression: with a primary text frame, deleting a page or breaking the thread must
    /// not bring the pages back while the story oversets; Smart Text Reflow follows text edits.
    #[test]
    fn primary_text_frame_pages_can_be_deleted_and_unthreaded() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"pages": 1, "primaryTextFrame": true})).unwrap();
        let sid = s.doc().unwrap().doc.settings.primary_story.expect("primary story").0;
        s.execute("text.select", &json!({"story": sid, "anchor": 0, "focus": 0})).unwrap();
        s.execute("text.insert", &json!({"text": LONG.repeat(250)})).unwrap();
        let pages = s.doc().unwrap().doc.page_count();
        assert!(pages >= 3, "{pages}");
        s.execute("select.set", &json!({"ids": []})).unwrap();
        s.execute("layout.pages.delete", &json!({"pages": [pages - 1]})).unwrap();
        assert_eq!(s.doc().unwrap().doc.page_count(), pages - 1, "the deleted page stays deleted");
        let frames: Vec<u64> = s.doc().unwrap().doc.story(StoryId(sid)).unwrap().frames.iter().map(|f| f.0).collect();
        s.execute("frame.unthread", &json!({"frame": frames[0]})).unwrap();
        assert_eq!(s.doc().unwrap().doc.page_count(), pages - 1, "unthreading adds no pages");
        assert_eq!(s.doc().unwrap().doc.story(StoryId(sid)).unwrap().frames.len(), 1);
        // Typing into the primary story reflows again.
        s.execute("text.select", &json!({"story": sid, "anchor": 0, "focus": 0})).unwrap();
        s.execute("text.insert", &json!({"text": "More. "})).unwrap();
        assert!(s.doc().unwrap().doc.page_count() > pages - 1, "pages follow the edit");
        s.doc().unwrap().doc.check().unwrap();
    }
}
