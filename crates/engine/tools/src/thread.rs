//! Threading text by hand: a selected text frame's in and out ports, and the loaded text cursor.
//!
//! Clicking a selected text frame's out port (or in port) with the Selection tool loads the text
//! cursor (`frame.loadText`). With it loaded, clicking an empty or text frame threads it
//! (`frame.thread { from, to }`), dragging draws a new frame and threads it, and clicking empty
//! space makes a frame the width of the column under the pointer down to the bottom margin.
//! Alt-clicking keeps the cursor loaded from the new frame (semi-autoflow); Shift-clicking from an
//! out port autoflows (pages with threaded frames are added until the story fits). Double-clicking a port
//! breaks the thread there (`frame.unthread`), and so does clicking the frame already linked on the
//! loaded side; the cursor shows which a click does. Escape unloads the cursor.

use kition_layout_doc::{Content, Item, ItemId, SpreadRef};
use kition_layout_geom::{Affine, Point, Rect};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext, ToolKey};

/// Which port of a frame: `in` (before the frame in its thread) or `out` (after it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    In,
    Out,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::In => "in",
            Side::Out => "out",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "in" | "before" => Some(Side::In),
            "out" | "after" => Some(Side::Out),
            _ => None,
        }
    }
}

/// A text frame's in and out ports in canvas points, for the item at `a` (its spread's canvas
/// transform) at `zoom` screen pixels per point: the in port near the top-left corner, the out
/// port near the bottom-right corner, a fixed screen distance inside them.
pub fn port_points(it: &Item, a: Affine, zoom: f64) -> [Point; 2] {
    let px = 1.0 / zoom.max(1e-6);
    let r = it.inner_bounds();
    let m = a * it.xf;
    [m * Point::new(r.x0, r.y0) + (0.0, 14.5 * px), m * Point::new(r.x1, r.y1) - (0.0, 12.0 * px)]
}

/// The selected text frame whose in or out port is under canvas point `p`.
pub fn port_at(cx: &ToolContext, p: Point) -> Option<(ItemId, Side)> {
    let tol = cx.tol(6.0);
    for id in &cx.selection.items {
        let (Some(it), Some(a)) = (cx.doc.item(*id), cx.item_canvas_xf(*id)) else { continue };
        if !matches!(it.content, Content::Text(_)) || it.locked {
            continue;
        }
        let [inp, outp] = port_points(it, a, cx.zoom);
        if (outp - p).hypot() <= tol {
            return Some((*id, Side::Out));
        }
        if (inp - p).hypot() <= tol {
            return Some((*id, Side::In));
        }
    }
    None
}

/// View › Show Text Threads: for every story of the selection (selected text frames, or the text
/// being edited), a line from each frame's out port to the next frame's in port, in canvas
/// points, in the frame's layer colour.
pub fn thread_lines(cx: &ToolContext) -> Vec<Overlay> {
    let mut stories = Vec::new();
    if let Some(ts) = &cx.selection.text {
        stories.push(ts.story);
    }
    for id in &cx.selection.items {
        if let Some(t) = cx.doc.item(*id).and_then(|it| it.text_frame()) {
            stories.push(t.story);
        }
    }
    stories.sort_by_key(|s| s.0);
    stories.dedup();
    let mut out = Vec::new();
    for sid in stories {
        let Some(story) = cx.doc.story(sid) else { continue };
        for pair in story.frames.windows(2) {
            let [a, b] = pair else { continue };
            let (Some(ia), Some(xa), Some(ib), Some(xb)) = (cx.doc.item(*a), cx.item_canvas_xf(*a), cx.doc.item(*b), cx.item_canvas_xf(*b)) else {
                continue;
            };
            let [_, out_port] = port_points(ia, xa, cx.zoom);
            let [in_port, _] = port_points(ib, xb, cx.zoom);
            let color = cx.doc.layer(ia.layer).map_or([74, 144, 226], |layer| layer.color);
            out.push(Overlay::Line { a: out_port, b: in_port, color, dashed: false });
        }
    }
    out
}

