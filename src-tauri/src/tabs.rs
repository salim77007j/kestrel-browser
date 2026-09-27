//! Window + tab management: one OS window, the chrome webview pinned to the
//! top, and one webview per tab below it (Tauri `unstable` multiwebview).
//!
//! NOTE (Windows): creating/closing webviews must never happen synchronously
//! on the main thread — commands that do are `async` and `add_child` runs
//! through `run_on_main_thread` internally.

use crate::bridge;
use crate::intercept;
use crate::state::{now_ms, AppState, TabMeta, CHROME_LABEL};
use kestrel_data::GroupInfo;
use tauri::webview::{DownloadEvent, PageLoadEvent, WebviewUrl, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview};

pub const HOME_PAGE: &str = "pages/newtab.html";

pub fn create_main_window(app: &AppHandle) -> tauri::Result<()> {
    let window = tauri::window::WindowBuilder::new(app, "main")
        .title("Kestrel")
        .inner_size(1280.0, 832.0)
        .min_inner_size(960.0, 600.0)
        .decorations(false)
        .transparent(false)
        .visible(true)
        .build()?;

    // Chrome UI: tab strip + toolbar + bookmarks bar. Full-height at first;
    // the frontend reports its real height via set_chrome_height.
    let state = app.state::<AppState>();
    let chrome_h = state.chrome_height.load(std::sync::atomic::Ordering::SeqCst) as f64;
    let size = window.inner_size()?.to_logical(window.scale_factor());
    let chrome = WebviewBuilder::new(CHROME_LABEL, WebviewUrl::App("chrome.html".into()))
        .initialization_script(bridge::chrome_init());
    window.add_child(
        chrome,
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(size.width, chrome_h.max(1.0)),
    )?;

    // Restore session tabs or start with the home page.
    let restored: Vec<TabMeta> = {
        let session = state.session.lock().unwrap();
        session
            .tabs
            .iter()
            .map(|t| TabMeta {
                id: String::new(),
                url: t.url.clone(),
                title: t.title.clone(),
                pinned: t.pinned,
                muted: t.muted,
                loading: false,
                can_back: false,
                can_forward: false,
                zoom: t.zoom,
                group: t.group.clone(),
                favicon: None,
            })
            .collect()
    };

    if restored.is_empty() {
        let _ = new_tab(app, None, false, None);
    } else {
        for (i, t) in restored.iter().enumerate() {
            let url = if t.url.is_empty() { None } else { Some(t.url.clone()) };
            let meta = new_tab(app, url, i > 0, None);
            if let Ok(meta) = meta {
                let mut tabs = state.tabs.lock().unwrap();
                if let Some(m) = tabs.iter_mut().find(|m| m.id == meta.id) {
                    m.pinned = t.pinned;
                    m.group = t.group.clone();
                }
                drop(tabs);
                if t.muted {
                    if let Some(wv) = app.get_webview(&meta.id) {
                        let _ = crate::platform::set_muted(&wv, true);
                    }
                    let mut tabs = state.tabs.lock().unwrap();
                    if let Some(m) = tabs.iter_mut().find(|m| m.id == meta.id) {
                        m.muted = true;
                    }
                }
                if t.active && i == 0 {
                    activate_tab_by_id(app, &meta.id, false);
                }
            }
        }
        // ensure SOME tab is active
        if state.active_tab.lock().unwrap().is_none() {
            let first = state.tabs.lock().unwrap().first().map(|t| t.id.clone());
            if let Some(f) = first {
                activate_tab_by_id(app, &f, false);
            }
        }
    }

    let app2 = app.clone();
    window.on_window_event(move |e| match e {
        tauri::WindowEvent::Resized(_) => {
            relayout(&app2);
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            let app3 = app2.clone();
            api.prevent_close();
            std::thread::spawn(move || {
                request_close(&app3);
            });
        }
        _ => {}
    });

    state.emit_tabs(app);
    Ok(())
}

