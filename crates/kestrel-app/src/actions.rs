//! Window actions + keyboard accelerators. Every menu entry, shortcut and
//! button lands here — all of them perform real work.

use crate::window::BrowserWindow;
use gtk::prelude::*;
use gtk::glib;
use webkit::prelude::*;

pub fn install(w: &Rc<BrowserWindow>) {
    let a = w.win.action_group().unwrap_or_else(|| {
        let g = gio_shim();
        w.win.insert_action_group("win", Some(&g));
        g
    });

    macro_rules! act {
        ($name:expr, $state:expr, $body:expr) => {{
            let w = $state.clone();
            let action = gio_shim_simple_action($name, None::<&str>);
            action.connect_activate(glib::clone!(#[weak] w, move |_, _| {
                let f: fn(&Rc<BrowserWindow>) = $body;
                f(&w);
            }));
            a.add_action(&action);
        }};
    }

    act!("new-tab", w, |w| {
        w.new_tab("kestrel://newtab", false, false);
    });
    act!("new-private-tab", w, |w| {
        w.new_tab("kestrel://newtab", false, true);
    });
    act!("reopen-closed", w, |w| w.reopen_closed_tab());
    act!("close-tab", w, |w| {
        let i = w.active_index();
        w.close_tab(i);
    });
    act!("next-tab", w, |w| w.next_tab(1));
    act!("prev-tab", w, |w| w.next_tab(-1));
    act!("move-tab-left", w, |w| w.move_tab(-1));
    act!("move-tab-right", w, |w| w.move_tab(1));
    act!("go-back", w, |w| {
        if let Some(t) = w.current_tab() {
            t.webview.go_back();
        }
    });
    act!("go-forward", w, |w| {
        if let Some(t) = w.current_tab() {
            t.webview.go_forward();
        }
    });
    act!("reload", w, |w| {
        if let Some(t) = w.current_tab() {
            t.webview.reload();
        }
    });
    act!("stop", w, |w| {
        if let Some(t) = w.current_tab() {
            t.webview.stop_loading();
        }
    });
    act!("zoom-in", w, |w| zoom(w, 0.1));
    act!("zoom-out", w, |w| zoom(w, -0.1));
    act!("zoom-reset", w, |w| {
        if let Some(t) = w.current_tab() {
            t.webview.set_zoom_level(1.0);
        }
    });
    act!("find", w, |w| {
        if let Some(t) = w.current_tab() {
            t.findbar.open();
        }
    });
    act!("print", w, |w| {
        if let Some(t) = w.current_tab() {
            let op = webkit::PrintOperation::new(&t.webview);
            op.run_dialog(Some(&w.win));
        }
    });
    act!("save-page", w, |w| save_page(w));
    act!("devtools", w, |w| {
        if let Some(t) = w.current_tab() {
            let inspector = t.webview.inspector();
            inspector.show();
        }
    });
    act!("fullscreen-toggle", w, |w| {
        if w.win.is_fullscreen() {
            w.win.unfullscreen();
        } else {
            w.win.fullscreen();
        }
    });
    act!("bookmark-toggle", w, |w| bookmark_toggle(w));
    act!("open-bookmarks", w, |w| w.open_internal("bookmarks"));
    act!("open-history", w, |w| w.open_internal("history"));
    act!("open-downloads", w, |w| w.open_internal("downloads"));
    act!("open-privacy", w, |w| w.open_internal("privacy"));
    act!("open-settings", w, |w| w.open_internal("settings"));
    act!("about", w, |w| crate::menus::show_about(w));
    act!("mute-tab", w, |w| {
        if let Some(t) = w.current_tab() {
            let new_state = !t.muted.get();
            t.muted.set(new_state);
            t.webview.set_is_muted(new_state);
            w.toast_text(if new_state { "Tab muted" } else { "Tab unmuted" });
        }
    });
    act!("pin-tab", w, |w| {
        if let Some(t) = w.current_tab() {
            let new_state = !t.pinned.get();
            t.pinned.set(new_state);
            t.title_label.set_visible(!new_state);
            w.toast_text(if new_state { "Tab pinned" } else { "Tab unpinned" });
        }
    });

    // Parameterized select-tab action for Alt+1..8
    let action = gio_shim_simple_action("select-tab", Some(&String::static_variant_type()));
    {
        let w2 = w.clone();
        action.connect_activate(move |_, param| {
            if let Some(p) = param {
                let idx: usize = p.str().and_then(|s| s.parse().ok()).unwrap_or(0);
                w2.select_tab(idx);
            }
        });
    }
    a.add_action(&action);

    install_accelerators(w);
}

fn zoom(w: &Rc<BrowserWindow>, delta: f64) {
    if let Some(t) = w.current_tab() {
        let z = (t.webview.zoom_level() + delta).clamp(0.25, 5.0);
        t.webview.set_zoom_level(z);
        w.toast_text(&format!("Zoom {}%", (z * 100.0) as i64));
    }
}

