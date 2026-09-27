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

    // Smoke-test mode: <binary> --smoke-test <outdir>
    let smoke_dir = args
        .iter()
        .position(|a| a == "--smoke-test")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from);

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

    // URL-looking args (http/https, or dotted host), skipping CLI flags and
    // the smoke-test outdir argument.
    let mut urls: Vec<String> = Vec::new();
    let mut skip_next = false;
    for a in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if a == "--smoke-test" {
            skip_next = true;
            continue;
        }
        if a.starts_with('-') {
            continue;
        }
        if a.starts_with("http://") || a.starts_with("https://") || (a.contains('.') && !a.contains('/')) {
            urls.push(a.clone());
        }
    }

    gtk::init().expect("GTK init");

    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gtk::gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let state = state::AppState::new(settings);
    app.connect_startup(glib::clone!(
        #[weak]
        state,
        move |_| {
            theme::apply(&state.settings.borrow().theme);
            state.on_startup();
        }
    ));

    let urls_cell = RefCell::new(urls.clone());
    let smoke_cell = RefCell::new(smoke_dir.clone());
    app.connect_activate(glib::clone!(
        #[weak]
        state,
        move |app| {
            let w = window::BrowserWindow::new(app, &state);
            w.present();
            state.add_window(&w);

            let urls = urls_cell.borrow().clone();
            if !urls.is_empty() {
                for u in urls {
                    w.open_url_in_new_tab(&u);
                }
            }
            if let Some(dir) = smoke_cell.borrow().clone() {
                smoke::run(&w, dir);
            }
        }
    ));

    // Additional instances hand their URLs to the primary instance.
    app.connect_open(glib::clone!(
        #[weak]
        state,
        move |_, files, _| {
            for f in files {
                if let Some(uri) = f.uri() {
                    if let Some(w) = state.main_window() {
                        w.open_url_in_new_tab(&uri);
                    }
                }
            }
        }
    ));

    app.run();
}