/// Ask the chrome UI to confirm quitting when several tabs are open.
pub fn request_close(app: &AppHandle) {
    let state = app.state::<AppState>();
    let many = state.tabs.lock().unwrap().len() > 1;
    let confirm = state.settings.read().unwrap().confirm_close_multiple;
    if many && confirm {
        let _ = app.emit_to(CHROME_LABEL, "confirm-quit", true);
    } else {
        force_quit(app);
    }
}

pub fn force_quit(app: &AppHandle) {
    let state = app.state::<AppState>();
    {
        let mut session = state.session.lock().unwrap();
        session.clean_exit = true;
        session.save(&kestrel_data::JsonStore::new(state.data_dir.join("session.json")));
    }
    crate::stats::save_stats(state);
    app.exit(0);
}

pub fn is_internal(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.starts_with("http://tauri.localhost")
        || lower.starts_with("https://tauri.localhost")
        || lower.starts_with("http://kestrel.localhost")
        || lower.starts_with("https://kestrel.localhost")
        || lower.starts_with("kestrel://")
        || lower.starts_with("tauri://")
        || lower.starts_with("about:blank")
        || lower.is_empty()
}

pub fn internal_url(page: &str) -> String {
    if cfg!(any(target_os = "windows", target_os = "android")) {
        format!("https://tauri.localhost/{page}")
    } else {
        format!("tauri://localhost/{page}")
    }
}

/// Create a tab (optionally loading `url`). Returns the created tab metadata.
/// Must be called off the main thread (async command or dedicated thread).
pub fn new_tab(
    app: &AppHandle,
    url: Option<String>,
    background: bool,
    after_id: Option<String>,
) -> Result<TabMeta, String> {
    let state = app.state::<AppState>();
    let id = state.new_tab_id();
    let target = url.clone();

    let window = app
        .get_window("main")
        .ok_or_else(|| "main window missing".to_string())?;

    let builder = tab_webview_builder(app, &id);
    let ((_, y), (w, h)) = content_bounds(app);
    let webview = window
        .add_child(builder, LogicalPosition::new(0.0, y), LogicalSize::new(w, h))
        .map_err(|e| e.to_string())?;
    let _ = &webview;

    let start = internal_url(HOME_PAGE);
    let meta = TabMeta {
        id: id.clone(),
        url: target.clone().unwrap_or(start),
        title: String::from("New Tab"),
        pinned: false,
        muted: false,
        loading: target.is_some(),
        can_back: false,
        can_forward: false,
        zoom: 1.0,
        group: None,
        favicon: None,
    };

    {
        let mut tabs = state.tabs.lock().unwrap();
        let pos = match &after_id {
            Some(a) => tabs
                .iter()
                .position(|t| t.id == *a)
                .map(|p| p + 1)
                .unwrap_or(tabs.len()),
            None => match state.settings.read().unwrap().new_tab_position {
                kestrel_data::NewTabPosition::AfterCurrent => {
                    let active = state.active_tab.lock().unwrap().clone();
                    active
                        .and_then(|a| tabs.iter().position(|t| t.id == a))
                        .map(|p| p + 1)
                        .unwrap_or(tabs.len())
                }
                kestrel_data::NewTabPosition::End => tabs.len(),
            },
        };
        tabs.insert(pos, meta.clone());
    }
    if !background {
        activate_tab_by_id(app, &id, false);
    } else {
        state.emit_tabs(app);
    }

    // Navigate to the target once the internal start page has loaded.
    if let Some(u) = target {
        let app2 = app.clone();
        let id2 = id.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let _ = app2.run_on_main_thread(move || {
                if let Some(wv) = app2.get_webview(&id2) {
                    if let Ok(parsed) = normalize_url(&u) {
                        let _ = wv.navigate(parsed);
                    }
                }
            });
        });
    }

    persist_session(app);
    Ok(meta)
}

