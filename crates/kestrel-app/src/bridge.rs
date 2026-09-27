//! JS<->Rust bridge.
//!
//! - `kestrelHost` message handler: registered ONLY on internal webviews so
//!   web pages can never invoke browser commands (hard security boundary).
//! - `push_state`: serialize UI state to JSON and hand it to the page via
//!   `window.__kestrel.receiveState(...)`.
//! - `kestrelForm`: login-form detection + vault save/fill prompts.

use crate::state::AppState;
use crate::tab::Tab;
use crate::window::BrowserWindow;
use gtk::prelude::*;
use gtk::glib;
use std::rc::Rc;
use webkit::prelude::*;
use webkit::WebView;

pub fn register_handlers(state: &Rc<AppState>, win: &Rc<BrowserWindow>, tab: &Rc<Tab>) {
    let ucm = &tab.ucm;

    // ---- internal pages command bridge ----
    if tab.is_internal_view {
        let registered = ucm.register_script_message_handler("kestrelHost", None);
        if !registered {
            eprintln!("kestrel: failed to register kestrelHost handler");
        }
        let weak_win = std::rc::Rc::downgrade(win);
        let state2 = state.clone();
        ucm.connect_script_message_received(
            Some("kestrelHost"),
            move |_, value| {
                let Some(win2) = weak_win.upgrade() else { return };
                let raw = value.to_str().to_string();
                handle_command(&state2, &win2, &raw);
            },
        );
    }

    // ---- credential capture (all web views, opt-in vault) ----
    if !tab.is_internal_view && !tab.private && state.settings.borrow().vault_enabled {
        let registered = ucm.register_script_message_handler("kestrelForm", None);
        if registered {
            let state2 = state.clone();
            ucm.connect_script_message_received(Some("kestrelForm"), move |_, value| {
                let raw = value.to_str().to_string();
                handle_form_message(&state2, &raw);
            });
            // Inject the form-detection script at document end.
            let script = webkit::UserScript::new(
                FORM_SCRIPT,
                webkit::UserContentInjectedFrames::TopFrame,
                webkit::UserScriptInjectionTime::End,
                &[],
                &[],
            );
            ucm.add_script(&script);
        }
    }
}

const FORM_SCRIPT: &str = r#"
(function(){
  if (window.__kestrelForm) return; window.__kestrelForm = true;
  function origin(){ return location.origin; }
  function findForms(){
    return Array.from(document.forms).filter(function(f){
      return f.querySelector('input[type=password]');
    });
  }
  function send(user, pass){ try {
    window.webkit.messageHandlers.kestrelForm.postMessage(JSON.stringify({
      origin: origin(), user: user, pass: pass
    })); } catch(e){}
  }
  document.addEventListener('submit', function(ev){
    var f = ev.target;
    if (!f || !f.querySelector) return;
    var pw = f.querySelector('input[type=password]');
    if (!pw || !pw.value) return;
    var u = f.querySelector('input[type=text],input[type=email],input[name*=user i],input[name*=email i],input[name*=login i]');
    send(u ? u.value : '', pw.value);
  }, true);
})();
"#;

fn handle_form_message(state: &Rc<AppState>, raw: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else { return };
    let Some(origin) = v["origin"].as_str().map(|s| s.to_string()) else { return };
    let user = v["user"].as_str().unwrap_or("").to_string();
    let pass = v["pass"].as_str().unwrap_or("").to_string();
    if pass.is_empty() {
        return;
    }

    // Already saved?
    if let Ok(Some(_)) = kestrel_data::vault::get(&state.db.borrow(), &origin) {
        return;
    }

    // Ask: "Save password for example.com?"
    let msg = format!("Save password for {origin}?");
    let dialog = gtk::MessageDialog::builder()
        .text(msg)
        .buttons(gtk::ButtonsType::YesNo)
        .modal(true)
        .title("Password manager — Kestrel")
        .secondary_text(if user.is_empty() {
            "Stored encrypted in your system keyring.".to_string()
        } else {
            format!("Username: {user}\nStored encrypted in your system keyring.")
        })
        .build();
    let (tx, rx) = std::sync::mpsc::channel::<bool>();
    dialog.connect_response(glib::clone!(
        #[strong]
        tx,
        move |d, r| {
            d.hide();
            let _ = tx.send(r == gtk::ResponseType::Yes);
        }
    ));
    dialog.present();
    let yes = rx.recv().unwrap_or(false);
    dialog.close();

    if yes {
        let cred = kestrel_data::vault::Credential { origin: origin.clone(), username: user, password: pass };
        match kestrel_data::vault::save(&state.db.borrow(), &cred) {
            Ok(()) => {
                if let Some(w) = state.main_window() {
                    w.toast_text("Password saved to keyring");
                }
            }
            Err(e) => {
                if let Some(w) = state.main_window() {
                    w.toast_text("Keyring unavailable — password not saved");
                }
                eprintln!("kestrel: vault save failed: {e}");
            }
        }
    }
}

