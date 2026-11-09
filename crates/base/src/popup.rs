use std::{cell::Cell, rc::Rc};

use gpui::{
    Anchor, AnyElement, App, Bounds, Div, ElementId, Hsla, InteractiveElement, Interactivity,
    IntoElement, ParentElement, PathBuilder, Pixels, Point, RenderOnce, StatefulInteractiveElement,
    StyleRefinement, Styled, Window, canvas, div, point, px, size,
};

use crate::{Placement, Positioner, ResolvedPosition, StyledExt as _, deferred_popup};

/// Distance kept between a popup and the window edge.
const WINDOW_MARGIN: Pixels = px(8.);

/// Former paint priority of every popup surface. It paints over every dialog,
/// whatever layer drew the popup, so a popup of the window covers a dialog
/// opened over it.
#[deprecated(note = "use `deferred_popup`, which paints in the layer that draws it")]
pub const POPUP_PRIORITY: usize = crate::stacking::TOP_LAYER;

#[derive(Default)]
struct PopupAnchorState {
    bounds: Bounds<Pixels>,
    captured: bool,
}

/// Where a popup's arrow points on the trigger.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ArrowTarget {
    /// The trigger corner or edge the popup's anchor names.
    #[default]
    Anchor,
    /// The middle of the trigger's edge nearest the popup.
    TriggerCenter,
    /// A point in window coordinates.
    Point(Point<Pixels>),
}

/// A pointer arrow a popup draws on its edge nearest the trigger.
///
/// The arrow follows the popup's anchor to pick its side, and fills with
/// `fill` so it reads as part of the surface. A surface with a hairline ring
/// passes the same colour as `outline`, and its corner `radius`, so the
/// triangle's base stays clear of the curve.
#[derive(Clone, Copy, Debug)]
pub struct PopupArrow {
    depth: Pixels,
    fill: Hsla,
    outline: Option<Hsla>,
    radius: Pixels,
    target: ArrowTarget,
}

impl PopupArrow {
    /// An arrow `fill` draws, 6px deep, pointing at the trigger corner the
    /// popup's anchor names.
    pub fn new(fill: impl Into<Hsla>) -> Self {
        Self {
            depth: px(6.),
            fill: fill.into(),
            outline: None,
            radius: px(0.),
            target: ArrowTarget::Anchor,
        }
    }

    /// How far the tip stands off the surface, and so the extra gap the arrow
    /// takes between the trigger and the surface.
    pub fn depth(mut self, depth: Pixels) -> Self {
        self.depth = depth;
        self
    }

    /// The colour of the hairline round the arrow, for a surface that has a
    /// ring.
    pub fn outline(mut self, outline: impl Into<Hsla>) -> Self {
        self.outline = Some(outline.into());
        self
    }

    /// The surface's corner radius, which the arrow's base stays clear of.
    pub fn radius(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }

    /// Where along the trigger the tip points.
    pub fn target(mut self, target: ArrowTarget) -> Self {
        self.target = target;
        self
    }
}

/// An unstyled trigger and anchored popup host.
///
/// `Popup` owns trigger measurement, anchor-point calculation, first-frame
/// synchronization, deferred rendering, an optional pointer arrow, and
/// window-edge snapping. Callers own open state, interaction, popup content,
/// appearance of the surface, and motion.
#[derive(IntoElement)]
pub struct Popup {
    id: ElementId,
    base: gpui::Stateful<Div>,
    style: StyleRefinement,
    anchor: Anchor,
    offset: Pixels,
    arrow: Option<PopupArrow>,
    on_position: Option<Box<dyn Fn(ResolvedPosition, Bounds<Pixels>)>>,
    trigger: AnyElement,
    content: Option<AnyElement>,
}

impl Popup {
    pub fn new(id: impl Into<ElementId>, trigger: impl IntoElement) -> Self {
        let id = id.into();
        Self {
            base: div().id(id.clone()),
            id,
            style: StyleRefinement::default(),
            anchor: Anchor::TopLeft,
            offset: px(0.),
            arrow: None,
            on_position: None,
            trigger: trigger.into_any_element(),
            content: None,
        }
    }

    pub fn anchor(mut self, anchor: impl Into<Anchor>) -> Self {
        self.anchor = anchor.into();
        self
    }

    /// Gap from the trigger along the anchor's outward direction, zero by default.
    pub fn offset(mut self, offset: Pixels) -> Self {
        self.offset = offset;
        self
    }