/// Normalize user input into a navigable URL: bare domains get https://,
/// search terms are turned into a search-engine query.
pub fn normalize_url(input: &str) -> Result<url::Url, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("empty".into());
    }
    if s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("file://")
        || s.starts_with("about:")
    {
        return url::Url::parse(s).map_err(|e| e.to_string());
    }
    let host_like = !s.contains(char::is_whitespace)
        && (s.starts_with("localhost")
            || s.contains('.')
            || s.parse::<std::net::IpAddr>().is_ok());
    if host_like {
        url::Url::parse(&format!("https://{s}")).map_err(|e| e.to_string())
    } else {
        let engine = {
            // engine search URL is resolved by the caller via state; a bare
            // DDG fallback keeps this helper stateless
            format!(
                "https://duckduckgo.com/?q={}",
                urlencode(s)
            )
        };
        url::Url::parse(&engine).map_err(|e| e.to_string())
    }
}

pub fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

pub fn tab_webview_builder(app: &AppHandle, id: &str) -> WebviewBuilder<tauri::Wry> {
    let page_bridge = bridge::page_init_with_privacy(app);
    let fp = bridge::fingerprint_init(app);

    let app_req = app.clone();
    let id_req = id.to_string();
    let mut builder = WebviewBuilder::new(id, WebviewUrl::App(HOME_PAGE.into()))
        .initialization_script(page_bridge)
        .use_https_scheme(true)
        .on_web_resource_request(move |req, res| {
            intercept::handle_request(&app_req, &id_req, req, res);
        });

    if !fp.is_empty() {
        builder = builder.initialization_script(fp);
    }

    {
        let app3 = app.clone();
        let id3 = id.to_string();
        builder = builder.on_navigation(move |url| intercept::allow_navigation(&app3, &id3, url));
    }

    {
        let app4 = app.clone();
        let id4 = id.to_string();
        builder = builder.on_page_load(move |wv, payload| {
            intercept::on_page_load(&app4, &id4, wv, payload.event());
        });
    }

    {
        let app5 = app.clone();
        let id5 = id.to_string();
        builder = builder.on_document_title_changed(move |wv, title| {
            intercept::on_title(&app5, &id5, wv, title);
        });
    }

    {
        let app6 = app.clone();
        builder = builder.on_new_window(move |url, _features| {
            // Popups become tabs.
            let appc = app6.clone();
            let u = url.to_string();
            std::thread::spawn(move || {
                let _ = appc.run_on_main_thread(move || {
                    let _ = new_tab(&appc, Some(u.clone()), false, None);
                });
            });
            tauri::webview::NewWindowResponse::Deny
        });
    }

    {
        let app7 = app.clone();
        builder = builder.on_download(move |wv, event| handle_download(&app7, wv, event));
    }

    {
        let app8 = app.clone();
        builder =
            builder.on_permission_request(move |_wv, kind| intercept::permission_decision(&app8, kind));
    }

    builder
}

fn handle_download(app: &AppHandle, _wv: Webview<tauri::Wry>, event: DownloadEvent<'_>) -> bool {
    let state = app.state::<AppState>();
    match event {
        DownloadEvent::Requested { url, destination } => {
            let (dir, ask) = {
                let s = state.settings.read().unwrap();
                (s.download_dir.clone(), s.download_ask_each_time)
            };
            let dir = if dir.is_empty() {
                dirs::download_dir()
                    .map(|d| d.to_string_lossy().to_string())
                    .unwrap_or_else(|| std::env::temp_dir().to_string_lossy().to_string())
            } else {
                dir
            };
            let fname = crate::net::sanitize_filename(destination);
            let mut target = std::path::PathBuf::from(&dir).join(&fname);
            if ask {
                if let Some(chosen) = rfd::FileDialog::new()
                    .set_file_name(&fname)
                    .set_directory(&dir)
                    .save_file()
                {
                    target = chosen;
                }
            }
            *destination = target.clone();
            let did = state.downloads.lock().unwrap().begin(
                url.as_str(),
                &target.to_string_lossy(),
                now_ms(),
            );
            crate::net::spawn_download_poller(app.clone(), did, target);
            let _ = app.emit_to(
                CHROME_LABEL,
                "downloads-changed",
                state.downloads.lock().unwrap().list().to_vec(),
            );
            true
        }
        DownloadEvent::Finished { path, success, .. } => {
            let size = path
                .as_ref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0);
            let rec_id = {
                let dls = state.downloads.lock().unwrap();
                let path_str = path.as_ref().map(|p| p.to_string_lossy().to_string());
                dls.list()
                    .iter()
                    .find(|d| {
                        d.state == kestrel_data::DownloadState::Active
                            && path_str.as_deref().map_or(true, |ps| d.path == ps)
                    })
                    .map(|d| d.id)
            };
            if let Some(rid) = rec_id {
                state.downloads.lock().unwrap().finish(rid, success, size);
                if let Some(flag) = state.active_downloads.lock().unwrap().remove(&rid) {
                    flag.store(false, std::sync::atomic::Ordering::SeqCst);
                }
            }
            let _ = app.emit_to(
                CHROME_LABEL,
                "downloads-changed",
                state.downloads.lock().unwrap().list().to_vec(),
            );
            true
        }
        _ => true,
    }
}

