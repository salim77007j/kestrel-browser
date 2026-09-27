//! WebView creation + all page-facing signal wiring: navigation policy,
//! popups, script dialogs, permissions, downloads handoff, cosmetic filtering,
//! fingerprint shield injection, history, shield audit, crash recovery.

use crate::session::normalize_uri;
use crate::state::AppState;
use crate::tab::Tab;
use crate::window::BrowserWindow;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use webkit::prelude::*;
use webkit::{LoadEvent, NetworkSession, PolicyDecisionType, UserContentManager, WebView};

pub fn create_tab(
    state: Rc<AppState>,
    win: Rc<BrowserWindow>,
    uri: &str,
    private: bool,
) -> Rc<Tab> {
    let id = Tab::new_id();
    let ucm = UserContentManager::new();

    // Fingerprint shield (document start, main world, all frames).
    let mode = state.fingerprint_mode();
    let script_src = mode.script().replace("__KESTREL_MODE__", mode.key());
    if !script_src.is_empty() {
        let script = webkit::UserScript::new(
            &script_src,
            webkit::UserContentInjectedFrames::AllFrames,
            webkit::UserScriptInjectionTime::Start,
            &[],
            &[],
        );
        ucm.add_script(&script);
    }

    let session: NetworkSession = if private {
        state.ephemeral_session()
    } else {
        state.session.clone()
    };

    let webview = WebView::builder()
        .web_context(&state.webctx)
        .user_content_manager(&ucm)
        .network_session(&session)
        .build();

    let settings = webview.settings();
    apply_settings(&state, &settings);

    // Compiled network content filter (if already built).
    if let Some(filter) = state.network_filter.borrow().as_ref() {
        ucm.add_filter(filter);
    }

    // Tab widget.
    let (button, title_label, fav_label, spinner, close_btn) =
        crate::tab::build_tab_widget(&state);

    // Content box = webview + findbar.
    webview.set_vexpand(true);
    webview.set_hexpand(true);
    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_box.append(&webview);

    let findbar = crate::findbar::FindBar::new();
    findbar.attach(&webview);
    content_box.append(&findbar.revealer);
    let tab = Rc::new(Tab {
        id,
        webview: webview.clone(),
        ucm: ucm.clone(),
        content_box,
        findbar,
        button,
        title_label,
        fav_label,
        spinner,
        close_btn,
        title: RefCell::new("New Tab".into()),
        pinned: std::cell::Cell::new(false),
        muted: std::cell::Cell::new(false),
        private,
        internal: internal_page_of(uri),
        is_internal_view: internal_page_of(uri).is_some(),
        shield_hits: std::cell::Cell::new(0),
        cosmetic_rules: std::cell::Cell::new(0),
    });

    // Close button.
    {
        let tab2 = tab.clone();
        tab.close_btn.connect_clicked(move |_| {
            if let Some(w) = state.main_window() {
                if let Some(i) = w.index_of(&tab2) {
                    w.close_tab(i);
                }
            }
        });
    }

    wire_webview(&state, &win, &tab, &webview);
    crate::bridge::register_handlers(&state, &win, &tab);

    if !uri.is_empty() {
        webview.load_uri(&normalize_uri(uri));
    }
    tab
}

fn internal_page_of(uri: &str) -> Option<&'static str> {
    const PAGES: [&str; 7] = [
        "newtab", "settings", "privacy", "bookmarks", "history", "downloads", "about",
    ];
    if let Some(rest) = uri.strip_prefix("kestrel://") {
        let page = rest.trim_start_matches('/');
        let page = page.split('?').next().unwrap_or(page);
        return PAGES.into_iter().find(|p| *p == page);
    }
    None
}

pub fn apply_settings(state: &Rc<AppState>, settings: &webkit::Settings) {
    let s = state.settings.borrow();
    settings.set_enable_developer_extras(true);
    settings.set_enable_smooth_scrolling(true);
    settings.set_enable_back_forward_navigation_gestures(true);
    settings.set_enable_webrtc(s.webrtc_enabled); // privacy: off by default
    settings.set_javascript_can_open_windows_automatically(false);
    settings.set_allow_modal_dialogs(true);
    settings.set_enable_fullscreen(true);
    settings.set_user_agent(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15",
    );
}