fn handle_command(state: &Rc<AppState>, win: &Rc<BrowserWindow>, raw: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else { return };
    let cmd = v["cmd"].as_str().unwrap_or("");
    let apply = |new: &dyn Fn(&mut kestrel_data::Settings)| {
        new(&mut state.settings.borrow_mut());
        state.save_settings();
    };

    match cmd {
        // ----- settings -----
        "set-setting" => {
            let key = v["key"].as_str().unwrap_or("");
            let val = v["value"].clone();
            apply(&move |s| {
                match key {
                    "theme" => s.theme = val.as_str().unwrap_or("dark").into(),
                    "search_engine" => s.search_engine = val.as_str().unwrap_or("duckduckgo").into(),
                    "custom_search_url" => s.custom_search_url = val.as_str().unwrap_or("").into(),
                    "fingerprint_protection" => {
                        s.fingerprint_protection = val.as_str().unwrap_or("standard").into()
                    }
                    "webrtc_enabled" => s.webrtc_enabled = val.as_bool().unwrap_or(false),
                    "popups" => s.popups = val.as_str().unwrap_or("block").into(),
                    "ask_download_location" => s.ask_download_location = val.as_bool().unwrap_or(false),
                    "downloads_dir" => s.downloads_dir = val.as_str().unwrap_or("").into(),
                    "restore_session" => s.restore_session = val.as_bool().unwrap_or(true),
                    "do_not_track" => s.do_not_track = val.as_bool().unwrap_or(true),
                    "gestures" => s.gestures = val.as_bool().unwrap_or(true),
                    "discard_inactive_tabs" => s.discard_inactive_tabs = val.as_bool().unwrap_or(true),
                    "hardware_accel" => s.hardware_accel = val.as_bool().unwrap_or(true),
                    "block_third_party_cookies" => {
                        s.block_third_party_cookies = val.as_bool().unwrap_or(true)
                    }
                    "vault_enabled" => s.vault_enabled = val.as_bool().unwrap_or(true),
                    _ => {}
                }
            });
            // Theme applies instantly.
            if key == "theme" {
                crate::theme::apply(&state.settings.borrow().theme);
            }
        }
        "set-list" => {
            let key = v["key"].as_str().unwrap_or("").to_string();
            let on = v["value"].as_bool().unwrap_or(true);
            apply(&|s| {
                s.lists.insert(key.clone(), on);
            });
            state.reload_filters();
        }
        "add-custom-filter" => {
            let rule = v["value"].as_str().unwrap_or("").trim().to_string();
            if !rule.is_empty() {
                apply(&|s| {
                    s.custom_filters.push(rule.clone());
                });
                state.reload_filters();
            }
        }
        "remove-custom-filter" => {
            let rule = v["value"].as_str().unwrap_or("");
            apply(&move |s| {
                s.custom_filters.retain(|r| r != rule);
            });
            state.reload_filters();
        }
        "update-filters" => state.update_lists_now(),
        "test-url" => {
            let url = v["value"].as_str().unwrap_or("").to_string();
            let rrx = state.query_explain(&url, "https://example.com/");
            let win2 = std::rc::Rc::downgrade(win);
            glib::spawn_future_local(async move {
                if let Ok(result) = rrx.recv().await {
                    if let Some(w) = win2.upgrade() {
                        push_to_view(&w, &serde_json::json!({ "type": "test-url-result", "result": result }));
                    }
                }
            });
        }
        // ----- data clearing -----
        "clear-data" => {
            let what = v["value"].as_str().unwrap_or("");
            match what {
                "history" => state.clear_browsing_data(true, false, false),
                "cookies" => state.clear_browsing_data(false, true, false),
                "permissions" => state.clear_browsing_data(false, false, true),
                "all" => state.clear_browsing_data(true, true, true),
                _ => {}
            }
            win.refresh_internal_pages();
        }
        // ----- navigation -----
        "open-url" => {
            let url = v["value"].as_str().unwrap_or("");
            if !url.is_empty() {
                win.new_tab(&crate::session::normalize_uri(url), false, false);
            }
        }
        "search" => {
            let q = v["value"].as_str().unwrap_or("");
            let url = state.settings.borrow().search_url(q);
            win.load_active(&url);
        }
        // ----- bookmarks -----
        "remove-bookmark" => {
            let url = v["value"].as_str().unwrap_or("");
            kestrel_data::bookmarks::remove(&state.db.borrow(), url);
            win.refresh_internal_pages();
        }
        // ----- history -----
        "remove-history" => {
            let id = v["id"].as_i64().unwrap_or(0);
            kestrel_data::history::remove(&state.db.borrow(), id);
            win.refresh_internal_pages();
        }
        // ----- downloads -----
        "open-file" => {
            let path = v["value"].as_str().unwrap_or("");
            let _ = std::process::Command::new("xdg-open")
                .arg(path)
                .spawn();
        }
        "open-folder" => {
            let path = v["value"].as_str().unwrap_or("");
            let _ = std::process::Command::new("xdg-open")
                .arg(std::path::Path::new(path).parent().unwrap_or(std::path::Path::new("/")))
                .spawn();
        }
        "clear-downloads" => {
            kestrel_data::downloads::clear(&state.db.borrow());
            win.refresh_internal_pages();
        }
        // ----- vault -----
        "delete-vault-entry" => {
            let origin = v["value"].as_str().unwrap_or("");
            let _ = kestrel_data::vault::delete(&state.db.borrow(), origin);
            win.refresh_internal_pages();
        }
        _ => {
            eprintln!("kestrel: unknown bridge command {cmd}");
        }
    }
}