/// (position, size) of the content area: ((x, y), (w, h)).
pub fn content_bounds(app: &AppHandle) -> ((f64, f64), (f64, f64)) {
    let window = app.get_window("main").expect("main window");
    let size = window.inner_size().unwrap_or_default();
    let scale = window.scale_factor().unwrap_or(1.0);
    let w = size.width as f64 / scale;
    let h = size.height as f64 / scale;
    let state = app.state::<AppState>();
    let ch = state.chrome_height.load(std::sync::atomic::Ordering::SeqCst) as f64;
    ((0.0, ch), (w, (h - ch).max(1.0)))
}

/// Recalculate the bounds of every webview (resize, tab switch, group
/// collapse, bookmarks bar toggle ...).
pub fn relayout(app: &AppHandle) {
    let Some(window) = app.get_window("main") else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    if size.width == 0 {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let w = size.width as f64 / scale;
    let h = size.height as f64 / scale;
    let state = app.state::<AppState>();
    let ch = state.chrome_height.load(std::sync::atomic::Ordering::SeqCst) as f64;
    let active = state.active_tab.lock().unwrap().clone();

    if let Some(c) = window.get_webview(CHROME_LABEL) {
        let _ = c.set_bounds(tauri::Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(w, ch).into(),
        });
    }

    let metas = state.tabs.lock().unwrap().clone();
    for m in metas {
        if let Some(wv) = window.get_webview(&m.id) {
            let group_collapsed = m.group.as_ref().map(|g| g.collapsed).unwrap_or(false);
            let is_active = active.as_deref() == Some(m.id.as_str());
            if group_collapsed || !is_active {
                let _ = wv.hide();
            } else {
                let _ = wv.show();
                let _ = wv.set_bounds(tauri::Rect {
                    position: LogicalPosition::new(0.0, ch).into(),
                    size: LogicalSize::new(w, (h - ch).max(1.0)).into(),
                });
            }
        }
    }
}

pub fn activate_tab_by_id(app: &AppHandle, id: &str, do_relayout: bool) {
    let state = app.state::<AppState>();
    {
        let mut active = state.active_tab.lock().unwrap();
        *active = Some(id.to_string());
    }
    if do_relayout {
        relayout(app);
    }
    state.emit_tabs(app);
    persist_session(app);
}

pub fn persist_session(app: &AppHandle) {
    let state = app.state::<AppState>();
    let tabs = state.tabs.lock().unwrap().clone();
    let active = state.active_tab.lock().unwrap().clone();
    let mut session = state.session.lock().unwrap();
    session.tabs = tabs
        .iter()
        .map(|t| kestrel_data::TabState {
            url: if is_internal(&t.url) { String::new() } else { t.url.clone() },
            title: t.title.clone(),
            pinned: t.pinned,
            muted: t.muted,
            group: t.group.clone(),
            zoom: t.zoom,
            active: active.as_deref() == Some(t.id.as_str()),
        })
        .collect();
    session.save(&kestrel_data::JsonStore::new(state.data_dir.join("session.json")));
}

