use gpui::{App, Bounds, IntoElement, ParentElement, Pixels, Styled as _, Window, canvas};

use crate::TextSelectionScopeId;

/// Extends a GPUI parent element with post-layout prepaint observation.
pub trait ElementExt: ParentElement + Sized {
    /// Marks this element subtree as belonging to a text-selection scope.
    fn text_selection_scope(self, scope: TextSelectionScopeId) -> impl IntoElement
    where
        Self: IntoElement,
    {
        crate::text_selection::text_selection_scope(scope, self)
    }

    /// Invokes `callback` during prepaint with this element's resolved bounds.
    ///
    /// The bounds are measured by a child laid over the element, so the
    /// element must be positioned, as `relative()` makes it. Otherwise they are
    /// the bounds of its nearest positioned ancestor.
    fn on_prepaint<F>(self, callback: F) -> Self
    where
        F: FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static,
    {
        self.child(
            canvas(
                move |bounds, window, cx| callback(bounds, window, cx),
                |_, _, _, _| {},
            )
            .absolute()
            // An absolute child with no inset stays where the flow would put
            // it, below the text of an element laid out as a block.
            .top_0()
            .left_0()
            .size_full(),
        )
    }
}

impl<T: ParentElement> ElementExt for T {}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use gpui::{
        Context, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext, Window, div,
        point, px, size,
    };

    use super::ElementExt as _;

    struct BlockWithText(Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>);

    impl Render for BlockWithText {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let seen = self.0.clone();
            div().pt(px(10.)).child(
                div()
                    .relative()
                    .w(px(120.))
                    .h(px(20.))
                    .child("Some text")
                    .on_prepaint(move |bounds, _, _| seen.set(Some(bounds))),
            )
        }
    }

    #[gpui::test]
    fn a_block_element_reports_its_own_bounds(cx: &mut TestAppContext) {
        let seen = Rc::new(Cell::new(None));
        let (_, cx) = cx.add_window_view({
            let seen = seen.clone();
            move |_, _| BlockWithText(seen)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            seen.get(),
            Some(gpui::Bounds::new(
                point(px(0.), px(10.)),
                size(px(120.), px(20.))
            ))
        );
    }
}
