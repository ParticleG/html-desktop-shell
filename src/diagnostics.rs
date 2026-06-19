use gtk4::{gio::prelude::ListModelExt, prelude::DisplayExt};

use crate::{assets, config::LoadedConfig, messages};

pub fn run_if_requested(loaded_config: &LoadedConfig) -> Result<bool, String> {
    let diagnostics = loaded_config.diagnostics;
    if !diagnostics.print_capabilities && !diagnostics.print_config && !diagnostics.check {
        return Ok(false);
    }

    if diagnostics.print_capabilities {
        print_capabilities()?;
    }
    if diagnostics.print_config {
        print_config(loaded_config)?;
    }
    if diagnostics.check {
        run_check()?;
    }

    Ok(true)
}

fn print_capabilities() -> Result<(), String> {
    let capabilities = messages::capabilities();
    let json = serde_json::to_string_pretty(&capabilities)
        .map_err(|error| format!("failed to serialize capabilities: {error}"))?;
    println!("{json}");
    Ok(())
}

fn print_config(loaded_config: &LoadedConfig) -> Result<(), String> {
    let config = toml::to_string_pretty(&loaded_config.config)
        .map_err(|error| format!("failed to serialize config: {error}"))?;
    print!("{config}");
    Ok(())
}

fn run_check() -> Result<(), String> {
    gtk4::init().map_err(|error| format!("failed to initialize GTK: {error}"))?;

    if !gtk4_layer_shell::is_supported() {
        return Err("Wayland compositor does not support layer-shell".to_owned());
    }
    println!("layer_shell_supported = true");

    let web_index = assets::web_index_path()?;
    println!("web_index = {}", web_index.display());

    let display =
        gtk4::gdk::Display::default().ok_or_else(|| "missing default GDK display".to_owned())?;
    let monitors = display.monitors();
    let monitor_count = monitors.n_items();
    if monitor_count == 0 {
        return Err("no GDK monitors available".to_owned());
    }
    println!("monitor_count = {monitor_count}");

    Ok(())
}