fn bookmark_toggle(w: &Rc<BrowserWindow>) {
    let Some(t) = w.current_tab() else { return };
    let uri = t.uri();
    if uri.is_empty() {
        return;
    }
    let db = w.state.db.borrow();
    if kestrel_data::bookmarks::has(&db, &uri) {
        kestrel_data::bookmarks::remove(&db, &uri);
        w.toast_text("Bookmark removed");
    } else {
        let title = t.title.borrow().clone();
        kestrel_data::bookmarks::add(&db, &uri, &title, "");
        w.toast_text("Bookmark added");
    }
    drop(db);
    w.refresh_chrome();
}

fn save_page(w: &Rc<BrowserWindow>) {
    let Some(t) = w.current_tab() else { return };
    let dialog = gtk::FileDialog::new();
    dialog.set_initial_name(Some("page.mhtml"));
    let w2 = w.clone();
    dialog.save(Some(&w.win), None::<&gio::Cancellable>, move |res| {
        if let Ok(file) = res {
            if let Some(path) = file.path() {
                let webview = w2.current_tab().map(|t| t.webview.clone());
                if let Some(v) = webview {
                    v.save_to_file(
                        &file,
                        webkit::SaveMode::Mhtml,
                        None::<&gio::Cancellable>,
                        glib::clone!(#[strong] path, move |result| {
                            if result.is_err() {
                                eprintln!("kestrel: save page failed");
                            }
                        }),
                    );
                }
            }
        }
    });
}

fn install_accelerators(w: &Rc<BrowserWindow>) {
    let app = w.win.application().unwrap();
    let accels: &[(&str, &[&str])] = &[
        ("win.new-tab", &["<Primary>T"]),
        ("win.new-private-tab", &["<Primary><Shift>P"]),
        ("win.reopen-closed", &["<Primary><Shift>T"]),
        ("win.close-tab", &["<Primary>W"]),
        ("win.next-tab", &["<Primary>Tab"]),
        ("win.prev-tab", &["<Primary><Shift>Tab"]),
        ("win.move-tab-left", &["<Primary><Shift>Left"]),
        ("win.move-tab-right", &["<Primary><Shift>Right"]),
        ("win.go-back", &["<Alt>Left"]),
        ("win.go-forward", &["<Alt>Right"]),
        ("win.reload", &["<Primary>R", "F5"]),
        ("win.stop", &["Escape"]),
        ("win.zoom-in", &["<Primary>plus", "<Primary>equal"]),
        ("win.zoom-out", &["<Primary>minus"]),
        ("win.zoom-reset", &["<Primary>0"]),
        ("win.find", &["<Primary>F"]),
        ("win.print", &["<Primary>P"]),
        ("win.save-page", &["<Primary>S"]),
        ("win.fullscreen-toggle", &["F11"]),
        ("win.devtools", &["F12"]),
        ("win.bookmark-toggle", &["<Primary>D"]),
        ("win.open-bookmarks", &["<Primary><Shift>O"]),
        ("win.open-history", &["<Primary>H"]),
        ("win.open-downloads", &["<Primary>J"]),
        ("win.open-settings", &["<Primary>comma"]),
        ("win.mute-tab", &["<Primary>M"]),
        ("win.pin-tab", &["<Primary><Shift>P"]),
        ("app.quit", &["<Primary>Q"]),
        ("win.select-tab(0)", &["<Alt>1"]),
        ("win.select-tab(1)", &["<Alt>2"]),
        ("win.select-tab(2)", &["<Alt>3"]),
        ("win.select-tab(3)", &["<Alt>4"]),
        ("win.select-tab(4)", &["<Alt>5"]),
        ("win.select-tab(5)", &["<Alt>6"]),
        ("win.select-tab(6)", &["<Alt>7"]),
        ("win.select-tab(7)", &["<Alt>8"]),
    ];
    for (action, keys) in accels {
        app.set_accels_for_action(action, keys);
    }

    // Focus address bar
    app.set_accels_for_action("win.focus-address", &["<Primary>L"]);
    let action = gio_shim_simple_action("focus-address", None::<&str>);
    let w2 = w.clone();
    action.connect_activate(move |_, _| {
        w2.address.entry.grab_focus();
    });
    w.win.add_action(&action);
}

// --- thin shims so imports stay local ---
use gtk::gio as gio_gtk;

fn gio_shim() -> gio_gtk::SimpleActionGroup {
    gio_gtk::SimpleActionGroup::new()
}

fn gio_shim_simple_action(
    name: &str,
    param: Option<&glib::VariantTy>,
) -> gio_gtk::SimpleAction {
    gio_gtk::SimpleAction::new(name, param)
}