fn wire_webview(state: &Rc<AppState>, win: &Rc<BrowserWindow>, tab: &Rc<Tab>, webview: &WebView) {
    // --- load lifecycle ---
    {
        let win2 = std::rc::Rc::downgrade(win);
        let tab2 = tab.clone();
        webview.connect_load_changed(move |v, event| {
            let Some(win2) = win2.upgrade() else { return };
            match event {
            LoadEvent::Started => {
                tab2.spinner.set_visible(true);
                tab2.spinner.start();
                win2.progress.set_visible(true);
                win2.progress.set_fraction(0.05);
                win2.stop_btn.set_visible(true);
                win2.reload_btn.set_visible(false);
            }
            LoadEvent::Committed => {
                inject_cosmetic(state, v);
            }
            LoadEvent::Finished => {
                tab2.spinner.stop();
                tab2.spinner.set_visible(false);
                win2.progress.set_visible(false);
                win2.stop_btn.set_visible(false);
                win2.reload_btn.set_visible(true);
                on_finished(state, &win2, &tab2, v);
            }
            _ => {}
            }
        });
    }

    // --- progress ---
    {
        let win2 = std::rc::Rc::downgrade(win);
        webview.connect_estimated_load_progress_notify(move |v| {
            let Some(win2) = win2.upgrade() else { return };
            let p = v.estimated_load_progress();
            if p < 1.0 {
                win2.progress.set_visible(true);
                win2.progress.set_fraction(p);
            }
        });
    }

    // --- uri / title ---
    {
        let win2 = std::rc::Rc::downgrade(win);
        let tab2 = tab.clone();
        webview.connect_uri_notify(move |_| {
            let Some(win2) = win2.upgrade() else { return };
            let uri = tab2.uri();
            tab2.fav_label.set_text(&fav_char(&uri));
            if tab2.private {
                tab2.fav_label.add_css_class("private");
            }
            win2.refresh_chrome();
        });
    }
    {
        let win2 = std::rc::Rc::downgrade(win);
        let tab2 = tab.clone();
        webview.connect_title_notify(move |v| {
            let Some(win2) = win2.upgrade() else { return };
            let title = v.title().map(|t| t.to_string()).unwrap_or_default();
            tab2.set_title_text(&title);
            win2.refresh_chrome();
        });
    }

    // --- navigation policy ---
    {
        let win2 = std::rc::Rc::downgrade(win);
        webview.connect_decide_policy(move |_, decision, decision_type| {
            let Some(win2) = win2.upgrade() else {
                decision.use_();
                return glib::Propagation::Proceed;
            };
            match decision_type {
                PolicyDecisionType::NavigationAction => {
                    // External schemes -> system handler.
                    if let Ok(nav) = decision
                        .clone()
                        .dynamic_cast::<webkit::NavigationPolicyDecision>()
                    {
                        let uri = nav
                            .navigation_action()
                            .and_then(|a| a.request())
                            .and_then(|r| r.uri())
                            .map(|u| u.to_string())
                            .unwrap_or_default();
                        let known = uri.starts_with("http://")
                            || uri.starts_with("https://")
                            || uri.starts_with("kestrel://")
                            || uri.starts_with("file://")
                            || uri.starts_with("about:")
                            || uri.starts_with("data:")
                            || uri.starts_with("blob:")
                            || uri.is_empty();
                        if !known && !uri.is_empty() {
                            crate::webview::open_external(&uri);
                            decision.ignore();
                            return glib::Propagation::Stop;
                        }
                    }
                    decision.use_();
                    glib::Propagation::Proceed
                }
                PolicyDecisionType::NewWindowAction => {
                    let (uri, user_gesture) = decision
                        .clone()
                        .dynamic_cast::<webkit::NavigationPolicyDecision>()
                        .ok()
                        .and_then(|nav| nav.navigation_action())
                        .map(|a| {
                            let uri = a
                                .request()
                                .and_then(|r| r.uri())
                                .map(|u| u.to_string())
                                .unwrap_or_default();
                            (uri, a.is_user_gesture())
                        })
                        .unwrap_or((String::new(), false));
                    if user_gesture {
                        win2.new_tab(&normalize_uri(&uri), true, false);
                        decision.ignore();
                    } else if win2.state.settings.borrow().popups == "block" {
                        win2.toast_text("Popup blocked");
                        decision.ignore();
                    } else {
                        win2.new_tab(&normalize_uri(&uri), true, false);
                        decision.ignore();
                    }
                    glib::Propagation::Stop
                }
                PolicyDecisionType::Response => {
                    decision.use_();
                    glib::Propagation::Proceed
                }
                _ => {
                    decision.use_();
                    glib::Propagation::Proceed
                }
            }
        });
    }

    // --- window.open / target=_blank -> new tab ---
    {
        let win2 = std::rc::Rc::downgrade(win);
        webview.connect_create(move |_, action| {
            let Some(win2) = win2.upgrade() else { return None };
            let uri = action
                .request()
                .and_then(|r| r.uri())
                .map(|u| u.to_string())
                .unwrap_or_default();
            let tab = win2.new_tab(&normalize_uri(&uri), false, false);
            let widget: gtk::Widget = tab.webview.clone().upcast();
            Some(widget)
        });
    }

    // --- custom context menu ---
    {
        let win2 = std::rc::Rc::downgrade(win);
        webview.connect_context_menu(move |_, _menu, hit| {
            let Some(win2) = win2.upgrade() else { return true };
            let is_link = hit.context_is_link();
            let link = hit.link_uri().map(|u| u.to_string()).unwrap_or_default();
            crate::menus::show_web_context_menu(&win2, is_link, &link);
            true // suppress default menu
        });
    }

    // --- script dialogs (alert/confirm/prompt/beforeunload) ---
    {
        webview.connect_script_dialog(move |_, dialog| {
            handle_script_dialog(dialog);
            true
        });
    }

    // --- permissions ---
    {
        let state2 = state.clone();
        webview.connect_permission_request(move |_, request| {
            crate::perms::handle(&state2, request);
            true // decided by our handler
        });
    }

    // --- mouse target -> status bar ---
    {
        let win2 = std::rc::Rc::downgrade(win);
        webview.connect_mouse_target_changed(move |_, hit, _| {
            let Some(win2) = win2.upgrade() else { return };
            if hit.context_is_link() {
                let uri = hit.link_uri().map(|u| u.to_string()).unwrap_or_default();
                win2.show_status(&uri);
            } else {
                win2.show_status("");
            }
        });
    }

    // --- shield audit: count real tracker/ads requests observed ---
    {
        let state2 = state.clone();
        let tab2 = tab.clone();
        let win2 = std::rc::Rc::downgrade(win);
        webview.connect_resource_load_started(move |_, _resource, request| {
            let Some(win2) = win2.upgrade() else { return };
            let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
            if state2.shield.borrow().is_tracker(&uri) {
                tab2.shield_hits.set(tab2.shield_hits.get() + 1);
                win2.refresh_chrome();
            }
        });
    }

    // --- load failures (TLS, network) ---
    {
        let win2 = std::rc::Rc::downgrade(win);
        webview.connect_load_failed(move |_v, _event, _uri, error| {
            let Some(win2) = win2.upgrade() else { return false };
            let msg = error.to_string();
            if msg.to_lowercase().contains("certificate") || msg.to_lowercase().contains("tls") {
                win2.show_status("TLS certificate error — connection blocked");
                win2.toast_text("Blocked an invalid TLS certificate");
            } else {
                win2.show_status(&format!("Load failed: {msg}"));
            }
            false // let WebKit show its error page
        });
    }

    // --- web process crash ---
    {
        let win2 = std::rc::Rc::downgrade(win);
        let tab2 = tab.clone();
        webview.connect_web_process_terminated(move |_, reason| {
            let Some(win2) = win2.upgrade() else { return };
            let msg = match reason {
                webkit::WebProcessTerminationReason::ExceededMemoryLimit => {
                    "Page crashed: out of memory".to_string()
                }
                _ => "Page crashed (renderer error)".to_string(),
            };
            win2.toast_text(&msg);
            tab2.set_title_text("Crashed");
        });
    }
}

