mod bridge;
mod config;
mod messages;
mod providers;
mod shell_host;
mod shell_window;

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

    app.run_with_args(&loaded_config.app_args);
}
