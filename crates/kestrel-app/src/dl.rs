//! Downloads: WebKit download lifecycle -> sqlite + UI badge + toasts.

use crate::state::AppState;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::Cell;
use std::rc::Rc;
use webkit::prelude::*;

pub struct DownloadCenter {
    pub active_count: Cell<u64>,
}

impl DownloadCenter {
    pub fn new() -> Self {
        Self { active_count: Cell::new(0) }
    }
}

impl Default for DownloadCenter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn install(state: &Rc<AppState>) {
    state.session.connect_download_started(glib::clone!(
        #[weak]
        state,
        move |_session, download| {
            let uri = download
                .request()
                .and_then(|r| r.uri())
                .map(|u| u.to_string())
                .unwrap_or_default();
            let suggested = suggested_filename(&uri);

            let id = kestrel_data::downloads::insert(&state.db.borrow(), &uri, &suggested);
            state.downloads.active_count.set(state.downloads.active_count.get() + 1);
            update_badge(&state);

            let ask = state.settings.borrow().ask_download_location;
            let dest_dir = state.settings.borrow().downloads_dir.clone();
            if ask {
                let state2 = state.clone();
                let download2 = download.clone();
                let dialog = gtk::FileDialog::new();
                dialog.set_initial_name(Some(&suggested));
                dialog.save(None::<&gtk::Window>, None::<&gio::Cancellable>, move |res| {
                    if let Ok(file) = res {
                        if let Some(path) = file.path() {
                            download2.set_destination(&path.to_string_lossy());
                            kestrel_data::downloads::update(
                                &state2.db.borrow(),
                                id,
                                &path.to_string_lossy(),
                                "active",
                                0,
                                0,
                            );
                        }
                    }
                });
            } else {
                let path = std::path::Path::new(&dest_dir).join(&suggested);
                let _ = std::fs::create_dir_all(&dest_dir);
                download.set_destination(&path.to_string_lossy());
                kestrel_data::downloads::update(
                    &state.db.borrow(),
                    id,
                    &path.to_string_lossy(),
                    "active",
                    0,
                    0,
                );
            }

            {
                let state2 = state.clone();
                let id2 = id;
                download.connect_received_data(move |d, _| {
                    let received = d.received_data_length() as i64;
                    let total = (d.estimated_progress() * 1_000_000.0) as i64;
                    kestrel_data::downloads::update(&state2.db.borrow(), id2, "", "active", received, total);
                });
            }
            {
                let state2 = state.clone();
                let id2 = id;
                let dest = download.destination().map(|d| d.to_string()).unwrap_or_default();
                download.connect_finished(move |d| {
                    let received = d.received_data_length() as i64;
                    kestrel_data::downloads::update(&state2.db.borrow(), id2, &dest, "done", received, received);
                    state2.downloads.active_count.set(state2.downloads.active_count.get().saturating_sub(1));
                    update_badge(&state2);
                    if let Some(w) = state2.main_window() {
                        w.toast_text(&format!("Downloaded {dest}"));
                    }
                });
            }
            {
                let state2 = state.clone();
                let id2 = id;
                download.connect_failed(move |_, _error| {
                    kestrel_data::downloads::update(&state2.db.borrow(), id2, "", "failed", 0, 0);
                    state2.downloads.active_count.set(state2.downloads.active_count.get().saturating_sub(1));
                    update_badge(&state2);
                });
            }
        }
    ));
}

fn suggested_filename(uri: &str) -> String {
    let base = uri.split('/').next_back().unwrap_or("download");
    let cleaned: String = base
        .split('?')
        .next()
        .unwrap_or("download")
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '~'))
        .collect();
    if cleaned.is_empty() {
        "download".into()
    } else {
        cleaned
    }
}

fn update_badge(state: &Rc<AppState>) {
    let n = state.downloads.active_count.get();
    if let Some(w) = state.main_window() {
        let text = if n > 0 { n.to_string() } else { String::new() };
        w.downloads_badge.set_text(&text);
    }
}
