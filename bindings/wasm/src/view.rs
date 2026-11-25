//! The editor's view options (layout-usable-by-hand phase 3): what the canvas shows over the
//! pages (guides, frame edges, margins, bleed, grids, hidden characters), the screen mode, what
//! drags snap to and the ruler unit. They are the editor's state, set through `setView`: never
//! part of the document, its file or its history, so the same layout saves the same bytes however
//! it is viewed.

use kition_layout_engine::{SnapView, ViewInfo};
use kition_layout_geom::{Rect, Unit};
use serde::Deserialize;

/// Normal shows the non-printing aids; Preview the trimmed pages only; Bleed the pages out to
/// their bleed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScreenMode {
    #[default]
    Normal,
    Preview,
    Bleed,
}

/// A ruler guide: its spread (index of the document's spreads), page in the spread and index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct GuideRef {
    pub spread: usize,
    pub page: usize,
    pub index: usize,
}

/// What the editor shows and snaps to. Absent keys keep their defaults (InDesign's: guides,
/// margins, frame edges and bleed shown; grids and hidden characters hidden; snap to guides and
/// smart guides on, the document grid off; points).
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewOptions {
    /// Ruler guides.
    pub guides: bool,
    /// Margin and column guides.
    pub margins: bool,
    pub frame_edges: bool,
    /// Bleed and slug outlines.
    pub bleed: bool,
    pub document_grid: bool,
    pub baseline_grid: bool,
    pub hidden_characters: bool,
    pub screen_mode: ScreenMode,
    pub snap_to_guides: bool,
    pub snap_to_document_grid: bool,
    pub smart_guides: bool,
    /// The ruler unit: tools' readouts measure in it.
    pub unit: Unit,
    /// The guide the editor has selected (drawn thicker).
    pub selected_guide: Option<GuideRef>,
    /// The guide being dragged: the editor draws it where it goes, so it is not drawn where it was.
    pub moving_guide: Option<GuideRef>,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            guides: true,
            margins: true,
            frame_edges: true,
            bleed: true,
            document_grid: false,
            baseline_grid: false,
            hidden_characters: false,
            screen_mode: ScreenMode::Normal,
            snap_to_guides: true,
            snap_to_document_grid: false,
            smart_guides: true,
            unit: Unit::Points,
            selected_guide: None,
            moving_guide: None,
        }
    }
}

impl ViewOptions {
    /// The options a JSON object sets (the editor's `setView`); an unknown key or value is an error.
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| format!("invalid view options: {error}"))
    }

    /// The non-printing aids show (Normal screen mode).
    pub fn aids(&self) -> bool {
        self.screen_mode == ScreenMode::Normal
    }

    /// What drags snap to. Guides and the baseline grid catch only while they are shown, so
    /// nothing invisible pulls a shape (the walkthrough's hidden 12 pt baseline grid).
    pub fn snap(&self) -> SnapView {
        let shown = self.aids();
        SnapView {
            snap_to_guides: self.snap_to_guides,
            snap_to_document_grid: self.snap_to_document_grid,
            show_guides: self.guides && shown,
            smart_guides: self.smart_guides,
            snap_to_baseline_grid: self.baseline_grid && shown,
            ..SnapView::FACTORY
        }
    }

    /// The tools' view at `zoom` screen pixels per point.
    pub fn info(&self, zoom: f64) -> ViewInfo {
        ViewInfo { zoom: if zoom.is_finite() && zoom > 0.0 { zoom } else { 1.0 }, snap: self.snap(), unit: self.unit }
    }
}

/// Paints `color` over every pixel of a `width × height` straight-RGBA image outside `shown`
/// (rectangles in pixels): Preview and Bleed hide the pasteboard and what lies on it.
pub fn mask_outside(rgba: &mut [u8], width: u32, height: u32, shown: &[Rect], color: [u8; 4]) {
    let (w, h) = (width as usize, height as usize);
    for y in 0..h {
        let cy = y as f64 + 0.5;
        // The spans of this row inside a shown rectangle.
        let spans: Vec<(usize, usize)> = shown
            .iter()
            .filter(|r| r.y0 <= cy && cy < r.y1)
            .map(|r| (r.x0.max(0.0).round() as usize, r.x1.max(0.0).round().min(w as f64) as usize))
            .collect();
        for x in 0..w {
            let cx = x as f64 + 0.5;
            if spans.iter().any(|(a, b)| (*a as f64) <= cx && cx < *b as f64) {
                continue;
            }
            let at = (y * w + x) * 4;
            if let Some(pixel) = rgba.get_mut(at..at + 4) {
                pixel.copy_from_slice(&color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_parse_with_defaults_and_refuse_unknown_keys() {
        let view = ViewOptions::parse(r#"{"guides": false, "screenMode": "preview", "unit": "millimeters"}"#).unwrap();
        assert!(!view.guides && view.frame_edges && !view.aids());
        assert_eq!(view.unit, Unit::Millimeters);
        assert!(ViewOptions::parse(r#"{"guide": true}"#).is_err());
        assert!(ViewOptions::parse(r#"{"screenMode": "outline"}"#).is_err());
        assert_eq!(ViewOptions::parse("{}").unwrap(), ViewOptions::default());
    }

    #[test]
    fn hidden_guides_and_grids_catch_nothing() {
        let mut view = ViewOptions::default();
        assert!(view.snap().show_guides && !view.snap().snap_to_baseline_grid);
        view.baseline_grid = true;
        assert!(view.snap().snap_to_baseline_grid);
        view.screen_mode = ScreenMode::Preview;
        assert!(!view.snap().show_guides && !view.snap().snap_to_baseline_grid);
        view.smart_guides = false;
        view.snap_to_guides = false;
        assert!(!view.snap().any());
    }

    #[test]
    fn the_mask_covers_only_what_lies_outside() {
        let mut rgba = vec![9u8; 4 * 4 * 2];
        mask_outside(&mut rgba, 4, 2, &[Rect::new(1.0, 0.0, 3.0, 1.0)], [1, 2, 3, 255]);
        let pixel = |x: usize, y: usize| rgba[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4].to_vec();
        assert_eq!(pixel(1, 0), vec![9, 9, 9, 9]);
        assert_eq!(pixel(2, 0), vec![9, 9, 9, 9]);
        assert_eq!(pixel(0, 0), vec![1, 2, 3, 255]);
        assert_eq!(pixel(3, 0), vec![1, 2, 3, 255]);
        assert_eq!(pixel(1, 1), vec![1, 2, 3, 255]);
    }
}