fn on_finished(state: &Rc<AppState>, win: &Rc<BrowserWindow>, tab: &Rc<Tab>, v: &WebView) {
    let uri = tab.uri();
    let title = v.title().map(|t| t.to_string()).unwrap_or_default();

    // Record history (never for private tabs).
    if !tab.private && (uri.starts_with("http://") || uri.starts_with("https://")) {
        kestrel_data::history::add(&state.db.borrow(), &uri, &title);
    }

    // Push state into internal pages.
    if tab.is_internal_view {
        crate::bridge::push_state(v, state);
    }

    win.refresh_chrome();
}

fn inject_cosmetic(state: &Rc<AppState>, v: &WebView) {
    let Some(engine) = state.engine.borrow().as_ref().cloned() else { return };
    let Some(uri) = v.uri().map(|u| u.to_string()) else { return };
    if uri.starts_with("kestrel://") || uri.starts_with("about:") || uri.starts_with("data:") {
        return;
    }
    let cosmetic = engine.cosmetic(&uri);
    let count = cosmetic.hide_css.len() as u64;
    if count == 0 && cosmetic.injected_script.is_empty() {
        return;
    }
    let mut js = String::new();
    if !cosmetic.hide_css.is_empty() {
        let selectors: Vec<String> = cosmetic
            .hide_css
            .iter()
            .map(|s| serde_json::to_string(s).unwrap_or_default())
            .collect();
        js.push_str("(function(){try{var css=document.createElement('style');css.textContent=");
        js.push_str(&format!(
            "[{}].map(function(s){{return s+'{{display:none !important}}'}}).join('\\n');",
            selectors.join(",")
        ));
        js.push_str("(document.head||document.documentElement).appendChild(css);}catch(e){}})();");
    }
    if !cosmetic.injected_script.is_empty() {
        js.push_str(&cosmetic.injected_script);
    }
    let _ = v.evaluate_javascript(&js, None::<&str>, None::<&str>, None::<&gio::Cancellable>, |_| {});

    if count > 0 {
        state.settings.borrow_mut().lifetime_blocked += count;
        // Cheap persistence: debounce via the same settings save on tab close.
    }
}

