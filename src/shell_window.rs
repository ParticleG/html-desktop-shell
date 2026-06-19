use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;

use crate::{
    bridge,
    config::{PanelKeyboardMode, PanelLayer, ShellConfig},
};
const PANEL_NAMESPACE_PREFIX: &str = "html-desktop-shell-panel";

pub fn shell_window_for_monitor(
    app: &gtk4::Application,
    monitor: &gtk4::gdk::Monitor,
    index: u32,
    uri: &str,
    config: &ShellConfig,
) -> Result<gtk4::ApplicationWindow, String> {
    let window = gtk4::ApplicationWindow::new(app);
    window.set_title(Some("HTML Desktop Shell"));
    window.set_decorated(false);
    window.set_resizable(true);

    window.init_layer_shell();
    window.set_monitor(Some(monitor));
    let namespace = format!("{PANEL_NAMESPACE_PREFIX}-{index}");
    window.set_namespace(Some(namespace.as_str()));
    window.set_layer(layer(config.layer));
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, false);
    window.set_margin(Edge::Top, 0);
    window.set_exclusive_zone(config.panel_height);
    window.set_keyboard_mode(keyboard_mode(config.keyboard_mode));
    window.set_default_size(0, config.panel_height);

    let web_view = webkit6::WebView::new();
    if let Some(settings) = webkit6::prelude::WebViewExt::settings(&web_view) {
        settings.set_allow_file_access_from_file_urls(true);
    }
    web_view.set_hexpand(true);
    web_view.set_vexpand(true);
    if let Err(message) = bridge::attach_bridge(&web_view) {
        eprintln!("{message}");
    }

    web_view.connect_load_failed(|_, _event, failing_uri, error| {
        eprintln!("WebKit load failed for {failing_uri}: {error}");
        false
    });
    web_view.load_uri(uri);
    window.set_child(Some(&web_view));

    Ok(window)
}

fn layer(layer: PanelLayer) -> Layer {
    match layer {
        PanelLayer::Top => Layer::Top,
        PanelLayer::Bottom => Layer::Bottom,
        PanelLayer::Overlay => Layer::Overlay,
    }
}

fn keyboard_mode(keyboard_mode: PanelKeyboardMode) -> KeyboardMode {
    match keyboard_mode {
        PanelKeyboardMode::None => KeyboardMode::None,
        PanelKeyboardMode::OnDemand => KeyboardMode::OnDemand,
        PanelKeyboardMode::Exclusive => KeyboardMode::Exclusive,
    }
}
