mod bridge;
mod shell_host;
mod shell_window;

use crate::shell_host::ShellHost;
use gtk4::prelude::*;

const APP_ID: &str = "dev.ohmypi.HtmlDesktopShell";

fn main() {
    let app = gtk4::Application::builder().application_id(APP_ID).build();

    let shell_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let shell_host_for_activate = std::rc::Rc::clone(&shell_host);

    app.connect_activate(move |app| match ShellHost::new(app) {
        Ok(host) => {
            host.present();
            *shell_host_for_activate.borrow_mut() = Some(host);
        }
        Err(message) => {
            eprintln!("{message}");
            app.quit();
        }
    });

    app.run();
}
