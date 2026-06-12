mod bridge;
mod shell_window;

use gtk4::prelude::*;

const APP_ID: &str = "dev.ohmypi.HtmlDesktopShell";

fn main() {
    let app = gtk4::Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| match shell_window::shell_window_new(app) {
        Ok(window) => window.present(),
        Err(message) => {
            eprintln!("{message}");
            app.quit();
        }
    });

    app.run();
}
