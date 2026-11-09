//! The paint order of a window's deferred surfaces.
//!
//! A window stacks in layers: the window itself, then one per open dialog,
//! oldest first. Each layer owns a band of [`LAYER_STRIDE`] priorities. A
//! dialog paints at the base of its band, and a popup paints one above the
//! surface whose content drew it. So a popup opened from the window stays
//! under every dialog, a dialog's own dropdown paints over that dialog and
//! under the next one, and a submenu paints over its menu.
//!
//! The layer a popup belongs to is read as it prepaints, not as it renders,
//! because a cached view renders in prepaint and a nested popup prepaints a
//! round after the surface that drew it.
//!
//! GPUI hit-tests nested deferred draws in the order it prepaints them, a
//! round at a time, not by priority. A popup left open in a lower dialog is
//! painted under a later dialog but still takes the pointer where they meet.

use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};

/// Priorities in each layer's band. GPUI asserts that deferred draws nest no
/// more than ten deep, so no layer can stack more popups than its band holds.
const LAYER_STRIDE: usize = 10;

/// The band of the window itself, under every dialog.
const WINDOW_LAYER: usize = 0;

/// The band of a host outside every layer that serves whichever one has the
/// user. It leaves room for nine dialogs under it.
pub(crate) const TOP_LAYER: usize = 10 * LAYER_STRIDE;

/// The first priority above every surface stacked here, for what must never
/// be covered, such as a toast.
pub const ABOVE_LAYERS_PRIORITY: usize = TOP_LAYER + LAYER_STRIDE;

thread_local! {
    /// The priority of the surface whose content is prepainting.
    static ENCLOSING: Cell<usize> = const { Cell::new(WINDOW_LAYER) };
}

/// Defers `child` as a popup of the layer that draws it: over that layer and
/// any popup it was drawn in, under any dialog opened after it.
pub fn deferred_popup(child: impl IntoElement) -> impl IntoElement {
    DeferredSurface::new(Surface::Popup, child)
}

/// Defers `child` as the dialog at `layer`, counted from the oldest open.
pub(crate) fn deferred_dialog(layer: usize, child: impl IntoElement) -> impl IntoElement {
    DeferredSurface::new(Surface::Dialog(layer), child)
}

/// Draws `child` as part of the top layer, so a popup deferred in it paints
/// over every dialog.
///
/// For a host mounted outside every layer that draws for whichever one has
/// the user, such as a window's touch selection or a fallback native menu.
pub fn in_top_layer(child: impl IntoElement) -> impl IntoElement {
    Layer {
        priority: Rc::new(Cell::new(TOP_LAYER)),
        child: child.into_any_element(),
    }
}

#[derive(Clone, Copy)]
enum Surface {
    Dialog(usize),
    Popup,
}

impl Surface {
    fn priority(self) -> usize {
        match self {
            Self::Dialog(layer) => LAYER_STRIDE * (layer + 1),
            Self::Popup => ENCLOSING.get() + 1,
        }
    }
}

/// [`gpui::Deferred`] at the priority its [`Surface`] takes where it prepaints.
struct DeferredSurface {
    surface: Surface,
    /// Shared with the [`Layer`] it defers, which prepaints in a later round.
    priority: Rc<Cell<usize>>,
    layer: Option<AnyElement>,
}

impl DeferredSurface {
    fn new(surface: Surface, child: impl IntoElement) -> Self {
        let priority = Rc::new(Cell::new(WINDOW_LAYER));
        Self {
            surface,
            layer: Some(
                Layer {
                    priority: priority.clone(),
                    child: child.into_any_element(),
                }
                .into_any_element(),
            ),
            priority,
        }
    }
}

