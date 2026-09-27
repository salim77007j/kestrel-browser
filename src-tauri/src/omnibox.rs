//! Omnibox suggestions: local history/bookmarks matches + optional remote
//! search suggestions from the configured engine.

use crate::state::AppState;
use kestrel_data::SearchEngineId;
use serde_json::json;
use tauri::{AppHandle, Manager};
use url::Url;

#[derive(serde::Serialize, Clone)]
pub struct Suggestion {
    pub kind: String, // "history" | "bookmark" | "search" | "url"
    pub text: String,
    pub secondary: String,
}

pub fn suggest(app: &AppHandle, query: &str) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();
    let q = query.trim();
    if q.is_empty() {
        return out;
    }
    let state = app.state::<AppState>();
    let ql = q.to_lowercase();

    // bookmarks first (exact-ish)
    {
        let bms = state.bookmarks.lock().unwrap();
        for b in bms.bar().iter().chain(bms.other().iter()) {
            let hit = b.title.to_lowercase().contains(&ql) || b.url.to_lowercase().contains(&ql);
            if hit {
                out.push(Suggestion {
                    kind: "bookmark".into(),
                    text: b.url.clone(),
                    secondary: b.title.clone(),
                });
            }
            if out.len() >= 3 {
                break;
            }
        }
    }

    // history
    {
        let hist = state.history.lock().unwrap();
        for e in hist.search(q, 40) {
            if out.len() >= 7 {
                break;
            }
            if out.iter().any(|s| s.text == e.url) {
                continue;
            }
            out.push(Suggestion {
                kind: "history".into(),
                text: e.url.clone(),
                secondary: if e.title.is_empty() { String::from("History") } else { e.title.clone() },
            });
        }
    }

    // remote search suggestions (optional, off-UI-thread safe: we're on a
    // worker thread because the command is async)
    let (engine_id, remote_ok) = {
        let s = state.settings.read().unwrap();
        (s.search_engine, s.search_suggestions)
    };
    if remote_ok && out.len() < 10 {
        if let Some(url) = engine_id.suggest_url(q) {
            if let Ok(list) = fetch_suggestions(&engine_id, &url) {
                for s in list.into_iter().take(6) {
                    if out.iter().any(|x| x.text == s) {
                        continue;
                    }
                    out.push(Suggestion {
                        kind: "search".into(),
                        text: s,
                        secondary: String::new(),
                    });
                }
            }
        }
    }

    // URL-ish input: offer direct navigation
    if looks_like_url(q) && !out.iter().any(|s| s.text == q) {
        out.insert(
            0,
            Suggestion {
                kind: "url".into(),
                text: q.to_string(),
                secondary: "Open URL".into(),
            },
        );
    }
    out
}

fn looks_like_url(q: &str) -> bool {
    if q.contains(char::is_whitespace) {
        return false;
    }
    q.starts_with("http://")
        || q.starts_with("https://")
        || Url::parse(&format!("https://{q}"))
            .map(|u| u.host_str().map_or(false, |h| h.contains('.')))
            .unwrap_or(false)
}

fn fetch_suggestions(engine: &SearchEngineId, url: &str) -> Result<Vec<String>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(4))
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Kestrel/0.2")
        .build();
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    Ok(match engine {
        // [query, [suggestions]] format: DDG list, Google firefox, Bing osjson
        SearchEngineId::DuckDuckGo
        | SearchEngineId::Google
        | SearchEngineId::Bing => v
            .as_array()
            .and_then(|a| a.get(1))
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(|x| x.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        // [{phrase: "..."}, ...] or {results:[...]}
        SearchEngineId::Brave => {
            let arr = if v.is_array() {
                v.as_array().unwrap().clone()
            } else {
                v["results"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            };
            arr.iter()
                .filter_map(|s| {
                    s.as_str()
                        .map(|x| x.to_string())
                        .or_else(|| s["phrase"].as_str().map(|x| x.to_string()))
                        .or_else(|| s["title"].as_str().map(|x| x.to_string()))
                })
                .collect()
        }
    })
}

pub fn search_url_for(state: &AppState, query: &str) -> String {
    let engine = state.settings.read().unwrap().search_engine;
    engine.search_url(query)
}

pub fn dummy_json() -> serde_json::Value {
    json!({})
}