/// What a click with the loaded text cursor does at a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    /// Nothing there: a new frame.
    Empty,
    /// The frame the text was loaded from (the second click of a double click on its port).
    Own,
    /// A frame that can join the thread.
    Thread(ItemId),
    /// The frame already linked on the loaded side: the click breaks the thread there.
    Unthread,
    /// A frame that can't take the text (a picture, a group, a frame of the same story).
    Invalid,
}

/// The text cursor loaded from `from`'s `side` port.
pub struct LoadedTextTool {
    from: ItemId,
    side: Side,
    /// Alt held on release: the cursor loads again from the new frame (semi-autoflow).
    again: bool,
    /// Shift held on release: autoflow (pages and frames are added until the story fits).
    flow: bool,
    start: Option<Point>,
    cur: Point,
}

impl LoadedTextTool {
    pub fn new(from: ItemId, side: Side) -> Self {
        Self { from, side, again: false, flow: false, start: None, cur: Point::ORIGIN }
    }

    pub fn from(&self) -> ItemId {
        self.from
    }

    pub fn side(&self) -> Side {
        self.side
    }

    fn base(&self) -> Value {
        let mut p = json!({ "from": self.from.0, "side": self.side.as_str() });
        if self.flow {
            p["autoflow"] = json!(true);
        } else if self.again {
            p["keepLoaded"] = json!(true);
        }
        p
    }

    /// `frame.thread` onto frame `to`, or `None` when it cannot take the text (a graphic, a
    /// group, or a frame already in the story).
    fn onto(&self, cx: &ToolContext, to: ItemId) -> Option<Action> {
        let it = cx.doc.item(to)?;
        let story = |id: ItemId| cx.doc.item(id).and_then(|i| i.text_frame()).map(|t| t.story);
        match &it.content {
            Content::Unassigned => {}
            Content::Text(t) if Some(t.story) != story(self.from) => {}
            _ => return None,
        }
        let mut p = self.base();
        p["to"] = json!(to.0);
        Some(Action::Exec("frame.thread".into(), p))
    }

    /// The frame linked on the loaded side: the next frame from an out port, the previous one
    /// from an in port.
    fn neighbour(&self, cx: &ToolContext) -> Option<ItemId> {
        match self.side {
            Side::Out => cx.doc.next_frame(self.from),
            Side::In => cx.doc.prev_frame(self.from),
        }
    }

    /// What a click at canvas point `p` does.
    fn target(&self, cx: &ToolContext, p: Point) -> Target {
        let Some((_, id)) = cx.hit(p) else { return Target::Empty };
        if cx.doc.top_level_of(id).unwrap_or(id) == self.from || id == self.from {
            return Target::Own;
        }
        if Some(id) == self.neighbour(cx) {
            return Target::Unthread;
        }
        match self.onto(cx, id) {
            Some(_) => Target::Thread(id),
            None => Target::Invalid,
        }
    }

    /// `frame.thread` into a new frame at `rect` on spread `sr` (spread coordinates).
    fn new_frame(&self, sr: SpreadRef, rect: Rect) -> Action {
        let mut p = self.base();
        p["spread"] = crate::spread_json(sr);
        p["rect"] = crate::rect_json(rect);
        Action::Exec("frame.thread".into(), p)
    }

    /// A click on empty space: a frame the width of the column under the pointer, from the
    /// pointer down to the page's bottom margin (InDesign's click with a loaded text cursor);
    /// off the pages, a 216 × 216 pt frame.
    fn click_rect(cx: &ToolContext, sr: SpreadRef, at: Point) -> Rect {
        let page = cx.doc.spread(sr).and_then(|sp| sp.pages.iter().find(|pg| pg.bounds().contains(at)));
        if let Some(page) = page {
            let margin = page.margin_rect();
            let columns = page.column_rects();
            let column = columns.iter().find(|c| at.x >= c.x0 && at.x <= c.x1).copied().unwrap_or(margin);
            let top = at.y.clamp(margin.y0, (margin.y1 - 12.0).max(margin.y0));
            if margin.y1 - top >= 12.0 {
                return Rect::new(column.x0, top, column.x1, margin.y1);
            }
        }
        Rect::new(at.x, at.y, at.x + 216.0, at.y + 216.0)
    }
}

