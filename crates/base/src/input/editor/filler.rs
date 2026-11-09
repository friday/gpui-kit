//! Filler: empty rows that keep an editor level with another.

use gpui::{Context, ScrollHandle};

use super::{EditorState, Filler};

impl EditorState {
    /// Replaces all filler with `fillers`.
    ///
    /// Filler stands between lines without being part of the text. The caret
    /// steps over it, a click on it lands on the nearest row of text, and it
    /// paints as a quiet band with no line number. It stays on its buffer line
    /// through every rewrap and moves with the lines around it when the text
    /// is edited. Filler given twice for one line is added up, and a line past
    /// the end stands for the end.
    pub fn set_fillers(&mut self, fillers: Vec<Filler>, cx: &mut Context<Self>) {
        self.display_map.set_fillers(fillers);
        cx.notify();
    }

    /// The filler as set and since moved by edits, sorted by line.
    pub fn fillers(&self) -> &[Filler] {
        self.display_map.fillers()
    }

    /// How many rows of text each buffer line wraps to at the current wrap
    /// width, filler not counted. The width is known from the first layout
    /// on, which [`InputEvent::Rewrap`](super::InputEvent::Rewrap) announces
    /// along with every later change that is not an edit.
    pub fn text_rows_per_line(&self) -> Vec<usize> {
        self.display_map.text_rows_per_line()
    }

    /// Scrolls with `scroll_handle` instead of a handle of its own.
    ///
    /// Editors that track one handle scroll as one surface: the wheel or the
    /// scrollbar over either moves them all. They need the same content
    /// height, which filler can give them, or the shortest one clamps the
    /// offset for the rest.
    pub fn track_scroll(mut self, scroll_handle: &ScrollHandle) -> Self {
        self.scroll_handle = scroll_handle.clone();
        self
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, MouseButton,
        ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window, div,
        point, px, size,
    };

    use crate::input::{EditorState, Filler, InputEvent, RopeExt as _};

    struct Root(Entity<EditorState>);

    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().id("root").size_full().child(self.0.clone())
        }
    }

    fn open(
        cx: &mut TestAppContext,
        text: &str,
        fillers: Vec<Filler>,
    ) -> (Entity<EditorState>, VisualTestContext) {
        cx.update(crate::init);
        let text = text.to_string();
        let mut editor = None;
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                let state = cx.new(|cx| EditorState::new(window, cx).default_value(text));
                state.update(cx, |state, cx| state.set_fillers(fillers, cx));
                editor = Some(state.clone());
                cx.new(|_| Root(state))
            })
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let editor = editor.unwrap();
        cx.update(|window, cx| editor.update(cx, |state, cx| state.focus(window, cx)));
        cx.run_until_parked();
        (editor, cx)
    }

    fn caret_line(editor: &Entity<EditorState>, cx: &mut VisualTestContext) -> usize {
        editor.read_with(cx, |state, _| {
            state.text.offset_to_point(state.cursor()).row
        })
    }

    #[gpui::test]
    fn the_caret_steps_over_filler_both_ways(cx: &mut TestAppContext) {
        let fillers = vec![Filler::new(0, 1), Filler::new(1, 2), Filler::new(3, 1)];
        let (editor, mut cx) = open(cx, "one\ntwo\nthree", fillers);
        let rows = editor.read_with(&cx, |state, _| state.display_map.wrap_row_count());
        assert_eq!(rows, 3 + 4, "three lines of text and four of filler");
        assert_eq!(caret_line(&editor, &mut cx), 0);
        for (keys, line) in [("down", 1), ("down", 2), ("down", 2), ("up", 1), ("up", 0)] {
            cx.simulate_keystrokes(keys);
            assert_eq!(caret_line(&editor, &mut cx), line, "after {keys}");
        }
        cx.simulate_keystrokes("up");
        editor.read_with(&cx, |state, _| assert_eq!(state.cursor(), 0));
    }

    #[gpui::test]
    fn a_click_on_filler_lands_on_the_nearest_line(cx: &mut TestAppContext) {
        let (editor, mut cx) = open(cx, "one\ntwo\nthree", vec![Filler::new(1, 2)]);
        let (origin, line_height) = editor.read_with(&cx, |state, _| {
            let layout = state.last_layout.as_ref().unwrap();
            let origin = state.last_bounds.unwrap().origin;
            (
                origin + point(layout.line_number_width + px(2.), px(0.)),
                layout.line_height,
            )
        });
        // Line 0 is row 0, the filler rows 1 and 2, and line 1 row 3.
        for (rows_down, line) in [(1.2, 0), (2.8, 1), (0.5, 0), (3.5, 1)] {
            let position = origin + point(px(0.), line_height * rows_down);
            cx.simulate_mouse_down(position, MouseButton::Left, Default::default());
            cx.simulate_mouse_up(position, MouseButton::Left, Default::default());
            assert_eq!(
                caret_line(&editor, &mut cx),
                line,
                "at {rows_down} rows down"
            );
        }
    }

    #[gpui::test]
    fn a_resize_that_rewraps_is_announced(cx: &mut TestAppContext) {
        let line = "word ".repeat(40);
        let (editor, mut cx) = open(cx, &line, vec![]);
        let rewraps = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = rewraps.clone();
        cx.update(|_, cx| {
            cx.subscribe(&editor, move |_, event: &InputEvent, _| {
                if matches!(event, InputEvent::Rewrap) {
                    seen.set(seen.get() + 1);
                }
            })
            .detach();
        });
        let rows = editor.read_with(&cx, |state, _| state.text_rows_per_line());
        cx.simulate_resize(size(px(300.), px(400.)));
        cx.run_until_parked();
        assert_eq!(rewraps.get(), 1);
        let narrow = editor.read_with(&cx, |state, _| state.text_rows_per_line());
        assert!(narrow[0] > rows[0], "{narrow:?} after {rows:?}");
    }
}