pub fn close_tab(app: &AppHandle, id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let was_active;
    {
        let mut tabs = state.tabs.lock().unwrap();
        let pos = tabs.iter().position(|t| t.id == id).ok_or("no such tab")?;
        let m = tabs.remove(pos);
        was_active = state.active_tab.lock().unwrap().as_deref() == Some(m.id.as_str());
        if !is_internal(&m.url) {
            let mut session = state.session.lock().unwrap();
            session.push_closed(m.url.clone(), m.title.clone());
        }
    }

    if let Some(wv) = app.get_webview(id) {
        let _ = wv.close();
    }

    if was_active {
        let next = {
            let tabs = state.tabs.lock().unwrap();
            tabs.last().map(|t| t.id.clone())
        };
        if let Some(n) = next {
            activate_tab_by_id(app, &n, false);
        } else {
            *state.active_tab.lock().unwrap() = None;
        }
    }

    if state.tabs.lock().unwrap().is_empty() {
        // last tab closed: open a fresh home page
        new_tab(app, None, false, None)?;
    } else {
        relayout(app);
        state.emit_tabs(app);
        persist_session(app);
    }
    Ok(())
}

pub fn set_tab_pinned(app: &AppHandle, id: &str, pinned: bool) {
    let state = app.state::<AppState>();
    {
        let mut tabs = state.tabs.lock().unwrap();
        if let Some(m) = tabs.iter_mut().find(|t| t.id == id) {
            m.pinned = pinned;
        }
        tabs.sort_by_key(|t| !t.pinned);
    }
    state.emit_tabs(app);
    persist_session(app);
}

pub fn set_tab_muted(app: &AppHandle, id: &str, muted: bool) {
    let state = app.state::<AppState>();
    if let Some(wv) = app.get_webview(id) {
        if !crate::platform::set_muted(&wv, muted) {
            let _ = app.emit_to(
                CHROME_LABEL,
                "toast",
                serde_json::json!({"level": "warn", "text": "Tab muting is not supported by this WebView2 runtime"}),
            );
            return;
        }
    }
    {
        let mut tabs = state.tabs.lock().unwrap();
        if let Some(m) = tabs.iter_mut().find(|t| t.id == id) {
            m.muted = muted;
        }
    }
    state.emit_tabs(app);
    persist_session(app);
}

pub fn group_tabs(app: &AppHandle, ids: &[String], name: &str, color: &str) {
    let state = app.state::<AppState>();
    let gid = format!("grp-{}", now_ms());
    let info = GroupInfo {
        id: gid,
        name: name.to_string(),
        color: color.to_string(),
        collapsed: false,
    };
    {
        let mut tabs = state.tabs.lock().unwrap();
        for t in tabs.iter_mut() {
            if ids.iter().any(|i| *i == t.id) {
                t.group = Some(info.clone());
            }
        }
        // keep pinned tabs first, grouped tabs adjacent (stable)
        tabs.sort_by_key(|t| {
            let g = t.group.is_some();
            let p = t.pinned;
            (!p, !g)
        });
    }
    state.emit_tabs(app);
    persist_session(app);
}

pub fn ungroup_tab(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    {
        let mut tabs = state.tabs.lock().unwrap();
        if let Some(m) = tabs.iter_mut().find(|t| t.id == id) {
            m.group = None;
        }
    }
    state.emit_tabs(app);
    persist_session(app);
}

pub fn set_group_collapsed(app: &AppHandle, group_id: &str, collapsed: bool) {
    let state = app.state::<AppState>();
    {
        let mut tabs = state.tabs.lock().unwrap();
        for t in tabs.iter_mut() {
            if let Some(g) = t.group.as_mut() {
                if g.id == group_id {
                    g.collapsed = collapsed;
                }
            }
        }
    }
    relayout(app);
    state.emit_tabs(app);
    persist_session(app);
}
