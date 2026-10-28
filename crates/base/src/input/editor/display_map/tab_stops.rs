//! A tab reaches the next tab stop, measured from the start of its buffer line,
//! as in every code editor. The text system knows nothing of tab stops, so a
//! tab is shaped as a space and then widened.
use std::sync::Arc;

use gpui::{Pixels, ShapedLine, SharedString, TextRun, WindowTextSystem, px};

/// How close to a tab stop counts as on it. Positions are sums of glyph
/// widths, so one that should land on a stop can fall a hair short of it.
const STOP_SLACK: Pixels = px(0.01);

/// The width of a tab that starts `x` from the start of its buffer line.
pub(crate) fn tab_advance(x: Pixels, tab_width: Pixels) -> Pixels {
    if tab_width <= px(0.) {
        return px(0.);
    }
    let stops_passed = ((x + STOP_SLACK) / tab_width).floor();
    tab_width * (stops_passed + 1.) - x
}

/// Shapes one row of a buffer line, with each tab reaching the next stop.
///
/// `origin` is how far the row starts from the start of its buffer line.
pub(crate) fn shape_line_with_tabs(
    text_system: &WindowTextSystem,
    text: SharedString,
    font_size: Pixels,
    runs: &[TextRun],
    origin: Pixels,
    tab_width: Pixels,
) -> ShapedLine {
    if !text.contains('\t') {
        return text_system.shape_line(text, font_size, runs, None);
    }
    let spaced: SharedString = text.replace('\t', " ").into();
    let mut shaped = text_system.shape_line(spaced, font_size, runs, None);
    widen_tabs(&mut shaped, text.as_bytes(), origin, tab_width);
    // The whitespace indicators look for the tabs in the text.
    shaped.text = text;
    shaped
}

fn widen_tabs(shaped: &mut ShapedLine, text: &[u8], origin: Pixels, tab_width: Pixels) {
    let layout: &gpui::LineLayout = shaped;
    let mut runs = layout.runs.clone();
    let starts: Vec<Pixels> = runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.position.x))
        .chain([layout.width])
        .collect();
    let mut shift = px(0.);
    let glyphs = runs.iter_mut().flat_map(|run| run.glyphs.iter_mut());
    for (glyph, start) in glyphs.zip(starts.windows(2)) {
        let shaped_width = start[1] - start[0];
        glyph.position.x += shift;
        if text.get(glyph.index) == Some(&b'\t') {
            shift += tab_advance(origin + glyph.position.x, tab_width) - shaped_width;
        }
    }
    let widened = gpui::LineLayout {
        font_size: layout.font_size,
        width: layout.width + shift,
        ascent: layout.ascent,
        descent: layout.descent,
        runs,
        len: layout.len,
    };
    let layout: &mut Arc<gpui::LineLayout> = shaped;
    *layout = Arc::new(widened);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_reaches_the_next_stop() {
        let tab = px(16.);
        assert_eq!(tab_advance(px(0.), tab), px(16.));
        assert_eq!(tab_advance(px(4.), tab), px(12.));
        assert_eq!(tab_advance(px(16.), tab), px(16.));
        assert_eq!(tab_advance(px(30.), tab), px(2.));
    }

    #[test]
    fn a_tab_a_hair_short_of_a_stop_starts_from_it() {
        let tab = px(16.);
        let advance = tab_advance(px(15.999), tab);
        assert!((advance - px(16.001)).abs() < px(0.0001), "{advance:?}");
    }
}
