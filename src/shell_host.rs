use std::{cell::RefCell, rc::Rc};

use glib::prelude::{CastNone, ObjectExt};
use gtk4::{gio::prelude::ListModelExt, prelude::*};

use crate::{assets, config::ShellConfig, providers::ProviderRegistry, shell_window};

pub struct ShellHost {
    state: Rc<RefCell<ShellHostState>>,
    monitors: gtk4::gio::ListModel,
    monitor_changed_handler: Option<glib::SignalHandlerId>,
}

struct ShellHostState {
    app: gtk4::Application,
    panels: Vec<gtk4::ApplicationWindow>,
    config: ShellConfig,
    providers: ProviderRegistry,
}

impl ShellHost {
    pub fn new(app: &gtk4::Application, config: ShellConfig) -> Result<Self, String> {
        if !gtk4_layer_shell::is_supported() {
            return Err("Wayland compositor does not support layer-shell".to_owned());
        }

        let display = gtk4::gdk::Display::default()
            .ok_or_else(|| "missing default GDK display".to_owned())?;
        let monitors = display.monitors();
        let providers = ProviderRegistry::new(&monitors);
        let panels = panels_for_monitors(app, &monitors, &config, &providers)?;

        let state = Rc::new(RefCell::new(ShellHostState {
            app: app.clone(),
            panels,
            config,
            providers,
        }));
        let state_for_monitor_changes = Rc::clone(&state);
        let monitor_changed_handler = monitors.connect_items_changed(move |monitors, _, _, _| {
            if let Err(message) = state_for_monitor_changes
                .borrow_mut()
                .rebuild_panels(monitors)
            {
                eprintln!("{message}");
            }
        });

        Ok(Self {
            state,
            monitors,
            monitor_changed_handler: Some(monitor_changed_handler),
        })
    }

    pub fn present(&self) {
        self.state.borrow().present();
    }
}

impl Drop for ShellHost {
    fn drop(&mut self) {
        if let Some(handler) = self.monitor_changed_handler.take() {
            self.monitors.disconnect(handler);
        }
    }
}

impl ShellHostState {
    fn present(&self) {
        for panel in &self.panels {
            panel.present();
        }
    }

    fn rebuild_panels(&mut self, monitors: &gtk4::gio::ListModel) -> Result<(), String> {
        let new_panels = panels_for_monitors(&self.app, monitors, &self.config, &self.providers)?;

        for panel in self.panels.drain(..) {
            panel.close();
        }
        self.panels = new_panels;
        self.present();

        Ok(())
    }
}

fn panels_for_monitors(
    app: &gtk4::Application,
    monitors: &gtk4::gio::ListModel,
    config: &ShellConfig,
    providers: &ProviderRegistry,
) -> Result<Vec<gtk4::ApplicationWindow>, String> {
    if monitors.n_items() == 0 {
        return Err("no GDK monitors available".to_owned());
    }

    let html_path = assets::web_index_path()?;
    let uri = glib::filename_to_uri(&html_path, None).map_err(|error| {
        format!(
            "failed to create file URI for {}: {error}",
            html_path.display()
        )
    })?;

    let mut panels = Vec::with_capacity(monitors.n_items() as usize);
    for index in 0..monitors.n_items() {
        let Some(monitor) = monitors.item(index).and_downcast::<gtk4::gdk::Monitor>() else {
            eprintln!("skipping non-monitor GDK list item at index {index}");
            continue;
        };

        panels.push(shell_window::shell_window_for_monitor(
            app,
            &monitor,
            index,
            uri.as_str(),
            config,
            providers.clone(),
        )?);
    }

    if panels.is_empty() {
        return Err("no usable GDK monitors available".to_owned());
    }

    Ok(panels)
}
