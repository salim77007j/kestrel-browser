//! Network helpers: favicon fetching/caching, weather, download progress
//! polling, filename sanitization.

use crate::state::{now_ms, AppState, CHROME_LABEL};
use base64::Engine as _;
use serde_json::json;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(6))
        .timeout(std::time::Duration::from_secs(12))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Kestrel/0.2")
        .build()
}

pub fn sanitize_filename(p: &Path) -> String {
    let raw = p
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "download.bin".to_string());
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 32 => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').to_string();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("con") || trimmed.eq_ignore_ascii_case("nul") {
        "download.bin".to_string()
    } else {
        trimmed
    }
}

/// Make sure a favicon exists for the URL's host; fetch + cache if needed,
/// then push it to the chrome UI as a data URL.
pub fn ensure_favicon(app: AppHandle, url: &str) {
    let Ok(parsed) = url::Url::parse(url) else { return };
    let Some(host) = parsed.host_str().map(|s| s.to_string()) else { return };
    let cache = app.state::<AppState>().data_dir.join("favicons");
    let ext = "ico";
    let file = cache.join(format!("{host}.{ext}"));
    if file.exists() {
        push_favicon(app, &host, &file);
        return;
    }
    std::thread::spawn(move || {
        let scheme = if parsed.scheme() == "http" { "http" } else { "https" };
        let candidates = [
            format!("{scheme}://{host}/favicon.ico"),
            format!("https://{host}/favicon.ico"),
        ];
        for c in candidates {
            if let Ok(resp) = agent().get(&c).call() {
                let ct = resp.content_type().to_string();
                if !ct.starts_with("image/") {
                    continue;
                }
                let mut buf = Vec::new();
                if resp.into_reader().take(512 * 1024).read_to_end(&mut buf).is_ok() && !buf.is_empty()
                {
                    std::fs::write(&file, &buf).ok();
                    push_favicon(app, &host, &file);
                    return;
                }
            }
        }
    });
}

fn push_favicon(app: AppHandle, host: &str, file: &Path) {
    if let Ok(bytes) = std::fs::read(file) {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let _ = app.emit_to(
            CHROME_LABEL,
            "favicon",
            json!({"host": host, "data": format!("data:image/x-icon;base64,{b64}")}),
        );
    }
}

pub fn spawn_favicon_dir(app: AppHandle) {
    let dir = app.state::<AppState>().data_dir.join("favicons");
    std::fs::create_dir_all(dir).ok();
}

/// Poll a download's file size so the UI gets real progress even though
/// WebView2 does not expose byte-level progress events.
pub fn spawn_download_poller(app: AppHandle, id: u64, path: PathBuf) {
    let flag = Arc::new(AtomicBool::new(true));
    app.state::<AppState>()
        .active_downloads
        .lock()
        .unwrap()
        .insert(id, flag.clone());
    std::thread::spawn(move || {
        let mut last = 0u64;
        while flag.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if size != last {
                last = size;
                let state = app.state::<AppState>();
                state.downloads.lock().unwrap().progress(id, size, 0);
                let _ = app.emit_to(
                    CHROME_LABEL,
                    "downloads-changed",
                    state.downloads.lock().unwrap().list().to_vec(),
                );
            }
            if flag.load(Ordering::SeqCst) == false {
                break;
            }
        }
    });
}

/// Real weather from open-meteo (IP-geolocated), 30-minute cache.
pub fn get_weather(state: &AppState) -> serde_json::Value {
    let cache = state.data_dir.join("weather.json");
    if let Ok(meta) = std::fs::metadata(&cache) {
        if let Ok(modified) = meta.modified() {
            let age = std::time::SystemTime::now()
                .duration_since(modified)
                .map(|d| d.as_secs())
                .unwrap_or(u64::MAX);
            if age < 1800 {
                if let Ok(v) = std::fs::read_to_string(&cache) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v) {
                        return v;
                    }
                }
            }
        }
    }

    let mut out = json!({"available": false});
    let geo = agent().get("https://ipapi.co/json/").call();
    if let Ok(resp) = geo {
        if let Ok(v) = resp.into_json::<serde_json::Value>() {
            let lat = v["latitude"].as_f64();
            let lon = v["longitude"].as_f64();
            let city = v["city"].as_str().unwrap_or("").to_string();
            if let (Some(lat), Some(lon)) = (lat, lon) {
                let url = format!(
                    "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current_weather=true"
                );
                if let Ok(resp) = agent().get(&url).call() {
                    if let Ok(w) = resp.into_json::<serde_json::Value>() {
                        let temp = w["current_weather"]["temperature"].as_f64().unwrap_or(0.0);
                        let code = w["current_weather"]["weathercode"].as_i64().unwrap_or(0);
                        out = json!({
                            "available": true,
                            "tempC": temp.round() as i64,
                            "condition": describe_weather(code),
                            "city": city,
                        });
                    }
                }
            }
        }
    }
    std::fs::write(&cache, out.to_string()).ok();
    out
}

fn describe_weather(code: i64) -> &'static str {
    match code {
        0 => "Clear sky",
        1 | 2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Foggy",
        51..=67 | 80..=82 => "Rain showers",
        71..=77 | 85 | 86 => "Snow",
        95..=99 => "Thunderstorm",
        _ => "Cloudy",
    }
}
