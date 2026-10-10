use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde::Deserialize;
use webkit6::prelude::*;

use crate::{messages::NativeMethodError, surface::SurfaceKind, web_security::WebViewTrust};

pub(crate) const MENU_WIDTH: i32 = 360;
pub(crate) const INITIAL_MENU_HEIGHT: i32 = 160;
pub(crate) const MENU_GAP: i32 = 4;
const MAX_DOM_EXTENT: f64 = 1_000_000.0;
const MAX_QUERY_BYTES: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MenuAnchor {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy)]
pub(crate) enum MenuAnchorSpace {
    Parent,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MenuPlacement {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub(crate) fn menu_placement(
    anchor: MenuAnchor,
    output_width: i32,
    output_height: i32,
    desired_height: i32,
) -> MenuPlacement {
    let output_width = output_width.max(1);
    let output_height = output_height.max(1);
    let horizontal_inset = MENU_GAP.min((output_width - 1) / 2);
    let vertical_inset = MENU_GAP.min((output_height - 1) / 2);
    let width = MENU_WIDTH.min(output_width - horizontal_inset * 2).max(1);
    let top = anchor
        .y
        .clamp(vertical_inset, output_height - vertical_inset);
    let bottom = anchor
        .y
        .saturating_add(anchor.height)
        .clamp(vertical_inset, output_height - vertical_inset);
    let below_y = bottom
        .saturating_add(MENU_GAP)
        .min(output_height - vertical_inset);
    let above_end = top.saturating_sub(MENU_GAP).max(vertical_inset);
    let below = output_height - vertical_inset - below_y;
    let above = above_end - vertical_inset;
    let desired_height = desired_height.max(1);
    let (y, height) = if below > 0 && (desired_height <= below || below >= above) {
        (below_y, desired_height.min(below))
    } else if above > 0 {
        let height = desired_height.min(above);
        (above_end - height, height)
    } else {
        (
            vertical_inset,
            desired_height
                .min(output_height - vertical_inset * 2)
                .max(1),
        )
    };
    MenuPlacement {
        x: anchor
            .x
            .clamp(horizontal_inset, output_width - horizontal_inset - width),
        y,
        width,
        height,
    }
}

pub(crate) fn apply_placement(
    window: &gtk4::ApplicationWindow,
    placement: MenuPlacement,
    bounds_width: i32,
) {
    window.set_anchor(Edge::Left, false);
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, false);
    window.set_margin(Edge::Left, 0);
    window.set_margin(Edge::Right, bounds_width - placement.x - placement.width);
    window.set_margin(Edge::Top, placement.y);
    window.set_margin(Edge::Bottom, 0);
    window.set_default_size(placement.width, placement.height);
    window.set_size_request(placement.width, placement.height);
}

pub(crate) fn popover_size(
    bounds_width: i32,
    bounds_height: i32,
    desired_height: i32,
) -> (i32, i32) {
    let bounds_width = bounds_width.max(1);
    let bounds_height = bounds_height.max(1);
    let horizontal_inset = MENU_GAP.min((bounds_width - 1) / 2);
    let vertical_inset = MENU_GAP.min((bounds_height - 1) / 2);
    (
        MENU_WIDTH.min(bounds_width - horizontal_inset * 2).max(1),
        desired_height
            .max(1)
            .min(bounds_height - vertical_inset * 2),
    )
}

pub(crate) fn apply_popover_size(
    popover: &gtk4::Popover,
    content: &crate::surface_window::TrayMenuContent,
    size: (i32, i32),
) {
    // Natural size, not a hard minimum: the compositor can constrain a popup.
    content.set_preferred_size(size);
    if popover.is_mapped() {
        popover.present();
    }
}

pub(crate) fn anchor_in_workarea(
    anchor: MenuAnchor,
    output_width: i32,
    workarea_width: i32,
    workarea_height: i32,
) -> Option<MenuAnchor> {
    if workarea_width <= 0 || workarea_width > output_width || workarea_height <= 0 {
        return None;
    }
    let x = anchor.x - (output_width - workarea_width);
    let right = x.saturating_add(anchor.width).min(workarea_width);
    let bottom = anchor.y.saturating_add(anchor.height).min(workarea_height);
    let x = x.max(0);
    let y = anchor.y.max(0);
    (right > x && bottom > y).then_some(MenuAnchor {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}

#[derive(Clone, Copy)]
enum NativeTrigger {
    Pointer(f64, f64),
    Keyboard,
}

#[derive(Clone, Default)]
pub(crate) struct TriggerCapture(Rc<RefCell<Option<(Instant, NativeTrigger)>>>);

impl TriggerCapture {
    pub(crate) fn install(
        window: &gtk4::ApplicationWindow,
        web_view: &webkit6::WebView,
        trust: WebViewTrust,
        kind: SurfaceKind,
    ) -> Self {
        let capture = Self::default();
        let state = capture.0.clone();
        let controller = gtk4::EventControllerLegacy::new();
        controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let weak_view = web_view.downgrade();
        controller.connect_event(move |_, event| {
            let trigger = match event.event_type() {
                gtk4::gdk::EventType::ButtonPress
                | gtk4::gdk::EventType::ButtonRelease
                | gtk4::gdk::EventType::TouchBegin => {
                    event.position().map(|(x, y)| NativeTrigger::Pointer(x, y))
                }
                gtk4::gdk::EventType::KeyPress | gtk4::gdk::EventType::KeyRelease => {
                    Some(NativeTrigger::Keyboard)
                }
                _ => None,
            };
            if let Some(trigger) = trigger {
                *state.borrow_mut() = Some((Instant::now(), trigger));
            }
            if kind == SurfaceKind::Bar
                && event.event_type() == gtk4::gdk::EventType::KeyPress
                && event
                    .downcast_ref::<gtk4::gdk::KeyEvent>()
                    .is_some_and(|key| {
                        key.keyval() == gtk4::gdk::Key::Menu
                            || (key.keyval() == gtk4::gdk::Key::F10
                                && key
                                    .modifier_state()
                                    .contains(gtk4::gdk::ModifierType::SHIFT_MASK))
                    })
            {
                // WebKit's hidden GtkTextView otherwise opens its editor menu first.
                // Forward only this shell shortcut to the current trusted tray control.
                if let Some(view) = weak_view.upgrade()
                    && trust.allows_bridge(&view)
                {
                    view.evaluate_javascript(
                        r#"(() => {
                            const control = document.activeElement?.closest('button[data-tray-id]');
                            if (control) control.dispatchEvent(new KeyboardEvent('keydown', {
                                key: 'ContextMenu', bubbles: true, cancelable: true
                            }));
                        })()"#,
                        None,
                        None,
                        None::<&gtk4::gio::Cancellable>,
                        |_| {},
                    );
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(controller);
        capture
    }

    fn pointer(&self) -> Option<(f64, f64)> {
        match *self.0.borrow() {
            Some((when, NativeTrigger::Pointer(x, y)))
                if when.elapsed() < Duration::from_secs(2) =>
            {
                Some((x, y))
            }
            _ => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DomAnchor {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    viewport_width: f64,
    viewport_height: f64,
}

fn anchor_error() -> NativeMethodError {
    NativeMethodError::new(
        "tray_anchor_unavailable",
        "The tray menu trigger has no visible trusted native control anchor",
    )
}

fn parse_query<T: for<'de> Deserialize<'de>>(value: javascriptcore6::Value) -> Option<T> {
    let json = value.to_json(0)?;
    if json.len() > MAX_QUERY_BYTES {
        return None;
    }
    serde_json::from_str(json.as_str()).ok()
}

fn bounded_extent(value: f64) -> bool {
    value.is_finite() && value > 0.0 && value <= MAX_DOM_EXTENT
}

fn transform_anchor(
    dom: DomAnchor,
    view_width: f64,
    view_height: f64,
    origin_x: f64,
    origin_y: f64,
    output_width: i32,
    output_height: i32,
) -> Option<MenuAnchor> {
    if ![
        dom.width,
        dom.height,
        dom.viewport_width,
        dom.viewport_height,
        view_width,
        view_height,
    ]
    .into_iter()
    .all(bounded_extent)
        || !dom.x.is_finite()
        || !dom.y.is_finite()
        || !origin_x.is_finite()
        || !origin_y.is_finite()
        || output_width <= 0
        || output_height <= 0
    {
        return None;
    }
    let left = dom.x.max(0.0);
    let top = dom.y.max(0.0);
    let right = (dom.x + dom.width).min(dom.viewport_width);
    let bottom = (dom.y + dom.height).min(dom.viewport_height);
    if right <= left || bottom <= top {
        return None;
    }
    // GTK/GDK dimensions are logical, not device pixels. This ratio also handles WebKit page zoom.
    let x_scale = view_width / dom.viewport_width;
    let y_scale = view_height / dom.viewport_height;
    let x = (origin_x + left * x_scale).floor().max(0.0);
    let y = (origin_y + top * y_scale).floor().max(0.0);
    let right = (origin_x + right * x_scale)
        .ceil()
        .min(f64::from(output_width));
    let bottom = (origin_y + bottom * y_scale)
        .ceil()
        .min(f64::from(output_height));
    if right <= x || bottom <= y {
        return None;
    }
    Some(MenuAnchor {
        x: x as i32,
        y: y as i32,
        width: (right - x) as i32,
        height: (bottom - y) as i32,
    })
}

pub(crate) struct NativeMenuTrigger<'a> {
    pub web_view: &'a webkit6::WebView,
    pub window: &'a gtk4::ApplicationWindow,
    pub trust: WebViewTrust,
    pub capture: &'a TriggerCapture,
}

pub(crate) fn capture_anchor(
    trigger: NativeMenuTrigger<'_>,
    monitor: &gtk4::gdk::Monitor,
    space: MenuAnchorSpace,
    item_id: &str,
    cancellable: &gtk4::gio::Cancellable,
    callback: impl FnOnce(Result<MenuAnchor, NativeMethodError>) + 'static,
) {
    let NativeMenuTrigger {
        web_view,
        window,
        trust,
        capture,
    } = trigger;
    let Some(bounds) = web_view.compute_bounds(window) else {
        callback(Err(anchor_error()));
        return;
    };
    let (origin_x, origin_y, bounds_width, bounds_height) = match space {
        MenuAnchorSpace::Parent => (
            f64::from(bounds.x()),
            f64::from(bounds.y()),
            window.width(),
            window.height(),
        ),
        MenuAnchorSpace::Output => {
            let geometry = monitor.geometry();
            let x = if window.is_anchor(Edge::Left) {
                window.margin(Edge::Left)
            } else {
                geometry.width() - window.width() - window.margin(Edge::Right)
            };
            (
                f64::from(x) + f64::from(bounds.x()),
                f64::from(window.margin(Edge::Top)) + f64::from(bounds.y()),
                geometry.width(),
                geometry.height(),
            )
        }
    };
    if !trust.allows_bridge(web_view)
        || !window.is_layer_window()
        || window.width() <= 0
        || window.height() <= 0
        || !window.is_anchor(Edge::Top)
        || (!window.is_anchor(Edge::Left) && !window.is_anchor(Edge::Right))
    {
        callback(Err(anchor_error()));
        return;
    }
    let view_width = f64::from(bounds.width());
    let view_height = f64::from(bounds.height());
    let pointer = capture
        .pointer()
        .map(|(x, y)| (x - f64::from(bounds.x()), y - f64::from(bounds.y())));
    let args = serde_json::json!({"itemId": item_id, "pointer": pointer, "nativeWidth": view_width, "nativeHeight": view_height});
    let script = format!(
        r#"(() => {{
        const args = {args};
        const roots = Array.from(document.querySelectorAll('[data-tray-id]'));
        if (roots.length > 4096) return null;
        const rootsForItem = roots.filter(el => el.dataset.trayId === args.itemId);
        if (rootsForItem.length !== 1) return null;
        const root = rootsForItem[0];
        const viewport = window.visualViewport;
        const vw = viewport?.width ?? window.innerWidth;
        const vh = viewport?.height ?? window.innerHeight;
        const vx = viewport?.offsetLeft ?? 0;
        const vy = viewport?.offsetTop ?? 0;
        let control = document.activeElement?.closest('button');
        if (!control || !root.contains(control)) control = null;
        if (args.pointer) {{
            const pointed = document.elementFromPoint(args.pointer[0] * vw / args.nativeWidth + vx,
                args.pointer[1] * vh / args.nativeHeight + vy)?.closest('button');
            if (pointed && root.contains(pointed)) control = pointed;
        }}
        if (!control) control = root.matches('button') ? root : root.querySelector('button[data-tray-menu-trigger]');
        if (!control) return null;
        const rect = control.getBoundingClientRect();
        return {{x: rect.left - vx, y: rect.top - vy, width: rect.width, height: rect.height,
            viewportWidth: vw, viewportHeight: vh}};
    }})()"#
    );
    let weak_view = web_view.downgrade();
    let weak_window = window.downgrade();
    let expected_width = web_view.width();
    let expected_height = web_view.height();
    web_view.evaluate_javascript(&script, None, None, Some(cancellable), move |result| {
        let current = weak_view.upgrade().zip(weak_window.upgrade());
        let Some(_) = current.filter(|(view, window)| {
            trust.allows_bridge(view)
                && window.is_visible()
                && view.width() == expected_width
                && view.height() == expected_height
        }) else {
            callback(Err(anchor_error()));
            return;
        };
        let anchor = result
            .ok()
            .and_then(parse_query::<DomAnchor>)
            .and_then(|dom| {
                transform_anchor(
                    dom,
                    view_width,
                    view_height,
                    origin_x,
                    origin_y,
                    bounds_width,
                    bounds_height,
                )
            })
            .ok_or_else(anchor_error);
        callback(anchor);
    });
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContentSize {
    height: f64,
    viewport_height: f64,
}

pub(crate) fn measure_content(
    web_view: &webkit6::WebView,
    trust: WebViewTrust,
    cancellable: &gtk4::gio::Cancellable,
    callback: impl FnOnce(Option<i32>) + 'static,
) {
    if !trust.allows_bridge(web_view) {
        callback(None);
        return;
    }
    const QUERY: &str = r#"(() => {
        const content = document.getElementById('tray-menu-content');
        if (!content) return null;
        const rect = content.getBoundingClientRect();
        const style = getComputedStyle(content);
        const margin = (parseFloat(style.marginBottom) || 0);
        const border = Math.max(0, rect.height - content.clientHeight);
        return {height: Math.max(0, rect.top) + content.scrollHeight + margin + border,
            viewportHeight: window.visualViewport?.height ?? window.innerHeight};
    })()"#;
    let logical_height = web_view.height();
    let weak_view = web_view.downgrade();
    web_view.evaluate_javascript(QUERY, None, None, Some(cancellable), move |result| {
        if weak_view
            .upgrade()
            .is_none_or(|view| !trust.allows_bridge(&view) || view.height() != logical_height)
        {
            callback(None);
            return;
        }
        let height = result
            .ok()
            .and_then(parse_query::<ContentSize>)
            .and_then(|size| {
                if logical_height <= 0
                    || !bounded_extent(size.height)
                    || !bounded_extent(size.viewport_height)
                {
                    return None;
                }
                Some(
                    (size.height * f64::from(logical_height) / size.viewport_height)
                        .ceil()
                        .clamp(1.0, MAX_DOM_EXTENT) as i32,
                )
            });
        callback(height);
    });
}

pub(crate) fn dismiss_window(
    app: &gtk4::Application,
    monitor: &gtk4::gdk::Monitor,
    monitor_index: u32,
    dismiss: impl Fn() + 'static,
) -> gtk4::ApplicationWindow {
    let window = gtk4::ApplicationWindow::new(app);
    window.add_css_class("html-desktop-shell-native-surface");
    window.set_title(Some("HTML Desktop Shell Tray Dismiss"));
    window.set_decorated(false);
    window.init_layer_shell();
    window.set_monitor(Some(monitor));
    window.set_namespace(Some(&format!(
        "html-desktop-shell-tray-dismiss-{monitor_index}"
    )));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::None);
    // -1 reserves no space and bypasses other clients' exclusive areas.
    window.set_exclusive_zone(-1);
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        window.set_anchor(edge, true);
        window.set_margin(edge, 0);
    }
    let geometry = monitor.geometry();
    window.set_default_size(geometry.width().max(1), geometry.height().max(1));
    let target = gtk4::DrawingArea::new();
    target.set_hexpand(true);
    target.set_vexpand(true);
    // A fully empty snapshot never submits a Wayland buffer and cannot capture input.
    target.set_draw_func(|_, context, _, _| {
        context.set_operator(gtk4::cairo::Operator::Source);
        context.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        let _ = context.paint();
    });
    window.set_child(Some(&target));
    let dismiss = Rc::new(dismiss);
    let click_dismiss = dismiss.clone();
    let controller = gtk4::EventControllerLegacy::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    controller.connect_event(move |_, event| {
        if matches!(
            event.event_type(),
            gtk4::gdk::EventType::ButtonPress | gtk4::gdk::EventType::TouchBegin
        ) {
            click_dismiss();
        }
        glib::Propagation::Stop
    });
    window.add_controller(controller);
    let close_dismiss = dismiss.clone();
    window.connect_close_request(move |_| {
        close_dismiss();
        glib::Propagation::Proceed
    });
    window.connect_unrealize(move |_| dismiss());
    window
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_fits_both_axes_on_small_outputs_and_near_every_edge() {
        for (width, height) in [(1, 1), (180, 90), (1920, 1080), (1080, 1920)] {
            for (x, y) in [
                (0, 0),
                (width - 1, 0),
                (0, height - 1),
                (width - 1, height - 1),
            ] {
                let menu = menu_placement(
                    MenuAnchor {
                        x,
                        y,
                        width: 24,
                        height: 24,
                    },
                    width,
                    height,
                    9000,
                );
                assert!(menu.x >= 0 && menu.y >= 0);
                assert!(menu.width > 0 && menu.height > 0);
                assert!(menu.x + menu.width <= width);
                assert!(menu.y + menu.height <= height);
            }
        }
    }

    #[test]
    fn menu_is_tight_to_control_and_flips_above_when_below_cannot_fit() {
        let below = menu_placement(
            MenuAnchor {
                x: 600,
                y: 4,
                width: 24,
                height: 24,
            },
            1280,
            720,
            300,
        );
        assert_eq!((below.x, below.y, below.height), (600, 32, 300));
        let above = menu_placement(
            MenuAnchor {
                x: 1260,
                y: 600,
                width: 20,
                height: 30,
            },
            1280,
            720,
            300,
        );
        assert_eq!((above.x, above.y, above.height), (916, 296, 300));
    }

    #[test]
    fn quick_panel_anchor_uses_native_workarea_right_edge_without_foreign_offset_guesses() {
        let anchor = MenuAnchor {
            x: 1680,
            y: 120,
            width: 80,
            height: 32,
        };
        let native = anchor_in_workarea(anchor, 1920, 1760, 1000).unwrap();
        assert_eq!(
            native,
            MenuAnchor {
                x: 1520,
                y: 120,
                width: 80,
                height: 32
            }
        );
        // Right inset stays 160 logical units in both domains, regardless of which foreign edge reserved space.
        assert_eq!(
            1920 - anchor.x - anchor.width,
            1760 - native.x - native.width
        );
        assert!(anchor_in_workarea(MenuAnchor { y: 1100, ..anchor }, 1920, 1760, 1000).is_none());
        assert!(anchor_in_workarea(anchor, 1920, 0, 1000).is_none());
    }

    #[test]
    fn parent_local_anchor_clips_scaled_control_to_native_parent_bounds() {
        let native = transform_anchor(
            DomAnchor {
                x: 310.0,
                y: 14.0,
                width: 24.0,
                height: 4.0,
                viewport_width: 320.0,
                viewport_height: 16.0,
            },
            640.0,
            32.0,
            0.0,
            0.0,
            640,
            32,
        )
        .unwrap();
        assert_eq!(
            native,
            MenuAnchor {
                x: 620,
                y: 28,
                width: 20,
                height: 4,
            }
        );
    }

    #[test]
    fn popup_size_bounds_tall_content_but_allows_natural_height_collapse() {
        for (width, height) in [(1, 1), (180, 90), (1920, 1080)] {
            let size = popover_size(width, height, 9000);
            assert!(size.0 > 0 && size.0 <= width);
            assert!(size.1 > 0 && size.1 <= height);
        }
        let expanded = popover_size(1280, 720, 300);
        let collapsed = popover_size(1280, 720, 64);
        assert_eq!(expanded.1, 300);
        assert_eq!(collapsed.1, 64);
        assert_eq!(expanded.0, collapsed.0);
    }

    #[test]
    fn trusted_dom_geometry_transforms_css_zoom_and_rejects_hidden_invalid_controls() {
        let dom = DomAnchor {
            x: 100.0,
            y: 4.0,
            width: 24.0,
            height: 20.0,
            viewport_width: 640.0,
            viewport_height: 40.0,
        };
        let native = transform_anchor(dom, 1280.0, 80.0, 0.0, 0.0, 1920, 1080).unwrap();
        assert_eq!(
            native,
            MenuAnchor {
                x: 200,
                y: 8,
                width: 48,
                height: 40
            }
        );
        for (x, width, viewport_width) in [
            (f64::NAN, 24.0, 640.0),
            (800.0, 24.0, 640.0),
            (2.0, 0.0, 640.0),
            (2.0, 24.0, f64::INFINITY),
        ] {
            assert!(
                transform_anchor(
                    DomAnchor {
                        x,
                        y: 0.0,
                        width,
                        height: 20.0,
                        viewport_width,
                        viewport_height: 40.0
                    },
                    1280.0,
                    80.0,
                    0.0,
                    0.0,
                    1920,
                    1080
                )
                .is_none()
            );
        }
    }
}
