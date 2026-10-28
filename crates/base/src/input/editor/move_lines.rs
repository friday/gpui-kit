use std::ops::Range;

use gpui::{Context, Window};

use crate::input::{
    InputBaseState, InputModeKind, MoveLineDown, MoveLineUp, RopeExt as _, cursor::CursorSelection,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineDirection {
    Up,
    Down,
}

impl<M: InputModeKind> InputBaseState<M> {
    pub(super) fn move_line_up(
        &mut self,
        _: &MoveLineUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_lines(LineDirection::Up, window, cx);
    }

    pub(super) fn move_line_down(
        &mut self,
        _: &MoveLineDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_lines(LineDirection::Down, window, cx);
    }

    /// Swaps the lines every selection touches with the line above or below,
    /// as one undo step. Nothing moves when any of them is already at the
    /// edge it would move past.
    fn move_lines(
        &mut self,
        direction: LineDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_editable() || !self.is_multi_line() {
            cx.propagate();
            return;
        }

        let blocks = self.selected_line_blocks();
        let last_row = self.text.lines_len().saturating_sub(1);
        let blocked = blocks.iter().any(|rows| match direction {
            LineDirection::Up => *rows.start() == 0,
            LineDirection::Down => *rows.end() >= last_row,
        });
        if blocks.is_empty() || blocked {
            return;
        }

        let mut edits: Vec<(Range<usize>, String)> = Vec::with_capacity(blocks.len());
        // The bytes each block covers before the move, and how far it moves.
        let mut shifts: Vec<(Range<usize>, isize)> = Vec::with_capacity(blocks.len());
        for rows in &blocks {
            let (first, last) = (*rows.start(), *rows.end());
            let (swapped_row, edit_rows) = match direction {
                LineDirection::Up => (first - 1, first - 1..=last),
                LineDirection::Down => (last + 1, first..=last + 1),
            };
            let mut lines: Vec<String> = rows
                .clone()
                .map(|row| self.text.slice_line(row).to_string())
                .collect();
            let swapped_line = self.text.slice_line(swapped_row).to_string();
            match direction {
                LineDirection::Up => lines.push(swapped_line.clone()),
                LineDirection::Down => lines.insert(0, swapped_line.clone()),
            }
            let range = self.text.line_start_offset(*edit_rows.start())
                ..self.text.line_end_offset(*edit_rows.end());
            edits.push((range, lines.join("\n")));

            let block = self.text.line_start_offset(first)..self.text.line_end_offset(last);
            let distance = (swapped_line.len() + 1) as isize;
            shifts.push((
                block,
                match direction {
                    LineDirection::Up => -distance,
                    LineDirection::Down => distance,
                },
            ));
        }

        // Every edit only reorders lines, so text outside the edits keeps its
        // offsets. A selection in a block moves with it, and so does one that
        // ends at the start of the line after it.
        let shift = |offset: usize| {
            shifts
                .iter()
                .find(|(block, _)| block.start <= offset && offset <= block.end + 1)
                .map_or(offset, |(_, by)| offset.saturating_add_signed(*by))
        };
        let before: Vec<CursorSelection> = self.selections.iter().copied().collect();
        let after: Vec<CursorSelection> = before
            .iter()
            .map(|selection| {
                let mut moved = *selection;
                moved.start = shift(selection.start);
                moved.end = shift(selection.end);
                moved
            })
            .collect();

        self.undo_manager.begin_transaction();
        self.replace_text_in_ranges(&edits, window, cx);
        self.selections.replace_all(after.clone());
        self.undo_manager.record_selections(before, after);
        self.undo_manager.commit_transaction();

        self.scroll_to(self.cursor(), None, cx);
        cx.notify();
    }

    /// The rows each selection covers, joined where they touch, in order. A
    /// selection that ends at the very start of a later line leaves that
    /// line out, as it selects none of its text.
    fn selected_line_blocks(&self) -> Vec<std::ops::RangeInclusive<usize>> {
        let mut rows: Vec<(usize, usize)> = self
            .selections
            .iter()
            .map(|selection| {
                let start = self.text.offset_to_point(selection.start);
                let end = self.text.offset_to_point(selection.end);
                let last = if end.row > start.row && end.column == 0 {
                    end.row - 1
                } else {
                    end.row
                };
                (start.row, last)
            })
            .collect();
        rows.sort_unstable();

        let mut blocks: Vec<(usize, usize)> = Vec::with_capacity(rows.len());
        for (first, last) in rows {
            match blocks.last_mut() {
                Some(block) if first <= block.1 + 1 => block.1 = block.1.max(last),
                _ => blocks.push((first, last)),
            }
        }
        blocks
            .into_iter()
            .map(|(first, last)| first..=last)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        AppContext as _, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, div, px, size,
    };

    use crate::input::{EditorState, MoveLineDown, MoveLineUp, Redo, Undo};

    struct Harness;

    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn editor(
        cx: &mut TestAppContext,
        text: &str,
        selection: std::ops::Range<usize>,
    ) -> (Entity<EditorState>, VisualTestContext) {
        cx.update(crate::init);
        let mut editor = None;
        let window = cx.open_window(size(px(240.), px(140.)), |window, cx| {
            let state = cx.new(|cx| EditorState::new(window, cx).default_value(text));
            state.update(cx, |state, cx| state.set_selected_range(selection, cx));
            editor = Some(state);
            Harness
        });
        let cx = VisualTestContext::from_window(window.into(), cx);
        (editor.unwrap(), cx)
    }

    fn text_and_selection(
        state: &Entity<EditorState>,
        cx: &mut VisualTestContext,
    ) -> (String, std::ops::Range<usize>) {
        state.read_with(cx, |state, _| {
            (state.value().to_string(), state.selected_range())
        })
    }

    #[gpui::test]
    fn moves_the_caret_line_up_and_down_but_not_past_the_end(cx: &mut TestAppContext) {
        let (state, mut cx) = editor(cx, "one\ntwo\nthree", 5..5);
        cx.update(|window, cx| {
            state.update(cx, |state, cx| state.move_line_up(&MoveLineUp, window, cx));
        });
        assert_eq!(
            text_and_selection(&state, &mut cx),
            ("two\none\nthree".into(), 1..1)
        );

        cx.update(|window, cx| {
            state.update(cx, |state, cx| {
                state.move_line_down(&MoveLineDown, window, cx);
                state.move_line_down(&MoveLineDown, window, cx);
                state.move_line_down(&MoveLineDown, window, cx);
            });
        });
        assert_eq!(
            text_and_selection(&state, &mut cx),
            ("one\nthree\ntwo".into(), 11..11)
        );
    }

    #[gpui::test]
    fn moves_every_selected_line_as_one_undo_step(cx: &mut TestAppContext) {
        // "b" and "c" selected up to the start of "d", which stays put.
        let (state, mut cx) = editor(cx, "a\nb\nc\nd", 2..6);
        cx.update(|window, cx| {
            state.update(cx, |state, cx| state.move_line_up(&MoveLineUp, window, cx));
        });
        assert_eq!(
            text_and_selection(&state, &mut cx),
            ("b\nc\na\nd".into(), 0..4)
        );

        cx.update(|window, cx| {
            state.update(cx, |state, cx| state.undo(&Undo, window, cx));
        });
        assert_eq!(
            text_and_selection(&state, &mut cx),
            ("a\nb\nc\nd".into(), 2..6)
        );

        cx.update(|window, cx| {
            state.update(cx, |state, cx| state.redo(&Redo, window, cx));
        });
        assert_eq!(text_and_selection(&state, &mut cx).0, "b\nc\na\nd");
    }
}
