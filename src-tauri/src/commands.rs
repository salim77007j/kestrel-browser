//! All Tauri commands. Commands that create or destroy webviews are `async`
//! (Windows multiwebview deadlocks if done synchronously on the main
//! thread); pure state commands stay sync.

use crate::intercept;
use crate::omnibox::Suggestion;
use crate::state::{now_ms, AppState, TabMeta, CHROME_LABEL};
use crate::tabs;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

type Result<T> = std::result::Result<T, String>;

fn state<'a>(app: &'a AppHandle) -> tauri::State<'a, AppState> {
    app.state::<AppState>()
}

// ---------------------------------------------------------------------------
// bootstrap
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn init_ui(app: AppHandle) -> Value {
    let s = state(&app);
    let tabs = s.snapshot_tabs();
    let settings = s.settings.read().unwrap().clone();
    let bookmarks = {
        let b = s.bookmarks.lock().unwrap();
        json!({"bar": b.bar(), "other": b.other()})
    };
    let engine_rules = s.engine_rules.load(std::sync::atomic::Ordering::SeqCst);
    let stats = s.stats.lock().unwrap();
    json!({
        "tabs": tabs.tabs,
        "active": tabs.active,
        "settings": settings,
        "bookmarks": bookmarks,
        "engineRules": engine_rules,
        "engineReady": s.engine_ready.load(std::sync::atomic::Ordering::SeqCst),
        "stats": {
            "blockedToday": stats.blocked_today,
            "trackersToday": stats.trackers_today,
            "requestsToday": stats.requests_today,
        }
    })
}

// ---------------------------------------------------------------------------
// tabs
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn create_tab(
    app: AppHandle,
    url: Option<String>,
    background: Option<bool>,
    after_id: Option<String>,
) -> Result<TabMeta> {
    let app2 = app.clone();
    let res = std::thread::spawn(move || {
        tabs::new_tab(&app2, url, background.unwrap_or(false), after_id)
    })
    .join()
    .map_err(|_| "thread panicked")??;
    Ok(res)
}

#[tauri::command]
pub async fn close_tab(app: AppHandle, id: String) -> Result<()> {
    let app2 = app.clone();
    std::thread::spawn(move || tabs::close_tab(&app2, &id))
        .join()
        .map_err(|_| "thread panicked")?
}

#[tauri::command]
pub async fn close_other_tabs(app: AppHandle, id: String) -> Result<()> {
    let app2 = app.clone();
    std::thread::spawn(move || -> Result<()> {
        let s = app2.state::<AppState>();
        let ids: Vec<String> = s
            .tabs
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.id != id)
            .map(|t| t.id.clone())
            .collect();
        for i in ids {
            tabs::close_tab(&app2, &i)?;
        }
        Ok(())
    })
    .join()
    .map_err(|_| "thread panicked")?
}

#[tauri::command]
pub async fn activate_tab(app: AppHandle, id: String) -> Result<()> {
    let app2 = app.clone();
    std::thread::spawn(move || -> Result<()> {
        tabs::activate_tab_by_id(&app2, &id, true);
        Ok(())
    })
    .join()
    .map_err(|_| "thread panicked")?
}

#[tauri::command]
pub fn reorder_tab(app: AppHandle, id: String, index: usize) -> Result<()> {
    let s = state(&app);
    {
        let mut t = s.tabs.lock().unwrap();
        if let Some(pos) = t.iter().position(|x| x.id == id) {
            let m = t.remove(pos);
            let idx = index.min(t.len());
            t.insert(idx, m);
        }
    }
    s.emit_tabs(&app);
    tabs::persist_session(&app);
    Ok(())
}

#[tauri::command]
pub fn pin_tab(app: AppHandle, id: String, pinned: bool) -> Result<()> {
    tabs::set_tab_pinned(&app, &id, pinned);
    Ok(())
}

#[tauri::command]
pub fn mute_tab(app: AppHandle, id: String, muted: bool) -> Result<()> {
    tabs::set_tab_muted(&app, &id, muted);
    Ok(())
}

#[tauri::command]
pub async fn duplicate_tab(app: AppHandle, id: String) -> Result<TabMeta> {
    let app2 = app.clone();
    let res = std::thread::spawn(move || -> Result<TabMeta> {
        let s = app2.state::<AppState>();
        let url = s
            .tabs
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.url.clone());
        let _ = &s;
        drop(s);
        tabs::new_tab(&app2, url, false, Some(id))
    })
    .join()
    .map_err(|_| "thread panicked")??;
    Ok(res)
}