impl Tool for LoadedTextTool {
    fn id(&self) -> &'static str {
        "loadedText"
    }

    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let p = ev.pos;
        match ev.kind {
            PointerKind::Down => {
                self.start = Some(p);
                self.cur = p;
                vec![]
            }
            PointerKind::Drag => {
                self.cur = p;
                vec![]
            }
            PointerKind::Move => vec![],
            PointerKind::Up => {
                let Some(start) = self.start.take() else { return vec![] };
                self.again = ev.mods.alt;
                // Autoflow follows the story forward, so only from an out port.
                self.flow = ev.mods.shift && self.side == Side::Out;
                let Some((sr, sa)) = cx.layout.spread_at(start) else { return vec![] };
                let drew = (p - start).hypot() >= cx.tol(3.0);
                if drew {
                    let b = cx.layout.to_spread(sr, p);
                    let rect = Rect::from_points(sa, b);
                    if rect.width() < 1.0 || rect.height() < 1.0 {
                        return vec![];
                    }
                    return vec![self.new_frame(sr, rect)];
                }
                match self.target(cx, start) {
                    // Its own frame (the second click of a double-click on a port): wait.
                    Target::Own | Target::Invalid => vec![],
                    // The frame already linked there: the thread breaks between them.
                    Target::Unthread => vec![
                        Action::SwitchTool("selection".into()),
                        Action::Exec("frame.unthread".into(), json!({ "frame": self.from.0, "side": self.side.as_str() })),
                    ],
                    Target::Thread(id) => self.onto(cx, id).into_iter().collect(),
                    Target::Empty => vec![self.new_frame(sr, Self::click_rect(cx, sr, sa))],
                }
            }
            PointerKind::DoubleClick => {
                // Double-clicking the port the text was loaded from breaks the thread there.
                let (Some(it), Some(a)) = (cx.doc.item(self.from), cx.item_canvas_xf(self.from)) else { return vec![] };
                let [inp, outp] = port_points(it, a, cx.zoom);
                let at = match self.side {
                    Side::In => inp,
                    Side::Out => outp,
                };
                if (at - p).hypot() > cx.tol(6.0) {
                    return vec![];
                }
                vec![
                    Action::SwitchTool("selection".into()),
                    Action::Exec("frame.unthread".into(), json!({ "frame": self.from.0, "side": self.side.as_str() })),
                ]
            }
        }
    }

    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        match key {
            ToolKey::Escape => vec![Action::SwitchTool("selection".into())],
            _ => vec![],
        }
    }

    fn overlays(&self, _cx: &ToolContext) -> Vec<Overlay> {
        match self.start {
            Some(a) if (self.cur - a).hypot() > 0.0 => vec![Overlay::Marquee(Rect::from_points(a, self.cur))],
            _ => vec![],
        }
    }

    /// The cursor says what a click there does: flow the text in (link), break the link, nothing
    /// (not allowed), or, over empty space and the frame it was loaded from, the loaded cursor.
    fn cursor(&self, cx: &ToolContext, p: Point, _m: Mods) -> Cursor {
        match self.target(cx, p) {
            Target::Empty | Target::Own => Cursor::LoadedText,
            Target::Thread(_) => Cursor::ThreadLink,
            Target::Unthread => Cursor::Unthread,
            Target::Invalid => Cursor::NotAllowed,
        }
    }

    /// Loaded, the tool takes Escape (to unload) and waits for its click.
    fn busy(&self) -> bool {
        true
    }
}
