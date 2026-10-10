use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use glib::prelude::ObjectExt;
use gtk4::prelude::*;

use crate::{
    assets, config::ShellConfig, providers::ProviderRegistry, surface_manager::SurfaceManager,
    web_security::TrustedWebContext,
};

type MonitorConnectorHandlers = Rc<RefCell<Vec<(gtk4::gdk::Monitor, glib::SignalHandlerId)>>>;

pub struct ShellHost {
    manager: Rc<RefCell<SurfaceManager>>,
    monitors: gtk4::gio::ListModel,
    monitor_changed_handler: Option<glib::SignalHandlerId>,
    monitor_connector_handlers: MonitorConnectorHandlers,
    pending_monitor_rebuild: Rc<RefCell<Option<glib::SourceId>>>,
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

        let monitor_connector_handlers = Rc::new(RefCell::new(Vec::new()));
        let pending_monitor_rebuild = Rc::new(RefCell::new(None));
        let weak_manager = Rc::downgrade(&manager);
        watch_monitor_connectors(
            &monitors,
            &weak_manager,
            &monitor_connector_handlers,
            &pending_monitor_rebuild,
        );
        queue_monitor_rebuild(&weak_manager, &monitors, &pending_monitor_rebuild);
        let connector_handlers = monitor_connector_handlers.clone();
        let pending = pending_monitor_rebuild.clone();
        let monitor_changed_handler = monitors.connect_items_changed(move |monitors, _, _, _| {
            watch_monitor_connectors(monitors, &weak_manager, &connector_handlers, &pending);
            queue_monitor_rebuild(&weak_manager, monitors, &pending);
        });

        Ok(Self {
            manager,
            monitors,
            monitor_changed_handler: Some(monitor_changed_handler),
            monitor_connector_handlers,
            pending_monitor_rebuild,
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
        for (monitor, handler) in self.monitor_connector_handlers.borrow_mut().drain(..) {
            monitor.disconnect(handler);
        }
        if let Some(source) = self.pending_monitor_rebuild.borrow_mut().take() {
            source.remove();
        }
        SurfaceManager::shutdown(&self.manager);
    }
}

fn watch_monitor_connectors(
    monitors: &gtk4::gio::ListModel,
    manager: &Weak<RefCell<SurfaceManager>>,
    handlers: &MonitorConnectorHandlers,
    pending: &Rc<RefCell<Option<glib::SourceId>>>,
) {
    let mut handlers = handlers.borrow_mut();
    for (monitor, handler) in handlers.drain(..) {
        monitor.disconnect(handler);
    }
    for index in 0..monitors.n_items() {
        let Some(monitor) = monitors.item(index).and_downcast::<gtk4::gdk::Monitor>() else {
            continue;
        };
        let manager = manager.clone();
        let monitors = monitors.clone();
        let pending = pending.clone();
        let handler = monitor.connect_connector_notify(move |_| {
            queue_monitor_rebuild(&manager, &monitors, &pending);
        });
        handlers.push((monitor, handler));
    }
}

fn queue_monitor_rebuild(
    manager: &Weak<RefCell<SurfaceManager>>,
    monitors: &gtk4::gio::ListModel,
    pending: &Rc<RefCell<Option<glib::SourceId>>>,
) {
    if pending.borrow().is_some() {
        return;
    }
    let manager = manager.clone();
    let monitors = monitors.clone();
    let pending_for_callback = pending.clone();
    let source = glib::idle_add_local_once(move || {
        pending_for_callback.borrow_mut().take();
        if let Some(manager) = manager.upgrade()
            && let Err(message) = SurfaceManager::rebuild_for_monitors(&manager, &monitors)
        {
            eprintln!("{message}");
        }
    });
    *pending.borrow_mut() = Some(source);
}

fn install_native_surface_style(display: &gtk4::gdk::Display) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(concat!(
        ".html-desktop-shell-native-surface { background-color: transparent; }",
        ".html-desktop-shell-tray-popover { background-color: transparent; }",
        ".html-desktop-shell-tray-popover > contents {",
        " background-color: transparent; padding: 0; border: 0; border-radius: 0;",
        " box-shadow: none; margin: 0; }",
    ));
    gtk4::style_context_add_provider_for_display(
        display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