fn fav_char(uri: &str) -> String {
    crate::session::host_of(uri)
        .and_then(|h| h.chars().next())
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "K".into())
}

fn handle_script_dialog(dialog: &webkit::ScriptDialog) {
    use webkit::ScriptDialogType;
    match dialog.dialog_type() {
        ScriptDialogType::Alert => {
            let msg = dialog.message().map(|m| m.to_string()).unwrap_or_default();
            let d = gtk::AlertDialog::builder()
                .message("Page says")
                .detail(msg)
                .modal(true)
                .build();
            d.show(None::<&gtk::Window>);
            dialog.close();
        }
        ScriptDialogType::Confirm | ScriptDialogType::BeforeUnloadConfirm => {
            let msg = dialog.message().map(|m| m.to_string()).unwrap_or_default();
            dialog.confirm_set_confirmed(quick_confirm(&msg));
            dialog.close();
        }
        ScriptDialogType::Prompt => {
            let msg = dialog.message().map(|m| m.to_string()).unwrap_or_default();
            let default_text = dialog.prompt_get_default_text().map(|t| t.to_string());
            let reply = quick_prompt(&msg, default_text);
            dialog.prompt_set_text(reply.as_deref().unwrap_or(""));
            dialog.close();
        }
        _ => {
            dialog.close();
        }
    }
}

/// Synchronous modal confirm using a nested main loop (GTK4 pattern).
fn quick_confirm(msg: &str) -> bool {
    let dialog = gtk::MessageDialog::builder()
        .text(msg)
        .buttons(gtk::ButtonsType::YesNo)
        .modal(true)
        .title("Kestrel")
        .build();
    let (tx, rx) = std::sync::mpsc::channel::<gtk::ResponseType>();
    dialog.connect_response(glib::clone!(
        #[strong]
        tx,
        move |d, r| {
            d.hide();
            let _ = tx.send(r);
        }
    ));
    dialog.present();
    let answer = rx.recv().unwrap_or(gtk::ResponseType::No);
    dialog.close();
    answer == gtk::ResponseType::Yes
}

fn quick_prompt(msg: &str, default_text: Option<String>) -> Option<String> {
    let dialog = gtk::Dialog::builder().title("Kestrel").modal(true).build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("OK", gtk::ResponseType::Ok);
    let content = dialog.content_area();
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_spacing(8);
    let label = gtk::Label::new(Some(msg));
    let entry = gtk::Entry::new();
    if let Some(t) = default_text {
        entry.set_text(&t);
    }
    content.append(&label);
    content.append(&entry);
    let (tx, rx) = std::sync::mpsc::channel::<gtk::ResponseType>();
    dialog.connect_response(glib::clone!(
        #[strong]
        tx,
        move |d, r| {
            d.hide();
            let _ = tx.send(r);
        }
    ));
    dialog.present();
    entry.grab_focus();
    let answer = rx.recv().unwrap_or(gtk::ResponseType::Cancel);
    let text = entry.text().to_string();
    dialog.close();
    if answer == gtk::ResponseType::Ok {
        Some(text)
    } else {
        None
    }
}

pub fn open_external(uri: &str) {
    let _ = std::process::Command::new("xdg-open").arg(uri).spawn();
}
