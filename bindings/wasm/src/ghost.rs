//! The whole placed graphic of a frame, outside the frame too, drawn translucent with the page
//! while the Selection tool moves or sizes it inside its frame (InDesign shows the part the frame
//! crops as a faint image). The document does not change: the graphic is drawn from a copy of the
//! document that holds only it, unclipped, and laid over the page at [`ALPHA`].

use std::sync::Arc;

use kition_layout_doc::{Content, Document, Fill, Item, ItemId, Shape, Stroke};
use kition_layout_geom::{Rect, shapes};

/// How strongly the ghost shows over the page.
pub const ALPHA: u32 = 96;

/// A copy of `doc` holding only frame `id`'s graphic, in a frame as large as the graphic (so
/// nothing of it is cropped), or `None` when the frame holds no placed graphic.
pub fn document(doc: &Document, id: ItemId) -> Option<Document> {
    let loc = doc.find(id)?;
    let it = doc.item_at(&loc)?;
    let Content::Graphic(g) = &it.content else { return None };
    let whole = g.xf.transform_rect_bbox(Rect::new(0.0, 0.0, g.size.0, g.size.1));
    if !(whole.width() > 0.0 && whole.height() > 0.0 && whole.x0.is_finite() && whole.y0.is_finite()) {
        return None;
    }
    let mut ghost = Item::new(id, it.layer, Shape::Rectangle, shapes::rectangle(whole));
    ghost.xf = doc.parent_xf(&loc) * it.xf;
    ghost.content = it.content.clone();
    ghost.fill = Fill::none();
    ghost.stroke = Stroke::none();
    let mut copy = doc.clone();
    for spread in copy.spreads.iter_mut().chain(copy.parents.iter_mut()) {
        Arc::make_mut(spread).items.clear();
    }
    let spread = copy.spread_mut(loc.spread)?;
    spread.items.push(Arc::new(ghost));
    Some(copy)
}

/// Lays `ghost` (straight RGBA, transparent where there is no graphic) over `pixels` at [`ALPHA`].
pub fn blend(pixels: &mut [u8], ghost: &[u8]) {
    for (px, g) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(ghost.as_chunks::<4>().0) {
        let a = u32::from(g[3]) * ALPHA / 255;
        if a == 0 {
            continue;
        }
        for (channel, over) in px.iter_mut().zip(g).take(3) {
            *channel = ((u32::from(*channel) * (255 - a) + u32::from(*over) * a + 127) / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ghost_lies_faintly_over_the_page() {
        let mut page = vec![255, 255, 255, 255, 10, 20, 30, 255];
        blend(&mut page, &[0, 0, 0, 255, 0, 0, 0, 0]);
        assert!(page[0] < 255 && page[0] > 128, "{page:?}");
        assert_eq!(&page[4..8], &[10, 20, 30, 255], "no ghost there: unchanged");
    }
}