    /// Draw an arrow from the surface back to the trigger. The arrow's depth
    /// joins `offset`, so the gap stays between the trigger and the tip.
    pub fn arrow(mut self, arrow: PopupArrow) -> Self {
        self.arrow = Some(arrow);
        self
    }

    /// Observe resolved popup and trigger bounds before content prepaint.
    pub fn on_position(
        mut self,
        callback: impl Fn(ResolvedPosition, Bounds<Pixels>) + 'static,
    ) -> Self {
        self.on_position = Some(Box::new(callback));
        self
    }

    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }

    pub fn resolved_corner(anchor: Anchor, trigger_bounds: Bounds<Pixels>) -> Point<Pixels> {
        match anchor {
            Anchor::TopLeft => trigger_bounds.origin,
            Anchor::TopCenter => trigger_bounds.top_center(),
            Anchor::TopRight => trigger_bounds.top_right(),
            Anchor::BottomLeft => Point {
                x: trigger_bounds.origin.x,
                y: trigger_bounds.origin.y - trigger_bounds.size.height,
            },
            Anchor::BottomCenter => Point {
                x: trigger_bounds.top_center().x,
                y: trigger_bounds.origin.y - trigger_bounds.size.height,
            },
            Anchor::BottomRight => Point {
                x: trigger_bounds.top_right().x,
                y: trigger_bounds.origin.y - trigger_bounds.size.height,
            },
            Anchor::LeftCenter | Anchor::RightCenter => trigger_bounds.origin,
        }
    }
}

/// Match the popup's anchor to the opposite edge of the measured trigger.
fn anchor_position(anchor: Anchor, trigger: Bounds<Pixels>, offset: Pixels) -> Point<Pixels> {
    match anchor {
        Anchor::TopLeft => trigger.bottom_left() + point(px(0.), offset),
        Anchor::TopCenter => trigger.bottom_center() + point(px(0.), offset),
        Anchor::TopRight => trigger.bottom_right() + point(px(0.), offset),
        Anchor::BottomLeft => trigger.origin - point(px(0.), offset),
        Anchor::BottomCenter => trigger.top_center() - point(px(0.), offset),
        Anchor::BottomRight => trigger.top_right() - point(px(0.), offset),
        Anchor::LeftCenter => trigger.right_center() + point(offset, px(0.)),
        Anchor::RightCenter => trigger.left_center() - point(offset, px(0.)),
    }
}

impl Styled for Popup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl InteractiveElement for Popup {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for Popup {}

impl RenderOnce for Popup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Popup {
            id,
            base,
            style,
            anchor,
            offset,
            arrow,
            on_position,
            trigger,
            content,
        } = self;
        let state = window.use_keyed_state((id, "anchor"), cx, |_, _| PopupAnchorState::default());
        let trigger_bounds = Rc::new(Cell::new(state.read(cx).bounds));
        // The arrow takes its depth out of the gap, so the caller's offset
        // stays between the trigger and the tip.
        let offset = offset + arrow.map_or(px(0.), |arrow| arrow.depth);
        let position = Rc::new(Cell::new(anchor_position(
            anchor,
            state.read(cx).bounds,
            offset,
        )));

