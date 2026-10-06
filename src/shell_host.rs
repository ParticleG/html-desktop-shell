use std::{cell::RefCell, rc::Rc};

use glib::prelude::ObjectExt;
use gtk4::prelude::*;

use crate::{
    assets, config::ShellConfig, providers::ProviderRegistry, surface_manager::SurfaceManager,
    web_security::TrustedWebContext,
};

pub struct ShellHost {
    manager: Rc<RefCell<SurfaceManager>>,
    monitors: gtk4::gio::ListModel,
    monitor_changed_handler: Option<glib::SignalHandlerId>,
    _application_hold: gtk4::gio::ApplicationHoldGuard,
}

impl ShellHost {
    pub fn new(app: &gtk4::Application, config: ShellConfig) -> Result<Self, String> {
        if !gtk4_layer_shell::is_supported() {
            return Err("Wayland compositor does not support layer-shell".to_owned());
        }

        let display = gtk4::gdk::Display::default()
            .ok_or_else(|| "missing default GDK display".to_owned())?;
        install_native_surface_style(&display);
        let monitors = display.monitors();
        let providers = ProviderRegistry::new(&monitors);
        let web_context = TrustedWebContext::new(assets::web_assets()?)?;
        let manager = SurfaceManager::new(app, &monitors, config, providers, web_context)?;

        let weak_manager = Rc::downgrade(&manager);
        let monitor_changed_handler = monitors.connect_items_changed(move |monitors, _, _, _| {
            let Some(manager) = weak_manager.upgrade() else {
                return;
            };
            if let Err(message) = SurfaceManager::rebuild_for_monitors(&manager, monitors) {
                eprintln!("{message}");
            }
        });

        Ok(Self {
            manager,
            monitors,
            monitor_changed_handler: Some(monitor_changed_handler),
            _application_hold: app.hold(),
        })
    }

    pub fn present(&self) {
        SurfaceManager::present_bars(&self.manager);
    }
}

impl Drop for ShellHost {
    fn drop(&mut self) {
        if let Some(handler) = self.monitor_changed_handler.take() {
            self.monitors.disconnect(handler);
        }
        SurfaceManager::shutdown(&self.manager);
    }
}

fn install_native_surface_style(display: &gtk4::gdk::Display) {
    let provider = gtk4::CssProvider::new();
    provider
        .load_from_data(".html-desktop-shell-native-surface { background-color: transparent; }");
    gtk4::style_context_add_provider_for_display(
        display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
