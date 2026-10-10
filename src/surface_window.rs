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
    tray_overlay::{self, MenuAnchor},
    web_security::{TRUSTED_ENTRY_URI, TrustedWebContext, WebViewTrust},
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
    pub trust: WebViewTrust,
}

impl SurfaceInstance {
    pub const fn key(&self) -> SurfaceKey {
        self.context.key()
    }
}

mod tray_menu_content {
    use std::cell::Cell;

    use gtk4::{prelude::*, subclass::prelude::*};

    #[derive(Default)]
    pub struct TrayMenuContent {
        pub size: Cell<(i32, i32)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TrayMenuContent {
        const NAME: &'static str = "HtmlDesktopShellTrayMenuContent";
        type Type = super::TrayMenuContent;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for TrayMenuContent {
        fn dispose(&self) {
            if let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for TrayMenuContent {
        fn measure(&self, orientation: gtk4::Orientation, _: i32) -> (i32, i32, i32, i32) {
            let size = self.size.get();
            let natural = if orientation == gtk4::Orientation::Horizontal {
                size.0
            } else {
                size.1
            };
            (1, natural.max(1), -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.obj().first_child() {
                child.allocate(width, height, baseline, None);
            }
        }

        fn focus(&self, direction: gtk4::DirectionType) -> bool {
            self.obj()
                .first_child()
                .is_some_and(|child| child.child_focus(direction))
        }

        fn grab_focus(&self) -> bool {
            self.obj()
                .first_child()
                .is_some_and(|child| child.grab_focus())
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            if let Some(child) = self.obj().first_child() {
                self.obj().snapshot_child(&child, snapshot);
            }
        }
    }
}

glib::wrapper! {
    pub struct TrayMenuContent(ObjectSubclass<tray_menu_content::TrayMenuContent>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl TrayMenuContent {
    fn new(view: &webkit6::WebView) -> Self {
        let content: Self = glib::Object::new();
        view.set_parent(&content);
        content
    }

    pub(crate) fn set_preferred_size(&self, size: (i32, i32)) {
        use gtk4::subclass::prelude::ObjectSubclassIsExt;

        self.imp().size.set(size);
        self.queue_resize();
    }
}

#[derive(Clone)]
pub(crate) enum TrayMenuPresentation {
    LayerWindow(gtk4::ApplicationWindow),
    ParentPopup {
        popover: gtk4::Popover,
        content: TrayMenuContent,
    },
}

impl TrayMenuPresentation {
    pub(crate) fn widget(&self) -> &gtk4::Widget {
        match self {
            Self::LayerWindow(window) => window.upcast_ref(),
            Self::ParentPopup { popover, .. } => popover.upcast_ref(),
        }
    }

    pub(crate) fn layer_window(&self) -> Option<&gtk4::ApplicationWindow> {
        match self {
            Self::LayerWindow(window) => Some(window),
            Self::ParentPopup { .. } => None,
        }
    }

    pub(crate) fn retire(self) {
        match self {
            Self::LayerWindow(window) => window.destroy(),
            Self::ParentPopup { popover, .. } => {
                popover.popdown();
                popover.set_child(gtk4::Widget::NONE);
                popover.unparent();
            }
        }
    }
}

pub(crate) struct TrayMenuInstance {
    pub context: SurfaceContext,
    pub presentation: TrayMenuPresentation,
    pub web_view: webkit6::WebView,
    pub trust: WebViewTrust,
}

impl TrayMenuInstance {
    pub(crate) fn is_current(
        &self,
        context: &SurfaceContext,
        presentation: &gtk4::Widget,
        view: &webkit6::WebView,
    ) -> bool {
        let child = match &self.presentation {
            TrayMenuPresentation::LayerWindow(window) => window.child(),
            TrayMenuPresentation::ParentPopup { popover, content }
                if popover.child().as_ref() == Some(content.upcast_ref()) =>
            {
                content.first_child()
            }
            TrayMenuPresentation::ParentPopup { .. } => None,
        };
        self.context == *context
            && self.presentation.widget() == presentation
            && self.web_view == *view
            && child.as_ref() == Some(view.upcast_ref())
    }
}

pub(crate) fn tray_popover_for_parent(
    parent: &gtk4::ApplicationWindow,
    web_context: &TrustedWebContext,
    providers: ProviderRegistry,
    context: SurfaceContext,
    anchor: MenuAnchor,
    size: (i32, i32),
    manager: Weak<RefCell<SurfaceManager>>,
) -> Result<TrayMenuInstance, String> {
    let popover = gtk4::Popover::new();
    popover.set_widget_name(&format!(
        "html-desktop-shell-tray-menu-{}",
        context.monitor_index()
    ));
    popover.add_css_class("html-desktop-shell-tray-popover");
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(gtk4::PositionType::Bottom);
    popover.set_offset(0, tray_overlay::MENU_GAP);
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
        anchor.x,
        anchor.y,
        anchor.width,
        anchor.height,
    )));
    popover.set_parent(parent);

    let (web_view, trust) = web_context.secure_web_view();
    web_view.set_background_color(&gtk4::gdk::RGBA::TRANSPARENT);
    web_view.set_hexpand(true);
    web_view.set_vexpand(true);
    let content = TrayMenuContent::new(&web_view);
    popover.set_child(Some(&content));
    tray_overlay::apply_popover_size(&popover, &content, size);
    if let Err(message) = bridge::attach_bridge(
        &web_view,
        parent,
        popover.upcast_ref(),
        trust.clone(),
        providers,
        context.clone(),
        manager.clone(),
    ) {
        popover.set_child(gtk4::Widget::NONE);
        popover.unparent();
        return Err(message.to_owned());
    }

    let close_manager = manager.clone();
    let close_context = context.clone();
    popover.connect_closed(move |popover| {
        SurfaceManager::retire_tray_menu_presentation(
            &close_manager,
            &close_context,
            popover.upcast_ref(),
        );
    });
    let unrealize_manager = manager.clone();
    let unrealize_context = context.clone();
    popover.connect_unrealize(move |popover| {
        SurfaceManager::retire_tray_menu_presentation(
            &unrealize_manager,
            &unrealize_context,
            popover.upcast_ref(),
        );
    });
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let weak_popover = popover.downgrade();
    let escape_manager = manager.clone();
    let escape_context = context.clone();
    controller.connect_key_pressed(move |_, pressed, _, _| {
        if pressed != gtk4::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        if let Some(popover) = weak_popover.upgrade() {
            SurfaceManager::retire_tray_menu_presentation(
                &escape_manager,
                &escape_context,
                popover.upcast_ref(),
            );
        }
        glib::Propagation::Stop
    });
    popover.add_controller(controller);
    web_view.connect_load_failed(|_, _event, failing_uri, error| {
        eprintln!("WebKit load failed for {failing_uri}: {error}");
        false
    });
    Ok(TrayMenuInstance {
        context,
        presentation: TrayMenuPresentation::ParentPopup { popover, content },
        web_view,
        trust,
    })
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
    if let Err(message) = bridge::attach_bridge(
        &web_view,
        &window,
        window.upcast_ref(),
        trust.clone(),
        providers,
        context.clone(),
        manager,
    ) {
        window.destroy();
        return Err(message.to_owned());
    }

    web_view.connect_load_failed(|_, _event, failing_uri, error| {
        eprintln!("WebKit load failed for {failing_uri}: {error}");
        false
    });
    if context.kind() != SurfaceKind::TrayMenu {
        web_view.load_uri(TRUSTED_ENTRY_URI);
    }
    window.set_child(Some(&web_view));

    Ok(SurfaceInstance {
        context,
        window,
        trust,
    })
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
        SurfaceKind::TrayMenu => {
            window.set_resizable(false);
            window.set_layer(Layer::Overlay);
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Left, true);
            window.set_exclusive_zone(-1);
            window.set_keyboard_mode(KeyboardMode::Exclusive);
            window.set_default_size(
                crate::tray_overlay::MENU_WIDTH,
                crate::tray_overlay::INITIAL_MENU_HEIGHT,
            );
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
        SurfaceKind::QuickPanel | SurfaceKind::DiagnosticsDialog | SurfaceKind::TrayMenu
    ) {
        install_escape_close(window, key, manager.clone());
        install_focus_loss_close(window, key, manager.clone());
    }
    if context.kind() == SurfaceKind::WorkspaceHud {
        web_view.set_can_target(false);
        install_empty_input_region(window, key, manager.clone());
    }

    let unrealize_manager = manager.clone();
    window.connect_unrealize(move |window| {
        SurfaceManager::surface_closed(&unrealize_manager, key, window);
    });
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
        } else if was_active.replace(false)
            && !SurfaceManager::menu_suspends_focus_loss(&manager, window)
        {
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
        SurfaceKind::TrayMenu => "HTML Desktop Shell Tray Menu",
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
