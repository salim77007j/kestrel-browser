//! Kestrel Browser — fast, private, lightweight.
//!
//! Entry point: parses CLI, applies low-level env toggles from settings
//! (must happen before WebKit is initialized), then hands over to GTK.

mod actions;
mod addressbar;
mod bridge;
mod dl;
mod findbar;
mod gestures;
mod menus;
mod pages;
mod perms;
mod session;
mod smoke;
mod state;
mod tab;
mod theme;
mod webview;
mod window;

use std::cell::RefCell;
use std::rc::Rc;
use gtk::prelude::*;

const APP_ID: &str = "io.kestrel.Browser";

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version") {
        println!("Kestrel {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    // Settings must be read before any WebKit initialization because some
    // knobs are environment-based (renderer backend).
    if kestrel_data::dirs::ensure_dirs().is_err() {
        eprintln!("kestrel: cannot create data directories");
    }
    let settings = kestrel_data::Settings::load();
    if !settings.hardware_accel {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    // Stable, quiet GL stack for CI / VMs
    if std::env::var("KESTREL_FORCE_SOFTWARE_GL").is_ok() {
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
    }

    gtk::init().expect("GTK init");

    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gtk::gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let smoke_cell: RefCell<Option<std::path::PathBuf>> = RefCell::new(None);
    app.add_main_option(
        "smoke-test",
        b'0'.into(),
        gtk::glib::OptionFlags::NONE,
        gtk::glib::OptionArg::String,
        "run the built-in smoke test, writing evidence to DIR",
        Some("DIR"),
    );
    app.connect_handle_local_options(glib::clone!(
        #[weak]
        app,
        move |_, dict| {
            if let Some(v) = dict.lookup_value("smoke-test", gtk::glib::VariantTy::STRING) {
                if let Some(dir) = v.str() {
                    *smoke_cell.borrow_mut() = Some(std::path::PathBuf::from(dir));
                }
            }
            -1 // continue normal startup
        }
    ));

    let state = state::AppState::new(settings);
    {
        let state2 = state.clone();
        app.connect_startup(move |_| {
            theme::apply(&state2.settings.borrow().theme);
            state2.on_startup();
        });
    }

    let urls_cell: RefCell<Vec<String>> = RefCell::new(Vec::new());
    {
        let state2 = state.clone();
        app.connect_activate(move |app| {
            let w = window::BrowserWindow::new(app, &state2);
            w.present();
            state2.add_window(&w);

            let urls = urls_cell.borrow().clone();
            if !urls.is_empty() {
                for u in urls {
                    w.open_url_in_new_tab(&u);
                }
            }
            if let Some(dir) = smoke_cell.borrow().clone() {
                smoke::run(&w, dir);
            }
        });
    }

    // Additional instances hand their URLs to the primary instance.
    {
        let state2 = state.clone();
        app.connect_open(move |_, files, _| {
            for f in files {
                let uri = f.uri().to_string();
                if let Some(w) = state2.main_window() {
                    w.open_url_in_new_tab(&uri);
                }
            }
        });
    }

    app.run();
}
