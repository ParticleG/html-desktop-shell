use std::{env, path::PathBuf};

use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;

use crate::bridge;

const PANEL_HEIGHT: i32 = 32;
const PANEL_NAMESPACE: &str = "html-desktop-shell-panel";

pub fn shell_window_new(app: &gtk4::Application) -> Result<gtk4::ApplicationWindow, String> {
    if !gtk4_layer_shell::is_supported() {
        return Err("Wayland compositor does not support layer-shell".to_owned());
    }

    let window = gtk4::ApplicationWindow::new(app);
    window.set_title(Some("HTML Desktop Shell"));
    window.set_decorated(false);
    window.set_resizable(true);

    window.init_layer_shell();
    window.set_namespace(Some(PANEL_NAMESPACE));
    window.set_layer(Layer::Top);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, false);
    window.set_margin(Edge::Top, 0);
    window.set_exclusive_zone(PANEL_HEIGHT);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_default_size(0, PANEL_HEIGHT);

    let web_view = webkit6::WebView::new();
    web_view.set_hexpand(true);
    web_view.set_vexpand(true);
    if let Err(message) = bridge::attach_bridge(&web_view) {
        eprintln!("{message}");
    }

    let html_path = web_index_path()?;

    let uri = glib::filename_to_uri(&html_path, None).map_err(|error| {
        format!(
            "failed to create file URI for {}: {error}",
            html_path.display()
        )
    })?;

    web_view.connect_load_failed(|_, _event, failing_uri, error| {
        eprintln!("WebKit load failed for {failing_uri}: {error}");
        false
    });
    web_view.load_uri(uri.as_str());
    window.set_child(Some(&web_view));

    Ok(window)
}

fn web_index_path() -> Result<PathBuf, String> {
    let cwd_path = env::current_dir()
        .map_err(|error| format!("failed to resolve current directory: {error}"))?
        .join("web/index.html");
    if cwd_path.exists() {
        return Ok(cwd_path);
    }

    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/index.html");
    if manifest_path.exists() {
        return Ok(manifest_path);
    }

    Err(format!(
        "missing web/index.html: checked {} and {}",
        cwd_path.display(),
        manifest_path.display()
    ))
}
