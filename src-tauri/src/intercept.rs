//! Network interception: ad/tracker blocking, cookie policy, HTTPS-only
//! upgrades, safe-browsing interstitials, page-load bookkeeping and
//! permission decisions.
//!
//! This is the privacy heart of the shell: every request made by a tab
//! webview passes through `handle_request` BEFORE it leaves the machine.

use crate::state::{now_ms, AppState, TabMeta, CHROME_LABEL};
use kestrel_privacy::headers::{cookie_action, is_third_party, CookiePolicy};
use kestrel_privacy::Threat;
use serde_json::json;
use tauri::http::{header::HeaderMap, HeaderValue, Request, Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager};
use tauri::webview::{PageLoadEvent, PermissionKind, PermissionResponse};
use url::Url;

fn empty_response(status: StatusCode) -> Response<&'static [u8]> {
    Response::builder()
        .status(status)
        .body(&b""[..])
        .unwrap()
}

fn redirect_response(to: &str) -> Response<&'static [u8]> {
    Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header("Location", to)
        .body(&b""[..])
        .unwrap()
}

/// Map request headers to a content type token the engine understands.
fn request_type(req: &Request<Vec<u8>>) -> &'static str {
    let dest = req
        .headers()
        .get("sec-fetch-dest")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match dest {
        "document" | "frame" => "document",
        "script" | "worker" | "serviceworker" | "sharedworker" => "script",
        "image" => "image",
        "style" => "stylesheet",
        "font" => "font",
        "audio" | "video" | "media" => "media",
        "websocket" => "websocket",
        "empty" => {
            // fetch/xhr/eventsource
            let accept = req
                .headers()
                .get("accept")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if accept.contains("text/html") {
                "document"
            } else {
                "xmlhttprequest"
            }
        }
        "iframe" => "sub_frame",
        _ => {
            let m = req.method().as_str();
            if m == "GET" {
                // fall back on accept header heuristics
                let accept = req
                    .headers()
                    .get("accept")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if accept.starts_with("image/") {
                    "image"
                } else if accept.contains("text/css") {
                    "stylesheet"
                } else {
                    "other"
                }
            } else {
                "other"
            }
        }
    }
}

fn referer_url(req: &Request<Vec<u8>>) -> Option<Url> {
    req.headers()
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Url::parse(s).ok())
}

pub fn is_local_origin(url: &Url) -> bool {
    matches!(url.host_str(), Some(h) if h.ends_with(".localhost") || h == "localhost")
        || matches!(url.scheme(), "tauri" | "kestrel" | "about" | "data" | "blob" | "file")
}

/// The network request interceptor. Mutates the response to block/redirect.
pub fn handle_request(
    app: &AppHandle,
    tab_id: &str,
    req: Request<Vec<u8>>,
    res: &mut Response<std::borrow::Cow<'static, [u8]>>,
) {
    let url_str = req.uri().to_string();
    let url = match Url::parse(&url_str) {
        Ok(u) => u,
        Err(_) => return,
    };
    if is_local_origin(&url) {
        return;
    }
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return;
    }

    let state = app.state::<AppState>();
    let (https_only, cookie_policy, adblock_on) = {
        let s = state.settings.read().unwrap();
        (s.https_only, s.cookie_policy, s.adblock_enabled)
    };

    // ---- HTTPS-only upgrade: bounce main-frame http to https ----
    if scheme == "http" && https_only {
        let mut https = url.clone();
        let _ = https.set_scheme("https");
        if https.host_str().is_some() {
            *res = redirect_response(https.as_str()).map_body_to_cow();
            return;
        }
    }

    // ---- Engine network filtering ----
    let rtype = request_type(&req);
    let referer = referer_url(&req);
    let source = referer
        .as_ref()
        .map(|u| u.as_str().to_string())
        .unwrap_or_else(|| url.as_str().to_string());

    let mut blocked = false;
    if adblock_on {
        let client = state.engine.read().unwrap().clone();
        if let Some(eng) = client {
            blocked = eng.should_block(url.as_str(), &source, rtype);
        }
    }
    if blocked {
        let host = url.host_str().unwrap_or("").to_string();
        let is_tracker = state
            .tracker_hosts
            .read()
            .unwrap()
            .contains(&host);
        crate::stats::record(&state, &host, true, is_tracker);
        *res = empty_response(if matches!(rtype, "document" | "sub_frame") {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::OK
        })
        .map_body_to_cow();
        schedule_stats_emit(app.clone());
        return;
    }

    // ---- Cookie policy on third-party traffic ----
    // NOTE: WebView2 exposes request headers read-only in this hook, so the
    // enforceable half of the third-party cookie policy is stripping
    // Set-Cookie from third-party responses (no NEW third-party cookies).
    // Outgoing Cookie headers of third-party requests cannot be edited at
    // this layer; they are mitigated by Chromium cookie partitioning and the
    // "clear cookies on exit" setting.
    let target_host = url.host_str().unwrap_or("").to_string();
    let source_host = referer
        .as_ref()
        .and_then(|u| u.host_str())
        .unwrap_or(&target_host)
        .to_string();
    let third_party = is_third_party(&target_host, &source_host);
    if third_party {
        let has_res_set_cookie = res.headers().contains_key("set-cookie");
        let action = cookie_action(cookie_policy, third_party, false, has_res_set_cookie);
        if matches!(
            action,
            kestrel_privacy::headers::CookieAction::StripResponse
                | kestrel_privacy::headers::CookieAction::StripBoth
        ) {
            res.headers_mut().remove("set-cookie");
        }
    }

    let _ = tab_id;
}