        // A closed popup places nothing: it notes its trigger's bounds, for when
        // it opens, without asking for another frame.
        let has_content = content.is_some();
        let root = base
            .child(trigger)
            .child(
                canvas(
                    {
                        let state = state.clone();
                        let position = position.clone();
                        let trigger_bounds = trigger_bounds.clone();
                        move |bounds, window, cx| {
                            trigger_bounds.set(bounds);
                            position.set(anchor_position(anchor, bounds, offset));
                            let first = state.update(cx, |state, _| {
                                let first = !state.captured;
                                state.bounds = bounds;
                                state.captured = true;
                                first
                            });
                            if first && has_content {
                                window.request_animation_frame();
                            }
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
                .top_0()
                .left_0(),
            )
            .refine_style(&style);

        let Some(content) = content else {
            return root;
        };
        if !state.read(cx).captured {
            return root;
        }

        // Where the arrow draws: the surface's own bounds, and the trigger's.
        let geometry: Rc<Cell<Option<(Bounds<Pixels>, Bounds<Pixels>)>>> = Rc::default();
        let tracked = geometry.clone();
        let positioner = Positioner::corner(anchor, position.get())
            .tracked_corner_position(position)
            .on_position(move |position| {
                tracked.set(Some((position.bounds, trigger_bounds.get())));
                if let Some(callback) = &on_position {
                    callback(position, trigger_bounds.get());
                }
            });
        let content = match arrow {
            Some(arrow) => div()
                .relative()
                .child(content)
                .child(arrow_canvas(arrow, anchor, geometry))
                .into_any_element(),
            None => content,
        };
        root.child(deferred_popup(
            positioner
                .margin(WINDOW_MARGIN)
                // The host blocks the mouse, so no caller has to remember:
                // what a popup covers belongs to the popup.
                .occlude()
                .child(content),
        ))
    }
}

/// The triangle on `anchor`'s side of the surface, drawn once the positioner
/// has reported both bounds.
fn arrow_canvas(
    arrow: PopupArrow,
    anchor: Anchor,
    geometry: Rc<Cell<Option<(Bounds<Pixels>, Bounds<Pixels>)>>>,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let Some((surface, trigger)) = geometry.get() else {
                return;
            };
            let (side, target) = arrow_anchor(anchor, trigger);
            let target = match arrow.target {
                ArrowTarget::Anchor => Bounds::new(target, size(px(0.), px(0.))),
                ArrowTarget::TriggerCenter => trigger,
                ArrowTarget::Point(point) => Bounds::new(point, size(px(0.), px(0.))),
            };
            let points = arrow_points(surface, target, side, arrow.depth, arrow.radius);
            let mut fill = PathBuilder::fill();
            fill.move_to(points[0]);
            fill.line_to(points[1]);
            fill.line_to(points[2]);
            fill.close();
            if let Ok(path) = fill.build() {
                window.paint_path(path, arrow.fill);
            }
            // The triangle ends exactly at the surface edge. Cover the ring
            // and the antialiased base on both sides of that edge before
            // drawing its two slopes.
            window.paint_quad(gpui::fill(
                arrow_join_bounds(points, side, px(1.)),
                arrow.fill,
            ));
            if let Some(outline) = arrow.outline {
                let mut path = PathBuilder::stroke(px(1.));
                path.move_to(points[0]);
                path.line_to(points[1]);
                path.line_to(points[2]);
                if let Ok(path) = path.build() {
                    window.paint_path(path, outline);
                }
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

/// The arrow follows the named anchor instead of always aiming at trigger center.
fn arrow_anchor(anchor: Anchor, trigger: Bounds<Pixels>) -> (Placement, Point<Pixels>) {
    match anchor {
        Anchor::TopLeft => (Placement::Bottom, trigger.bottom_left()),
        Anchor::TopCenter => (Placement::Bottom, trigger.bottom_center()),
        Anchor::TopRight => (Placement::Bottom, trigger.bottom_right()),
        Anchor::BottomLeft => (Placement::Top, trigger.origin),
        Anchor::BottomCenter => (Placement::Top, trigger.top_center()),
        Anchor::BottomRight => (Placement::Top, trigger.top_right()),
        Anchor::LeftCenter => (Placement::Right, trigger.right_center()),
        Anchor::RightCenter => (Placement::Left, trigger.left_center()),
    }
}

/// Clamp the arrow base clear of rounded corners while aiming at the trigger.
fn arrow_points(
    surface: Bounds<Pixels>,
    trigger: Bounds<Pixels>,
    side: Placement,
    depth: Pixels,
    radius: Pixels,
) -> [Point<Pixels>; 3] {
    let horizontal = side.is_horizontal();
    let (start, end, target) = if horizontal {
        (surface.top(), surface.bottom(), trigger.center().y)
    } else {
        (surface.left(), surface.right(), trigger.center().x)
    };
    let half = depth.min((end - start) * 0.5);
    let inset = (radius + half).min((end - start) * 0.5);
    let center = target.clamp(start + inset, end - inset);
    match side {
        Placement::Bottom => [
            point(center - half, surface.top()),
            point(center, surface.top() - depth),
            point(center + half, surface.top()),
        ],
        Placement::Top => [
            point(center - half, surface.bottom()),
            point(center, surface.bottom() + depth),
            point(center + half, surface.bottom()),
        ],
        Placement::Right => [
            point(surface.left(), center - half),
            point(surface.left() - depth, center),
            point(surface.left(), center + half),
        ],
        Placement::Left => [
            point(surface.right(), center - half),
            point(surface.right() + depth, center),
            point(surface.right(), center + half),
        ],
    }
}

fn arrow_join_bounds(
    points: [Point<Pixels>; 3],
    side: Placement,
    stroke: Pixels,
) -> Bounds<Pixels> {
    // Inset by the stroke width so the patch remains inside the triangle's
    // slopes at the outer edge of the ring, including on very small surfaces.
    if side.is_horizontal() {
        let inset = stroke.min((points[2].y - points[0].y) * 0.5);
        Bounds::from_corners(
            point(points[0].x - stroke, points[0].y + inset),
            point(points[2].x + stroke, points[2].y - inset),
        )
    } else {
        let inset = stroke.min((points[2].x - points[0].x) * 0.5);
        Bounds::from_corners(
            point(points[0].x + inset, points[0].y - stroke),
            point(points[2].x - inset, points[2].y + stroke),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Render, px};

    #[test]
    fn resolved_corner_preserves_existing_anchor_math() {
        let bounds = Bounds {
            origin: Point::new(px(100.), px(100.)),
            size: gpui::Size::new(px(200.), px(50.)),
        };
        assert_eq!(
            Popup::resolved_corner(Anchor::TopCenter, bounds),
            Point::new(px(200.), px(100.))
        );
        assert_eq!(
            Popup::resolved_corner(Anchor::BottomRight, bounds),
            Point::new(px(300.), px(50.))
        );
    }

    struct Harness;

    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            Popup::new(
                "popup",
                div()
                    .debug_selector(|| "popup-trigger".into())
                    .size(px(100.)),
            )
            .content(
                div()
                    .debug_selector(|| "popup-content".into())
                    .size(px(20.)),
            )
        }
    }

    /// A caller that styles its own surface — a hover card, a dropdown — does
    /// not have to remember to block the mouse. The host does it, so the panel
    /// a popup covers stops reacting to a pointer that is over the popup.
    struct OcclusionHarness {
        background_hovered: Rc<Cell<bool>>,
        content_hovered: Rc<Cell<bool>>,
    }

    impl Render for OcclusionHarness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let background = self.background_hovered.clone();
            let content = self.content_hovered.clone();
            div()
                .relative()
                .size(px(200.))
                .child(
                    div()
                        .id("background")
                        .absolute()
                        .size_full()
                        .on_mouse_move(move |_, _, _| background.set(true)),
                )
                .child(
                    Popup::new("popup", div().size(px(100.))).content(
                        div()
                            .id("content")
                            .size(px(40.))
                            .on_mouse_move(move |_, _, _| content.set(true)),
                    ),
                )
        }
    }

    #[gpui::test]
    fn the_popup_surface_blocks_the_panel_it_covers(cx: &mut gpui::TestAppContext) {
        let background_hovered = Rc::new(Cell::new(false));
        let content_hovered = Rc::new(Cell::new(false));
        let (_, window) = cx.add_window_view({
            let background_hovered = background_hovered.clone();
            let content_hovered = content_hovered.clone();
            move |_, _| OcclusionHarness {
                background_hovered,
                content_hovered,
            }
        });
        window.update(|window, cx| window.draw(cx).clear(cx));
        window.update(|window, cx| window.draw(cx).clear(cx));

        window.simulate_mouse_move(
            gpui::point(px(20.), px(110.)),
            None,
            gpui::Modifiers::default(),
        );
        assert!(!background_hovered.get());
        // The surface blocks what is behind it, not its own content: the
        // hitbox goes in ahead of the children, never over them.
        assert!(content_hovered.get());

        // The same pointer outside the surface still reaches the panel, so the
        // assertion above is about occlusion and not a dead listener.
        window.simulate_mouse_move(
            gpui::point(px(150.), px(180.)),
            None,
            gpui::Modifiers::default(),
        );
        assert!(background_hovered.get());
    }

    /// A popup that may open later.
    struct ClosedHarness {
        open: bool,
    }

    impl Render for ClosedHarness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let popup = Popup::new(
                "popup",
                div()
                    .debug_selector(|| "popup-trigger".into())
                    .size(px(100.)),
            );
            if self.open {
                popup.content(
                    div()
                        .debug_selector(|| "popup-content".into())
                        .size(px(20.)),
                )
            } else {
                popup
            }
        }
    }

    /// A closed popup notes its trigger's bounds without asking for another
    /// frame: a list of rows holding closed popups is not drawn again for
    /// each row that shows. Opened, it places its content at once.
    #[gpui::test]
    fn a_closed_popup_asks_for_no_frame_and_opens_at_once(cx: &mut gpui::TestAppContext) {
        let (view, window) = cx.add_window_view(|_, _| ClosedHarness { open: false });
        window.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert_eq!(window.simulate_next_frame(cx), 0, "no frame was asked for");
        });

        view.update(window, |harness, cx| {
            harness.open = true;
            cx.notify();
        });
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            window.debug_bounds("popup-content").unwrap().size,
            gpui::Size::new(px(20.), px(20.))
        );
    }

    #[gpui::test]
    fn trigger_capture_enables_deferred_content_on_the_next_frame(cx: &mut gpui::TestAppContext) {
        let (_, window) = cx.add_window_view(|_, _| Harness);
        window.update(|window, cx| window.draw(cx).clear(cx));
        window.update(|window, cx| window.draw(cx).clear(cx));

        assert_eq!(
            window.debug_bounds("popup-trigger").unwrap().size,
            gpui::Size::new(px(100.), px(100.))
        );
        assert_eq!(
            window.debug_bounds("popup-content").unwrap().size,
            gpui::Size::new(px(20.), px(20.))
        );
    }

    #[test]
    fn arrow_alignment_uses_the_anchor_instead_of_trigger_center() {
        let trigger = Bounds::new(point(px(120.), px(120.)), size(px(40.), px(20.)));
        for (anchor, side, target) in [
            (
                Anchor::TopLeft,
                Placement::Bottom,
                point(px(120.), px(140.)),
            ),
            (
                Anchor::TopCenter,
                Placement::Bottom,
                point(px(140.), px(140.)),
            ),
            (
                Anchor::TopRight,
                Placement::Bottom,
                point(px(160.), px(140.)),
            ),
            (
                Anchor::BottomLeft,
                Placement::Top,
                point(px(120.), px(120.)),
            ),
            (
                Anchor::BottomCenter,
                Placement::Top,
                point(px(140.), px(120.)),
            ),
            (
                Anchor::BottomRight,
                Placement::Top,
                point(px(160.), px(120.)),
            ),
            (
                Anchor::LeftCenter,
                Placement::Right,
                point(px(160.), px(130.)),
            ),
            (
                Anchor::RightCenter,
                Placement::Left,
                point(px(120.), px(130.)),
            ),
        ] {
            assert_eq!(arrow_anchor(anchor, trigger), (side, target));
        }
    }

    #[test]
    fn arrows_point_toward_the_trigger_on_every_resolved_side() {
        let surface = Bounds::new(point(px(100.), px(100.)), size(px(80.), px(60.)));
        for (side, trigger, tip) in [
            (
                Placement::Bottom,
                Bounds::new(point(px(120.), px(50.)), size(px(40.), px(20.))),
                point(px(140.), px(94.)),
            ),
            (
                Placement::Top,
                Bounds::new(point(px(120.), px(180.)), size(px(40.), px(20.))),
                point(px(140.), px(166.)),
            ),
            (
                Placement::Right,
                Bounds::new(point(px(40.), px(120.)), size(px(40.), px(20.))),
                point(px(94.), px(130.)),
            ),
            (
                Placement::Left,
                Bounds::new(point(px(200.), px(120.)), size(px(40.), px(20.))),
                point(px(186.), px(130.)),
            ),
        ] {
            assert_eq!(arrow_points(surface, trigger, side, px(6.), px(4.))[1], tip);
        }
        let clamped = arrow_points(
            surface,
            Bounds::new(point(px(0.), px(50.)), size(px(20.), px(20.))),
            Placement::Bottom,
            px(6.),
            px(4.),
        );
        assert_eq!(clamped[0], point(px(104.), px(100.)));
        assert_eq!(clamped[1], point(px(110.), px(94.)));
    }

    #[test]
    fn arrow_join_covers_both_sides_of_the_surface_edge() {
        let surface = Bounds::new(point(px(100.), px(100.)), size(px(80.), px(60.)));
        let trigger = Bounds::new(point(px(120.), px(120.)), size(px(40.), px(20.)));
        for (side, expected) in [
            (
                Placement::Bottom,
                Bounds::new(point(px(135.), px(99.)), size(px(10.), px(2.))),
            ),
            (
                Placement::Top,
                Bounds::new(point(px(135.), px(159.)), size(px(10.), px(2.))),
            ),
            (
                Placement::Right,
                Bounds::new(point(px(99.), px(125.)), size(px(2.), px(10.))),
            ),
            (
                Placement::Left,
                Bounds::new(point(px(179.), px(125.)), size(px(2.), px(10.))),
            ),
        ] {
            let points = arrow_points(surface, trigger, side, px(6.), px(4.));
            assert_eq!(arrow_join_bounds(points, side, px(1.)), expected);
        }
    }
}
