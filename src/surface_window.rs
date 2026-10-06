use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;

use crate::{
    bridge,
    config::{PanelKeyboardMode, PanelLayer, ShellConfig},
    providers::ProviderRegistry,
    surface::{SurfaceContext, SurfaceKey, SurfaceKind},
    surface_manager::SurfaceManager,
    web_security::{TRUSTED_ENTRY_URI, TrustedWebContext},
};

const QUICK_PANEL_WIDTH: i32 = 384;
const QUICK_PANEL_HEIGHT: i32 = 480;
const QUICK_PANEL_MARGIN: i32 = 8;
const WORKSPACE_HUD_WIDTH: i32 = 280;
const WORKSPACE_HUD_HEIGHT: i32 = 96;
const DIAGNOSTICS_WIDTH: i32 = 560;
const DIAGNOSTICS_HEIGHT: i32 = 480;

pub struct SurfaceInstance {
    pub context: SurfaceContext,
    pub window: gtk4::ApplicationWindow,
}

impl SurfaceInstance {
    pub const fn key(&self) -> SurfaceKey {
        self.context.key()
    }
}

pub fn surface_window_for_monitor(
    app: &gtk4::Application,
    monitor: &gtk4::gdk::Monitor,
    config: &ShellConfig,
    web_context: &TrustedWebContext,
    providers: ProviderRegistry,
    context: SurfaceContext,
    manager: Weak<RefCell<SurfaceManager>>,
) -> Result<SurfaceInstance, String> {
    let window = gtk4::ApplicationWindow::new(app);
    window.add_css_class("html-desktop-shell-native-surface");
    window.set_title(Some(surface_title(context.kind())));
    window.set_decorated(false);
    window.init_layer_shell();
    window.set_monitor(Some(monitor));
    let namespace = format!(
        "{}-{}",
        context.kind().namespace_prefix(),
        context.monitor_index()
    );
    window.set_namespace(Some(namespace.as_str()));
    configure_surface(&window, context.kind(), config);

    let (web_view, trust) = web_context.secure_web_view();
    web_view.set_background_color(&gtk4::gdk::RGBA::TRANSPARENT);
    web_view.set_hexpand(true);
    web_view.set_vexpand(true);

    install_lifecycle_hooks(&window, &web_view, &context, manager.clone());
    if let Err(message) =
        bridge::attach_bridge(&web_view, trust, providers, context.clone(), manager)
    {
        window.destroy();
        return Err(message.to_owned());
    }

    web_view.connect_load_failed(|_, _event, failing_uri, error| {
        eprintln!("WebKit load failed for {failing_uri}: {error}");
        false
    });
    web_view.load_uri(TRUSTED_ENTRY_URI);
    window.set_child(Some(&web_view));

    Ok(SurfaceInstance { context, window })
}

fn configure_surface(window: &gtk4::ApplicationWindow, kind: SurfaceKind, config: &ShellConfig) {
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        window.set_anchor(edge, false);
        window.set_margin(edge, 0);
    }

    match kind {
        SurfaceKind::Bar => {
            window.set_resizable(true);
            window.set_layer(bar_layer(config.layer));
            window.set_anchor(Edge::Left, true);
            window.set_anchor(Edge::Right, true);
            window.set_anchor(Edge::Top, true);
            window.set_exclusive_zone(config.panel_height);
            window.set_keyboard_mode(bar_keyboard_mode(config.keyboard_mode));
            window.set_default_size(0, config.panel_height);
        }
        SurfaceKind::QuickPanel => {
            window.set_resizable(false);
            window.set_layer(Layer::Overlay);
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Right, true);
            window.set_margin(Edge::Top, config.panel_height + QUICK_PANEL_MARGIN);
            window.set_margin(Edge::Right, QUICK_PANEL_MARGIN);
            window.set_exclusive_zone(0);
            window.set_keyboard_mode(KeyboardMode::OnDemand);
            window.set_default_size(QUICK_PANEL_WIDTH, QUICK_PANEL_HEIGHT);
        }
        SurfaceKind::WorkspaceHud => {
            window.set_resizable(false);
            window.set_layer(Layer::Overlay);
            window.set_exclusive_zone(0);
            window.set_keyboard_mode(KeyboardMode::None);
            window.set_default_size(WORKSPACE_HUD_WIDTH, WORKSPACE_HUD_HEIGHT);
            window.set_can_target(false);
        }
        SurfaceKind::DiagnosticsDialog => {
            window.set_resizable(false);
            window.set_layer(Layer::Overlay);
            window.set_exclusive_zone(0);
            window.set_keyboard_mode(KeyboardMode::OnDemand);
            window.set_default_size(DIAGNOSTICS_WIDTH, DIAGNOSTICS_HEIGHT);
        }
    }
}