#[tauri::command]
pub async fn reopen_closed_tab(app: AppHandle) -> Result<Option<TabMeta>> {
    let app2 = app.clone();
    let res = std::thread::spawn(move || -> Result<Option<TabMeta>> {
        let s = app2.state::<AppState>();
        let entry = s.session.lock().unwrap().pop_closed();
        match entry {
            Some(c) => Ok(Some(tabs::new_tab(&app2, Some(c.url), false, None)?)),
            None => Ok(None),
        }
    })
    .join()
    .map_err(|_| "thread panicked")??;
    Ok(res)
}

#[tauri::command]
pub fn group_tabs(app: AppHandle, ids: Vec<String>, name: String, color: String) -> Result<()> {
    tabs::group_tabs(&app, &ids, &name, &color);
    Ok(())
}

#[tauri::command]
pub fn ungroup_tab(app: AppHandle, id: String) -> Result<()> {
    tabs::ungroup_tab(&app, &id);
    Ok(())
}

#[tauri::command]
pub fn set_group_collapsed(app: AppHandle, group_id: String, collapsed: bool) -> Result<()> {
    tabs::set_group_collapsed(&app, &group_id, collapsed);
    Ok(())
}

// ---------------------------------------------------------------------------
// navigation
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn navigate_active(app: AppHandle, input: String) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone().ok_or("no active tab")?;
    // omnibox semantics: URL or search query
    let url = if input.contains(' ')
        && !(input.starts_with("http://") || input.starts_with("https://"))
    {
        crate::omnibox::search_url_for(&s, &input)
    } else {
        match tabs::normalize_url(&input) {
            Ok(u) => u.to_string(),
            Err(_) => crate::omnibox::search_url_for(&s, &input),
        }
    };
    if let Some(wv) = app.get_webview(&active) {
        wv.navigate(url.parse().map_err(|e: url::ParseError| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn navigate_tab(app: AppHandle, id: String, url: String) -> Result<()> {
    if let Some(wv) = app.get_webview(&id) {
        wv.navigate(url.parse().map_err(|e: url::ParseError| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn nav_back(app: AppHandle) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone().ok_or("no active tab")?;
    if let Some(wv) = app.get_webview(&active) {
        if !crate::platform::go_back(&wv) {
            return Err("cannot go back".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn nav_forward(app: AppHandle) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone().ok_or("no active tab")?;
    if let Some(wv) = app.get_webview(&active) {
        if !crate::platform::go_forward(&wv) {
            return Err("cannot go forward".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn reload_active(app: AppHandle) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone().ok_or("no active tab")?;
    if let Some(wv) = app.get_webview(&active) {
        wv.reload().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn stop_active(app: AppHandle) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone();
    if let Some(id) = active {
        if let Some(wv) = app.get_webview(&id) {
            let _ = crate::platform::stop(&wv);
        }
        intercept::update_tab(&app, &id, |t| t.loading = false);
    }
    Ok(())
}

#[tauri::command]
pub fn set_zoom(app: AppHandle, level: f64) -> Result<()> {
    let s = state(&app);
    let level = level.clamp(0.25, 5.0);
    let active = s.active_tab.lock().unwrap().clone().ok_or("no active tab")?;
    if let Some(wv) = app.get_webview(&active) {
        wv.set_zoom(level).map_err(|e| e.to_string())?;
    }
    let host = {
        let tabs = s.tabs.lock().unwrap();
        tabs.iter()
            .find(|t| t.id == active)
            .and_then(|t| url::Url::parse(&t.url).ok())
            .and_then(|u| u.host_str().map(|h| h.to_string()))
    };
    if let Some(h) = host {
        s.zoom_by_host.lock().unwrap().insert(h, level);
        crate::state::save_zooms(&s);
    }
    intercept::update_tab(&app, &active, |t| t.zoom = level);
    Ok(())
}

// ---------------------------------------------------------------------------
// bookmarks
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn add_bookmark(app: AppHandle, title: String, url: String, to_other: Option<bool>) -> Result<()> {
    let s = state(&app);
    s.bookmarks
        .lock()
        .unwrap()
        .add(&title, &url, to_other.unwrap_or(false));
    let _ = app.emit_to(CHROME_LABEL, "bookmarks-changed", true);
    Ok(())
}

#[tauri::command]
pub fn remove_bookmark(app: AppHandle, id: u64) -> Result<()> {
    let s = state(&app);
    s.bookmarks.lock().unwrap().remove(id);
    let _ = app.emit_to(CHROME_LABEL, "bookmarks-changed", true);
    Ok(())
}

#[tauri::command]
pub fn rename_bookmark(app: AppHandle, id: u64, title: String) -> Result<()> {
    let s = state(&app);
    s.bookmarks.lock().unwrap().rename(id, &title);
    let _ = app.emit_to(CHROME_LABEL, "bookmarks-changed", true);
    Ok(())
}

#[tauri::command]
pub fn move_bookmark_to_bar(app: AppHandle, id: u64) -> Result<()> {
    let s = state(&app);
    s.bookmarks.lock().unwrap().move_to_bar(id);
    let _ = app.emit_to(CHROME_LABEL, "bookmarks-changed", true);
    Ok(())
}

#[tauri::command]
pub fn toggle_bookmark_current(app: AppHandle) -> Result<bool> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone();
    let (url, title) = {
        let tabs = s.tabs.lock().unwrap();
        let t = tabs
            .iter()
            .find(|t| Some(t.id.clone()) == active)
            .ok_or("no active tab")?;
        (t.url.clone(), t.title.clone())
    };
    if tabs::is_internal(&url) {
        return Err("internal pages cannot be bookmarked".into());
    }
    let host = url::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| url.clone());
    let title = if title.is_empty() { host } else { title };
    let added = s.bookmarks.lock().unwrap().toggle(&title, &url);
    let _ = app.emit_to(CHROME_LABEL, "bookmarks-changed", true);
    Ok(added)
}

#[tauri::command]
pub fn list_bookmarks(app: AppHandle) -> Value {
    let s = state(&app);
    let b = s.bookmarks.lock().unwrap();
    json!({"bar": b.bar(), "other": b.other()})
}

// ---------------------------------------------------------------------------
// history
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn history_search(app: AppHandle, query: String, limit: Option<usize>) -> Value {
    let s = state(&app);
    let items = s.history.lock().unwrap().search(&query, limit.unwrap_or(200));
    serde_json::to_value(items).unwrap_or(json!([]))
}

#[tauri::command]
pub fn delete_history_item(app: AppHandle, id: u64) -> Result<()> {
    let s = state(&app);
    s.history.lock().unwrap().delete(id);
    Ok(())
}

#[tauri::command]
pub fn forget_site(app: AppHandle, host: String) -> Result<()> {
    let s = state(&app);
    s.history.lock().unwrap().forget_site(&host);
    Ok(())
}

#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<()> {
    let s = state(&app);
    s.history.lock().unwrap().clear();
    Ok(())
}

// ---------------------------------------------------------------------------
// downloads
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_downloads(app: AppHandle) -> Value {
    let s = state(&app);
    let items = { s.downloads.lock().unwrap().list().to_vec() };
    serde_json::to_value(items).unwrap_or(json!([]))
}

#[tauri::command]
pub fn cancel_download(app: AppHandle, id: u64) -> Result<()> {
    let s = state(&app);
    s.downloads.lock().unwrap().cancel(id);
    if let Some(flag) = s.active_downloads.lock().unwrap().remove(&id) {
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
    }
    let _ = app.emit_to(CHROME_LABEL, "downloads-changed", list_downloads_inner(&s));
    Ok(())
}

fn list_downloads_inner(s: &State<AppState>) -> Value {
    let items = { s.downloads.lock().unwrap().list().to_vec() };
    serde_json::to_value(items).unwrap_or(json!([]))
}

#[tauri::command]
pub fn clear_finished_downloads(app: AppHandle) -> Result<()> {
    let s = state(&app);
    s.downloads.lock().unwrap().clear_finished();
    let _ = app.emit_to(CHROME_LABEL, "downloads-changed", list_downloads_inner(&s));
    Ok(())
}

#[tauri::command]
pub fn open_download(app: AppHandle, id: u64) -> Result<()> {
    let s = state(&app);
    let path = s
        .downloads
        .lock()
        .unwrap()
        .get(id)
        .map(|d| d.path.clone())
        .ok_or("not found")?;
    open::that(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn show_in_folder(app: AppHandle, id: u64) -> Result<()> {
    let s = state(&app);
    let path = s
        .downloads
        .lock()
        .unwrap()
        .get(id)
        .map(|d| d.path.clone())
        .ok_or("not found")?;
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("explorer")
            .args(["/select,", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(dir) = std::path::Path::new(&path).parent() {
            open::that(dir).map_err(|e| e.to_string())
        } else {
            Err("no parent".into())
        }
    }
}

// ---------------------------------------------------------------------------
// settings
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Value {
    let s = state(&app);
    let snapshot = { s.settings.read().unwrap().clone() };
    serde_json::to_value(&snapshot).unwrap_or(json!({}))
}

#[tauri::command]
pub fn update_settings(app: AppHandle, patch: Value) -> Result<Value> {
    let s = state(&app);
    {
        let mut settings = s.settings.write().unwrap();
        settings.apply_patch(&patch);
        let cloned = settings.clone();
        settings.save_to(&s.data_dir);
        drop(settings);
        // apply immediate effects
        apply_setting_effects(&app, &cloned, &patch);
    }
    let snapshot = { s.settings.read().unwrap().clone() };
    Ok(serde_json::to_value(&snapshot).unwrap_or(json!({})))
}

fn apply_setting_effects(app: &AppHandle, _settings: &kestrel_data::Settings, patch: &Value) {
    let s = state(app);
    // bookmarks bar visibility changes the chrome height -> relayout
    if patch.get("showBookmarksBar").is_some() {
        let _ = app.emit_to(CHROME_LABEL, "layout-changed", true);
    }
    // engine-affecting changes -> rebuild
    let engine_related = patch.get("adblockEnabled").is_some()
        || patch.get("filterLists").is_some()
        || patch.get("customFilters").is_some();
    if engine_related {
        crate::engine_host::spawn_engine_build(app.clone());
    }
    let _ = &s;
}

#[tauri::command]
pub fn add_custom_filter(app: AppHandle, rule: String) -> Result<()> {
    let s = state(&app);
    s.settings.write().unwrap().custom_filters.push(rule);
    s.settings.read().unwrap().save_to(&s.data_dir);
    crate::engine_host::spawn_engine_build(app.clone());
    Ok(())
}

#[tauri::command]
pub fn remove_custom_filter(app: AppHandle, rule: String) -> Result<()> {
    let s = state(&app);
    let mut set = s.settings.write().unwrap();
    set.custom_filters.retain(|f| *f != rule);
    set.save_to(&s.data_dir);
    drop(set);
    crate::engine_host::spawn_engine_build(app.clone());
    Ok(())
}

#[tauri::command]
pub async fn update_filter_lists_now(app: AppHandle) -> Result<String> {
    let app2 = app.clone();
    let report = std::thread::spawn(move || crate::engine_host::update_lists_blocking(&app2))
        .join()
        .map_err(|_| "thread panicked")?;
    Ok(report)
}

#[tauri::command]
pub fn get_adblock_stats(app: AppHandle) -> Value {
    let s = state(&app);
    let stats = s.stats.lock().unwrap();
    let rules = s.engine_rules.load(std::sync::atomic::Ordering::SeqCst);
    let top: Vec<Value> = stats
        .per_domain
        .iter()
        .rev()
        .take(8)
        .map(|(k, v)| json!({"domain": k, "count": v}))
        .collect();
    json!({
        "blockedToday": stats.blocked_today,
        "trackersToday": stats.trackers_today,
        "requestsToday": stats.requests_today,
        "blockedAll": stats.blocked_all,
        "trackersAll": stats.trackers_all,
        "rules": rules,
        "engineReady": s.engine_ready.load(std::sync::atomic::Ordering::SeqCst),
        "topDomains": top,
    })
}

#[tauri::command]
pub fn run_privacy_selftest(app: AppHandle) -> Value {
    let s = state(&app);
    let engine = s.engine.read().unwrap().clone();
    let mut results: Vec<Value> = Vec::new();
    let mut check = |name: &str, ok: bool, detail: &str| {
        results.push(json!({"name": name, "pass": ok, "detail": detail}))
    };

    match engine {
        Some(eng) => {
            let blocked = eng.should_block(
                "https://ad.doubleclick.net/ddm/adj/x",
                "https://news.example.com/",
                "script",
            );
            check("network block: ad server", blocked, "doubleclick.net script");
            let ok = !eng.should_block(
                "https://news.example.com/article",
                "https://news.example.com/",
                "document",
            );
            check("first-party passes", ok, "news.example.com document");
            let rules = s.engine_rules.load(std::sync::atomic::Ordering::SeqCst);
            check("cosmetic engine attached", rules > 10_000, &format!("{rules} rules loaded"));
            let _ = eng;
        }
        None => check("engine", false, "engine not ready yet"),
    }
    let fp = s.settings.read().unwrap().fingerprint.clone();
    check("fingerprint shields", fp.enabled, &format!("canvas={} audio={} webgl={} fonts={} hardware={}", fp.canvas, fp.audio, fp.webgl, fp.fonts, fp.hardware));
    let (cookies, https_only, safe) = {
        let st = s.settings.read().unwrap();
        (st.cookie_policy, st.https_only, st.safe_browsing)
    };
    check(
        "third-party cookie blocking",
        cookies == kestrel_privacy::headers::CookiePolicy::BlockThirdParty,
        "policy",
    );
    check("HTTPS-only upgrades", https_only, "policy");
    check(
        "safe browsing",
        safe,
        &format!("{} malware hosts known", s.safebrowsing.read().unwrap().len()),
    );
    json!(results)
}

// ---------------------------------------------------------------------------
// tools
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn open_devtools(app: AppHandle) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone().ok_or("no active tab")?;
    if let Some(wv) = app.get_webview(&active) {
        wv.open_devtools();
    }
    Ok(())
}

#[tauri::command]
pub async fn save_page(app: AppHandle) -> Result<String> {
    let (path, id) = {
        let s = state(&app);
        let active = s.active_tab.lock().unwrap().clone().ok_or("no active tab")?;
        let dir = {
            let st = s.settings.read().unwrap();
            if st.download_dir.is_empty() {
                dirs::download_dir()
                    .map(|d| d.to_path_buf())
                    .unwrap_or_else(std::env::temp_dir)
            } else {
                std::path::PathBuf::from(st.download_dir.clone())
            }
        };
        let host = {
            let tabs = s.tabs.lock().unwrap();
            tabs.iter()
                .find(|t| t.id == active)
                .and_then(|t| url::Url::parse(&t.url).ok())
                .and_then(|u| u.host_str().map(|h| h.to_string()))
                .unwrap_or_else(|| "page".into())
        };
        let path = dir.join(format!("kestrel-{host}-{timestamp}.html", timestamp = now_ms()));
        (path, active)
    };
    // ask the page bridge to send the DOM in chunks
    if let Some(wv) = app.get_webview(&id) {
        wv.eval("window.__kestrelSavePage && window.__kestrelSavePage();")
            .map_err(|e| e.to_string())?;
    } else {
        return Err("no webview".into());
    }
    // stash the target path; chunks arrive via page-event
    {
        let s = state(&app);
        s.pending_permissions
            .lock()
            .unwrap()
            .insert(format!("savepage:{id}"), (path.to_string_lossy().to_string(), String::new()));
    }
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn print_page(app: AppHandle) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone().ok_or("no active tab")?;
    if let Some(wv) = app.get_webview(&active) {
        wv.eval("window.print()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn find_in_page(app: AppHandle, query: String) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone().ok_or("no active tab")?;
    if let Some(wv) = app.get_webview(&active) {
        let q = query.replace('\\', "\\\\").replace('\'', "\\'");
        wv.eval(&format!(
            "window.__kestrelFind && window.__kestrelFind.find('{}');",
            q
        ))
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn find_step(app: AppHandle, dir: i32) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone().ok_or("no active tab")?;
    if let Some(wv) = app.get_webview(&active) {
        wv.eval(&format!(
            "window.__kestrelFind && window.__kestrelFind.step({dir});"
        ))
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn find_exit(app: AppHandle) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone();
    if let Some(id) = active {
        if let Some(wv) = app.get_webview(&id) {
            let _ = wv.eval("window.__kestrelFind && window.__kestrelFind.exit();");
        }
    }
    Ok(())
}

#[tauri::command]
pub fn set_chrome_height(app: AppHandle, height: f64) -> Result<()> {
    let s = state(&app);
    s.chrome_height
        .store(height.round().max(40.0) as u32, std::sync::atomic::Ordering::SeqCst);
    crate::tabs::relayout(&app);
    Ok(())
}

#[tauri::command]
pub fn get_favicon(app: AppHandle, host: String) -> Result<Option<String>> {
    let s = state(&app);
    let file = s.data_dir.join("favicons").join(format!("{host}.ico"));
    if let Ok(bytes) = std::fs::read(file) {
        use base64::Engine as _;
        return Ok(Some(format!(
            "data:image/x-icon;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )));
    }
    Ok(None)
}

#[tauri::command]
pub async fn omnibox_suggest(app: AppHandle, query: String) -> Result<Vec<Suggestion>> {
    let app2 = app.clone();
    let res = std::thread::spawn(move || crate::omnibox::suggest(&app2, &query))
        .join()
        .map_err(|_| "thread panicked")?;
    Ok(res)
}

// ---------------------------------------------------------------------------
// new tab page + misc
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn add_shortcut(app: AppHandle, title: String, url: String) -> Result<()> {
    let s = state(&app);
    s.shortcuts.lock().unwrap().add(&title, &url);
    let _ = app.emit_to(CHROME_LABEL, "shortcuts-changed", true);
    Ok(())
}

#[tauri::command]
pub fn get_shortcuts(app: AppHandle) -> Value {
    let s = state(&app);
    let items = { s.shortcuts.lock().unwrap().list().to_vec() };
    serde_json::to_value(items).unwrap_or(json!([]))
}

/// Navigate the active tab to an internal page (new tab as fallback).
#[tauri::command]
pub async fn open_internal(app: AppHandle, page: String) -> Result<()> {
    let s = state(&app);
    let active = s.active_tab.lock().unwrap().clone();
    let url = tabs::internal_url(&format!("pages/{page}"));
    if let Some(id) = active {
        if let Some(wv) = app.get_webview(&id) {
            wv.navigate(url.parse().map_err(|e: url::ParseError| e.to_string())?)
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    let app2 = app.clone();
    std::thread::spawn(move || {
        let _ = tabs::new_tab(&app2, Some(url), false, None);
    });
    Ok(())
}

#[tauri::command]
pub fn remove_shortcut(app: AppHandle, url: String) -> Result<()> {
    let s = state(&app);
    s.shortcuts.lock().unwrap().remove(&url);
    let _ = app.emit_to(CHROME_LABEL, "shortcuts-changed", true);
    Ok(())
}

#[tauri::command]
pub fn get_weather(app: AppHandle) -> Value {
    let s = state(&app);
    crate::net::get_weather(&s)
}

#[tauri::command]
pub fn allow_dangerous_site(app: AppHandle, url: String) -> Result<()> {
    let s = state(&app);
    let host = url::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .ok_or("bad url")?;
    s.allowed_dangerous.write().unwrap().insert(host);
    if let Some(wv) = app.get_webview(&s.active_tab.lock().unwrap().clone().unwrap_or_default()) {
        wv.navigate(url.parse().map_err(|e: url::ParseError| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_permission(app: AppHandle, origin: String, kind: String, decision: String) -> Result<()> {
    use kestrel_data::PermissionDecision;
    let s = state(&app);
    let d = match decision.as_str() {
        "allow" => PermissionDecision::Allow,
        _ => PermissionDecision::Deny,
    };
    if decision == "clear" {
        s.permissions.lock().unwrap().clear_origin(&origin);
    } else {
        s.permissions.lock().unwrap().set(&origin, &kind, d);
    }
    Ok(())
}

#[tauri::command]
pub fn list_permissions(app: AppHandle) -> Value {
    let s = state(&app);
    serde_json::to_value(s.permissions.lock().unwrap().snapshot().clone()).unwrap_or(json!({}))
}

#[tauri::command]
pub fn clear_permissions(app: AppHandle) -> Result<()> {
    let s = state(&app);
    s.permissions.lock().unwrap().clear_all();
    Ok(())
}

#[tauri::command]
pub fn clear_browsing_data(
    app: AppHandle,
    history: bool,
    cookies: bool,
    cache: bool,
    downloads_list: bool,
) -> Result<Value> {
    let s = state(&app);
    let mut report = json!({});
    if history {
        s.history.lock().unwrap().clear();
        report["history"] = json!(true);
    }
    if downloads_list {
        s.downloads.lock().unwrap().clear_finished();
        report["downloadsList"] = json!(true);
    }
    if cookies {
        // clear every webview's cookies via the engine-level cookie API
        let mut cleared = false;
        let tabs: Vec<String> = s.tabs.lock().unwrap().iter().map(|t| t.id.clone()).collect();
        for id in tabs {
            if let Some(wv) = app.get_webview(&id) {
                if let Ok(all) = wv.cookies() {
                    for c in all {
                        let _ = wv.delete_cookie(c);
                        cleared = true;
                    }
                }
            }
        }
        report["cookies"] = json!(cleared);
    }
    if cache {
        // favicon cache is our only on-disk cache
        let dir = s.data_dir.join("favicons");
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for f in rd.flatten() {
                std::fs::remove_file(f.path()).ok();
            }
        }
        report["cache"] = json!(true);
    }
    Ok(report)
}

#[tauri::command]
pub async fn force_quit(app: AppHandle) -> Result<()> {
    tabs::force_quit(&app);
    Ok(())
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<()> {
    open::that(&url).map_err(|e| e.to_string())
}

/// Toggle the main window fullscreen (F11).
#[tauri::command]
pub async fn set_fullscreen(app: AppHandle) -> Result<bool> {
    if let Some(w) = app.get_window("main") {
        let next = !w.is_fullscreen().map_err(|e| e.to_string())?;
        w.set_fullscreen(next).map_err(|e| e.to_string())?;
        return Ok(next);
    }
    Err("no window".into())
}

/// Single validated entry point for page-content events (keyboard
/// shortcuts, gestures, find results, save-page chunks). Reachable from
/// remote pages via the `page-event` Tauri event (capability grants ONLY
/// `core:event:allow-emit`) and from local pages via the command.
#[tauri::command]
pub fn page_event(app: AppHandle, event: Value) -> Result<()> {
    handle_page_event(&app, event);
    Ok(())
}

pub fn handle_page_event(app: &AppHandle, event: Value) {
    let t = event["t"].as_str().unwrap_or("");
    match t {
        "key" => {
            let _ = app.emit_to(CHROME_LABEL, "page-key", event);
        }
        "gesture" => {
            let name = event["name"].as_str().unwrap_or("");
            let s = app.state::<AppState>();
            let active = s.active_tab.lock().unwrap().clone();
            match name {
                "back" => {
                    if let Some(id) = active {
                        if let Some(wv) = app.get_webview(&id) {
                            let _ = crate::platform::go_back(&wv);
                        }
                    }
                }
                "forward" => {
                    if let Some(id) = active {
                        if let Some(wv) = app.get_webview(&id) {
                            let _ = crate::platform::go_forward(&wv);
                        }
                    }
                }
                "reload" => {
                    if let Some(id) = active {
                        if let Some(wv) = app.get_webview(&id) {
                            let _ = wv.reload();
                        }
                    }
                }
                "newtab" => {
                    let app2 = app.clone();
                    std::thread::spawn(move || {
                        let _ = tabs::new_tab(&app2, None, false, None);
                    });
                }
                "closetab" => {
                    let app2 = app.clone();
                    if let Some(id) = active {
                        std::thread::spawn(move || {
                            let _ = tabs::close_tab(&app2, &id);
                        });
                    }
                }
                _ => {}
            }
        }
        "find" => {
            let count = event["count"].as_i64().unwrap_or(0);
            let index = event["index"].as_i64().unwrap_or(0);
            let _ = app.emit_to(CHROME_LABEL, "find-result", json!({"count": count, "index": index}));
        }
        "save-start" | "save-chunk" | "save-end" | "save-error" => {
            handle_save_chunk(app, event);
        }
        _ => {}
    }
}

fn handle_save_chunk(app: &AppHandle, event: Value) {
    let s = state(app);
    let active = s.active_tab.lock().unwrap().clone().unwrap_or_default();
    let key = format!("savepage:{active}");
    let mut pending = s.pending_permissions.lock().unwrap();
    if let Some((path, _)) = pending.get(&key).cloned() {
        match event["t"].as_str() {
            Some("save-chunk") => {
                let data = event["data"].as_str().unwrap_or("");
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                    let _ = f.write_all(data.as_bytes());
                }
            }
            Some("save-end") => {
                pending.remove(&key);
                let _ = app.emit_to(
                    CHROME_LABEL,
                    "toast",
                    json!({"level": "info", "text": format!("Page saved to {path}")}),
                );
            }
            Some("save-error") => {
                pending.remove(&key);
                let _ = app.emit_to(
                    CHROME_LABEL,
                    "toast",
                    json!({"level": "error", "text": "Could not save the page"}),
                );
            }
            _ => {}
        }
    }
}