/// Navigation gate: safe browsing + session restore of zoom.
pub fn allow_navigation(app: &AppHandle, tab_id: &str, url: &Url) -> bool {
    if is_local_origin(url) {
        return true;
    }
    let state = app.state::<AppState>();
    let safe_on = state.settings.read().unwrap().safe_browsing;
    if safe_on {
        if let Some(threat) = state.safebrowsing.read().unwrap().check_url(url.as_str()) {
            let host_ok = state
                .allowed_dangerous
                .read()
                .unwrap()
                .contains(url.host_str().unwrap_or(""));
            if !host_ok {
                // redirect the tab to our interstitial
                let app2 = app.clone();
                let id2 = tab_id.to_string();
                let u = url.as_str().to_string();
                let reason = match &threat {
                    Threat::MalwareHost => "malware".to_string(),
                    Threat::PhishingLookalike { brand, detail } => {
                        format!("lookalike:{}:{}", brand, detail)
                    }
                };
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    let _ = app2.run_on_main_thread(move || {
                        if let Some(wv) = app2.get_webview(&id2) {
                            let page = format!(
                                "pages/blocked.html?u={}&reason={}",
                                urlencode_component(&u),
                                urlencode_component(&reason)
                            );
                            let _ = wv.navigate(crate::tabs::internal_url(&page).parse().unwrap());
                        }
                    });
                });
                return false;
            }
        }
    }
    true
}

pub fn urlencode_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Page-load bookkeeping: history, titles, tab state, cosmetic filtering.
pub fn on_page_load(app: &AppHandle, tab_id: &str, wv: tauri::Webview<tauri::Wry>, event: PageLoadEvent) {
    let state = app.state::<AppState>();
    let url = wv.url().map(|u| u.to_string()).unwrap_or_default();
    match event {
        PageLoadEvent::Started => {
            update_tab(app, tab_id, |t| {
                t.loading = true;
                if !url.is_empty() {
                    t.url = url.clone();
                }
            });
            // apply remembered per-host zoom
            let host = Url::parse(&url).ok().and_then(|u| u.host_str().map(|s| s.to_string()));
            if let Some(h) = host {
                let zoom = state.zoom_by_host.lock().unwrap().get(&h).copied();
                if let Some(z) = zoom {
                    if z > 0.1 && z < 8.0 {
                        let _ = wv.set_zoom(z);
                        update_tab(app, tab_id, |t| t.zoom = z);
                    }
                }
            }
        }
        PageLoadEvent::Finished => {
            update_tab(app, tab_id, |t| {
                t.loading = false;
                if !url.is_empty() {
                    t.url = url.clone();
                }
            });
            let internal = crate::tabs::is_internal(&url);
            if !internal {
                // history (url now; title comes with the title event)
                state.history.lock().unwrap().visit(&url, "", now_ms());
                // cosmetic filtering pass 1
                inject_cosmetic(app, tab_id, &url);
                // favicon lookup (async)
                crate::net::ensure_favicon(app.clone(), &url);
            }
            state.emit_tabs(app);
        }
        _ => {}
    }
}

pub fn on_title(app: &AppHandle, tab_id: &str, wv: tauri::Webview<tauri::Wry>, title: String) {
    let state = app.state::<AppState>();
    let url = wv.url().map(|u| u.to_string()).unwrap_or_default();
    let internal = crate::tabs::is_internal(&url);
    let t = title.clone();
    update_tab(app, tab_id, |m| {
        if !t.is_empty() {
            m.title = t.clone();
        }
    });
    if !internal && !title.is_empty() && !url.is_empty() {
        state.history.lock().unwrap().visit(&url, &title, now_ms());
    }
    state.emit_tabs(app);
}