fn install_lifecycle_hooks(
    window: &gtk4::ApplicationWindow,
    web_view: &webkit6::WebView,
    context: &SurfaceContext,
    manager: Weak<RefCell<SurfaceManager>>,
) {
    let key = context.key();
    if matches!(
        context.kind(),
        SurfaceKind::QuickPanel | SurfaceKind::DiagnosticsDialog
    ) {
        install_escape_close(window, key, manager.clone());
        install_focus_loss_close(window, key, manager.clone());
    }
    if context.kind() == SurfaceKind::WorkspaceHud {
        web_view.set_can_target(false);
        install_empty_input_region(window, key, manager.clone());
    }

    window.connect_close_request(move |window| {
        SurfaceManager::surface_closed(&manager, key, window);
        glib::Propagation::Proceed
    });
}

fn install_escape_close(
    window: &gtk4::ApplicationWindow,
    key: SurfaceKey,
    manager: Weak<RefCell<SurfaceManager>>,
) {
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let weak_window = window.downgrade();
    controller.connect_key_pressed(move |_, pressed, _, _| {
        if pressed != gtk4::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        if let Some(window) = weak_window.upgrade() {
            SurfaceManager::close_surface_window_from_weak(&manager, key, &window);
        }
        glib::Propagation::Stop
    });
    window.add_controller(controller);
}

fn install_focus_loss_close(
    window: &gtk4::ApplicationWindow,
    key: SurfaceKey,
    manager: Weak<RefCell<SurfaceManager>>,
) {
    let was_active = Rc::new(Cell::new(false));
    window.connect_is_active_notify(move |window| {
        if window.is_active() {
            was_active.set(true);
        } else if was_active.replace(false) {
            SurfaceManager::close_surface_window_from_weak(&manager, key, window);
        }
    });
}

fn install_empty_input_region(
    window: &gtk4::ApplicationWindow,
    key: SurfaceKey,
    manager: Weak<RefCell<SurfaceManager>>,
) {
    window.connect_realize(move |window| {
        let Some(surface) = window.surface() else {
            eprintln!("workspace HUD realized without a GDK surface");
            SurfaceManager::close_surface_window_from_weak(&manager, key, window);
            return;
        };
        if !surface.display().supports_input_shapes() {
            eprintln!("GDK display does not support workspace HUD input regions");
            SurfaceManager::close_surface_window_from_weak(&manager, key, window);
            return;
        }
        surface.set_input_region(Some(&gtk4::cairo::Region::create()));
    });
}

fn surface_title(kind: SurfaceKind) -> &'static str {
    match kind {
        SurfaceKind::Bar => "HTML Desktop Shell",
        SurfaceKind::QuickPanel => "HTML Desktop Shell Quick Panel",
        SurfaceKind::WorkspaceHud => "HTML Desktop Shell Workspace HUD",
        SurfaceKind::DiagnosticsDialog => "HTML Desktop Shell Diagnostics",
    }
}

fn bar_layer(layer: PanelLayer) -> Layer {
    match layer {
        PanelLayer::Top => Layer::Top,
        PanelLayer::Bottom => Layer::Bottom,
        PanelLayer::Overlay => Layer::Overlay,
    }
}

fn bar_keyboard_mode(keyboard_mode: PanelKeyboardMode) -> KeyboardMode {
    match keyboard_mode {
        PanelKeyboardMode::None => KeyboardMode::None,
        PanelKeyboardMode::OnDemand => KeyboardMode::OnDemand,
        PanelKeyboardMode::Exclusive => KeyboardMode::Exclusive,
    }
}