fn push_to_view(win: &Rc<BrowserWindow>, payload: &serde_json::Value) {
    if let Some(tab) = win.current_tab() {
        let js = format!(
            "window.__kestrel && window.__kestrel.onMessage && window.__kestrel.onMessage({})",
            payload.to_string()
        );
        let _ = tab.webview.evaluate_javascript(&js, None::<&str>, None::<&str>, None::<&gtk::gio::Cancellable>, |_| {});
    }
}

/// Build the JSON state blob and deliver it to an internal page.
pub fn push_state(webview: &WebView, state: &Rc<AppState>) {
    let s = state.settings.borrow();
    let db = state.db.borrow();

    let statuses = state.list_statuses.borrow();
    let status_json: Vec<serde_json::Value> = statuses
        .iter()
        .map(|st| {
            serde_json::json!({
                "key": st.key, "title": st.title, "enabled": st.enabled,
                "cached": st.cached, "rules": st.rules, "stale": st.stale
            })
        })
        .collect();

    let total_rules: usize = statuses.iter().filter(|st| st.enabled).map(|st| st.rules).sum();

    let bookmarks: Vec<serde_json::Value> = kestrel_data::bookmarks::list(&db)
        .iter()
        .map(|b| serde_json::json!({"url": b.url, "title": b.title}))
        .collect();

    let history: Vec<serde_json::Value> = kestrel_data::history::search(&db, "", 200)
        .iter()
        .map(|h| serde_json::json!({"id": h.id, "url": h.url, "title": h.title, "ts": h.visited_at}))
        .collect();

    let top: Vec<serde_json::Value> = kestrel_data::history::top_sites(&db, 8)
        .iter()
        .map(|h| serde_json::json!({"url": h.url, "title": h.title}))
        .collect();

    let downloads: Vec<serde_json::Value> = kestrel_data::downloads::list(&db, 50)
        .iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id, "uri": d.uri, "dest": d.destination, "file": d.filename,
                "state": d.state, "received": d.received_bytes, "total": d.total_bytes
            })
        })
        .collect();

    let vault: Vec<serde_json::Value> = kestrel_data::vault::list(&db)
        .iter()
        .map(|e| serde_json::json!({"origin": e.origin, "username": e.username}))
        .collect();

    let payload = serde_json::json!({
        "settings": {
            "theme": s.theme,
            "search_engine": s.search_engine,
            "custom_search_url": s.custom_search_url,
            "fingerprint_protection": s.fingerprint_protection,
            "lists": s.lists,
            "custom_filters": s.custom_filters,
            "webrtc_enabled": s.webrtc_enabled,
            "popups": s.popups,
            "ask_download_location": s.ask_download_location,
            "downloads_dir": s.downloads_dir,
            "restore_session": s.restore_session,
            "do_not_track": s.do_not_track,
            "gestures": s.gestures,
            "discard_inactive_tabs": s.discard_inactive_tabs,
            "hardware_accel": s.hardware_accel,
            "block_third_party_cookies": s.block_third_party_cookies,
            "vault_enabled": s.vault_enabled,
        },
        "shield": {
            "filters_active": state.filters_active.load(std::sync::atomic::Ordering::SeqCst),
            "compile_ok": state.filter_compile_ok.get(),
            "tracker_hosts": state.shield.borrow().tracker_host_count(),
            "network_rules": state.rule_count.get(),
            "total_rules": total_rules,
            "lifetime_blocked": s.lifetime_blocked,
            "statuses": status_json,
        },
        "bookmarks": bookmarks,
        "history": history,
        "top_sites": top,
        "downloads": downloads,
        "vault": vault,
        "version": env!("CARGO_PKG_VERSION"),
    });
    drop(db);
    drop(s);

    let js = format!(
        "window.__kestrel && window.__kestrel.receiveState && window.__kestrel.receiveState({})",
        payload.to_string()
    );
    let _ = webview.evaluate_javascript(&js, None::<&str>, None::<&str>, None::<&gtk::gio::Cancellable>, |_| {});
}