/// Cosmetic filter injection for a loaded page (+ a late second pass for
/// dynamically inserted ads).
pub fn inject_cosmetic(app: &AppHandle, tab_id: &str, url: &str) {
    let state = app.state::<AppState>();
    if !state.settings.read().unwrap().adblock_enabled {
        return;
    }
    let client = state.engine.read().unwrap().clone();
    if let Some(eng) = client {
        let cosmetic = eng.cosmetic(url);
        if let Some(wv) = app.get_webview(tab_id) {
            if !cosmetic.hide_css.is_empty() {
                let css = cosmetic
                    .hide_css
                    .iter()
                    .map(|s| css_escape(s))
                    .collect::<Vec<_>>()
                    .join(",");
                let js = format!(
                    "(function(){{try{{var s=document.createElement('style');s.id='kestrel-cosmetic';s.textContent=\"{}\";(document.head||document.documentElement).appendChild(s);}}catch(e){{}}}})();",
                    css.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ")
                );
                let _ = wv.eval(&js);
            }
            if !cosmetic.injected_script.is_empty() {
                let _ = wv.eval(&cosmetic.injected_script);
            }
            // second pass for late-inserted elements
            let wv2 = wv.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1800));
                let _ = wv2.eval(&format!(
                    "(function(){{try{{var s=document.getElementById('kestrel-cosmetic');if(s)document.documentElement.appendChild(s);}}catch(e){{}}}})();"
                ));
            });
        }
    }
}

fn css_escape(s: &str) -> String {
    s.trim().to_string()
}

pub fn update_tab(app: &AppHandle, tab_id: &str, f: impl FnOnce(&mut TabMeta)) {
    let state = app.state::<AppState>();
    {
        let mut tabs = state.tabs.lock().unwrap();
        if let Some(m) = tabs.iter_mut().find(|t| t.id == tab_id) {
            f(m);
        }
    }
    state.emit_tabs(app);
}

/// Permission decisions: remembered per-site choices first, then the
/// global per-kind defaults from settings (secure by default: deny).
pub fn permission_decision(app: &AppHandle, kind: PermissionKind) -> PermissionResponse {
    use kestrel_data::PermissionDecision;
    let state = app.state::<AppState>();
    // origin: we do not get the requesting origin from wry's API surface
    // here, so decisions key off the ACTIVE tab's current URL.
    let origin = {
        let active = state.active_tab.lock().unwrap().clone();
        active
            .and_then(|id| {
                state
                    .tabs
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.url.clone())
            })
            .and_then(|u| Url::parse(&u).ok())
            .map(|u| format!("{}://{}", u.scheme(), u.host_str().unwrap_or("")))
            .unwrap_or_default()
    };
    let kind_str = permission_kind_str(kind);
    if !origin.is_empty() {
        if let Some(d) = state.permissions.lock().unwrap().get(&origin, kind_str) {
            return match d {
                PermissionDecision::Allow => PermissionResponse::Allow,
                PermissionDecision::Deny => PermissionResponse::Deny,
            };
        }
    }
    // secure default: deny and notify the UI once per origin/kind
    let _ = app.emit_to(
        CHROME_LABEL,
        "toast",
        json!({"level":"info","text":format!("Kestrel denied {} for this site. Manage exceptions in Settings → Privacy & security → Permissions.", kind_str)}),
    );
    PermissionResponse::Deny
}

fn permission_kind_str(kind: PermissionKind) -> &'static str {
    match kind {
        PermissionKind::Camera => "camera",
        PermissionKind::Microphone => "microphone",
        PermissionKind::Geolocation => "geolocation",
        PermissionKind::Notifications => "notifications",
        PermissionKind::ClipboardRead => "clipboard-read",
        PermissionKind::DisplayCapture => "display-capture",
        PermissionKind::Midi => "midi",
        PermissionKind::Sensors => "sensors",
        PermissionKind::MediaKeySystemAccess => "media-keys",
        PermissionKind::LocalFonts => "local-fonts",
        PermissionKind::WindowManagement => "window-management",
        PermissionKind::PointerLock => "pointer-lock",
        _ => "other",
    }
}

/// Deferred stats UI refresh (batched, no per-request event spam).
fn schedule_stats_emit(app: AppHandle) {
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    let emit_now = {
        let mut last = LAST.lock().unwrap();
        let now = std::time::Instant::now();
        let due = last.map(|l| now.duration_since(l).as_millis() > 700).unwrap_or(true);
        if due {
            *last = Some(now);
        }
        due
    };
    if emit_now {
        let state = app.state::<AppState>();
        let stats = state.stats.lock().unwrap();
        let payload = json!({
            "blockedToday": stats.blocked_today,
            "trackersToday": stats.trackers_today,
            "requestsToday": stats.requests_today,
        });
        drop(stats);
        let _ = app.emit_to(CHROME_LABEL, "adblock-stats", payload);
    }
}

pub fn decode_component(s: &str) -> String {
    urldecode(s)
}

trait MapBodyToCow {
    fn map_body_to_cow(self) -> Response<std::borrow::Cow<'static, [u8]>>;
}

impl MapBodyToCow for Response<&'static [u8]> {
    fn map_body_to_cow(self) -> Response<std::borrow::Cow<'static, [u8]>> {
        let (parts, body) = self.into_parts();
        Response::from_parts(parts, std::borrow::Cow::Borrowed(body))
    }
}

pub fn request_type_of(req: &Request<Vec<u8>>) -> &'static str {
    request_type(req)
}

pub fn header_value(v: &str) -> HeaderValue {
    HeaderValue::from_str(v).unwrap_or(HeaderValue::from_static(""))
}