impl IntoElement for DeferredSurface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for DeferredSurface {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let layout_id = self
            .layer
            .as_mut()
            .expect("a surface lays out before it defers")
            .request_layout(window, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        let priority = self.surface.priority();
        self.priority.set(priority);
        let layer = self.layer.take().expect("a surface defers once");
        let offset = window.element_offset();
        window.defer_draw(layer, offset, priority, None);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

/// Prepaints `child` as the content of the surface at `priority`.
struct Layer {
    priority: Rc<Cell<usize>>,
    child: AnyElement,
}

impl IntoElement for Layer {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Layer {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let outer = ENCLOSING.replace(self.priority.get());
        self.child.prepaint(window, cx);
        ENCLOSING.set(outer);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::{
        Context, InteractiveElement as _, ParentElement as _, Render, Styled as _, canvas, div,
        point, px,
    };

    use super::*;
    use crate::{Dialog, Popup};

    type Painted = Rc<RefCell<Vec<&'static str>>>;

    /// A surface that notes its name as it paints.
    fn surface(name: &'static str, painted: &Painted) -> gpui::Div {
        let painted = painted.clone();
        div().size(px(40.)).child(
            canvas(
                |_, _, _| {},
                move |_, _, _, _| painted.borrow_mut().push(name),
            )
            .size_full(),
        )
    }

    fn popup(id: &'static str, content: impl IntoElement) -> Popup {
        Popup::new(id, div().size(px(10.))).content(content)
    }

    /// A popup open in the window, and two dialogs, the first with a popup
    /// open in it and a nested popup open in that.
    struct Harness {
        painted: Painted,
    }

    impl Render for Harness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let painted = &self.painted;
            div()
                .size_full()
                .child(popup("window-popup", surface("window popup", painted)))
                .child(
                    Dialog::new(cx).layer(0, false).child(
                        surface("dialog 0", painted).child(popup(
                            "dialog-popup",
                            surface("dialog 0 popup", painted)
                                .child(popup("submenu", surface("dialog 0 submenu", painted))),
                        )),
                    ),
                )
                .child(
                    Dialog::new(cx)
                        .layer(1, true)
                        .child(surface("dialog 1", painted)),
                )
        }
    }

    #[gpui::test]
    fn a_popup_paints_over_its_own_layer_and_under_the_next(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init);
        let painted = Painted::default();
        let (_, window) = cx.add_window_view({
            let painted = painted.clone();
            move |_, _| Harness { painted }
        });
        // A popup places its content once its trigger has been measured, a
        // frame for each popup nested in another.
        for _ in 0..3 {
            painted.borrow_mut().clear();
            window.update(|window, cx| window.draw(cx).clear(cx));
        }

        assert_eq!(
            *painted.borrow(),
            [
                "window popup",
                "dialog 0",
                "dialog 0 popup",
                "dialog 0 submenu",
                "dialog 1",
            ]
        );
    }

    /// The pointer follows the paint: over a dialog, a popup of the window
    /// under it does not take it.
    struct HitHarness {
        dialog_hovered: Rc<Cell<bool>>,
        popup_hovered: Rc<Cell<bool>>,
    }

    impl Render for HitHarness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let dialog = self.dialog_hovered.clone();
            let popup_hovered = self.popup_hovered.clone();
            div()
                .size_full()
                .child(popup(
                    "window-popup",
                    div()
                        .id("popup-content")
                        .size(px(100.))
                        .on_mouse_move(move |_, _, _| popup_hovered.set(true)),
                ))
                .child(
                    Dialog::new(cx).child(
                        div()
                            .id("dialog-content")
                            .occlude()
                            .size_full()
                            .on_mouse_move(move |_, _, _| dialog.set(true)),
                    ),
                )
        }
    }

    #[gpui::test]
    fn a_dialog_takes_the_pointer_from_a_window_popup_under_it(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init);
        let dialog_hovered = Rc::new(Cell::new(false));
        let popup_hovered = Rc::new(Cell::new(false));
        let (_, window) = cx.add_window_view({
            let dialog_hovered = dialog_hovered.clone();
            let popup_hovered = popup_hovered.clone();
            move |_, _| HitHarness {
                dialog_hovered,
                popup_hovered,
            }
        });
        window.update(|window, cx| window.draw(cx).clear(cx));
        window.update(|window, cx| window.draw(cx).clear(cx));

        window.simulate_mouse_move(point(px(50.), px(50.)), None, gpui::Modifiers::default());
        assert!(dialog_hovered.get());
        assert!(!popup_hovered.get());
    }
}
