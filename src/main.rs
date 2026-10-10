mod assets;
mod bridge;
mod config;
mod diagnostics;
mod messages;
mod providers;
mod shell_host;
mod surface;
mod surface_manager;
mod surface_window;
mod tray_overlay;
mod web_security;

use crate::shell_host::ShellHost;
use gtk4::prelude::*;

const APP_ID: &str = "dev.ohmypi.HtmlDesktopShell";

fn main() {
    let loaded_config = match config::load() {
        Ok(loaded_config) => loaded_config,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    };

    match diagnostics::run_if_requested(&loaded_config) {
        Ok(true) => return,
        Ok(false) => {}
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
    let shell_config = loaded_config.config;
    let app = gtk4::Application::builder().application_id(APP_ID).build();

    let shell_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let shell_host_for_activate = std::rc::Rc::clone(&shell_host);
    let shell_config_for_activate = shell_config.clone();
    app.connect_activate(
        move |app| match ShellHost::new(app, shell_config_for_activate.clone()) {
            Ok(host) => {
                host.present();
                *shell_host_for_activate.borrow_mut() = Some(host);
            }
            Err(message) => {
                eprintln!("{message}");
                app.quit();
            }
        },
    );

    let shutdown_signals = [libc::SIGINT, libc::SIGTERM].map(|signal| {
        let app = app.downgrade();
        glib_unix::unix_signal_add_local(signal, move || {
            if let Some(app) = app.upgrade() {
                app.quit();
            }
            glib::ControlFlow::Continue
        })
    });

    app.run_with_args(&loaded_config.app_args);
    for source in shutdown_signals {
        source.remove();
    }
    drop(shell_host.borrow_mut().take());
}
