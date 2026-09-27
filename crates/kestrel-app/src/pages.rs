//! kestrel:// internal pages: scheme handler serving embedded HTML/CSS/JS.
//!
//! Pages: newtab, settings, privacy, bookmarks, history, downloads, about.
//! State arrives via `bridge::push_state` after load; commands flow back
//! through the `kestrelHost` script-message handler (registered ONLY on
//! internal views — regular pages cannot invoke browser commands).

use crate::state::AppState;
use gtk::{gio, glib};
use std::rc::Rc;
use webkit::prelude::*;

const NEWTAB_HTML: &str = include_str!("../assets/pages/newtab.html");
const SETTINGS_HTML: &str = include_str!("../assets/pages/settings.html");
const PRIVACY_HTML: &str = include_str!("../assets/pages/privacy.html");
const BOOKMARKS_HTML: &str = include_str!("../assets/pages/bookmarks.html");
const HISTORY_HTML: &str = include_str!("../assets/pages/history.html");
const DOWNLOADS_HTML: &str = include_str!("../assets/pages/downloads.html");
const ABOUT_HTML: &str = include_str!("../assets/pages/about.html");
const APP_CSS: &str = include_str!("../assets/pages/app.css");
const APP_JS: &str = include_str!("../assets/pages/app.js");

pub fn register_scheme(state: &Rc<AppState>) {
    let weak = Rc::downgrade(state);
    state.webctx.register_uri_scheme("kestrel", move |request: &webkit::URISchemeRequest| {
        let Some(_state) = weak.upgrade() else { return };
        // Parse the FULL uri, not request.path(): URL parsing treats the
        // segment after `kestrel://` as a HOST (e.g. "assets" in
        // kestrel://assets/app.css), so path() would return "/app.css" and
        // every embedded asset would 404. Taking the raw uri keeps the
        // first segment in the matched key.
        let uri = request.uri().to_string();
        let rest = uri.strip_prefix("kestrel://").unwrap_or(&uri);
        let path = rest.split(['?', '#']).next().unwrap_or("");
        let path = path.trim_start_matches('/');
        let (page, _query) = match path.split_once('?') {
            Some((p, q)) => (p, q),
            None => (path, ""),
        };

        let (body, ctype) = match page {
            "newtab" | "" => (NEWTAB_HTML.to_string(), "text/html"),
            "settings" => (SETTINGS_HTML.to_string(), "text/html"),
            "privacy" => (PRIVACY_HTML.to_string(), "text/html"),
            "bookmarks" => (BOOKMARKS_HTML.to_string(), "text/html"),
            "history" => (HISTORY_HTML.to_string(), "text/html"),
            "downloads" => (DOWNLOADS_HTML.to_string(), "text/html"),
            "about" => (ABOUT_HTML.to_string(), "text/html"),
            "assets/app.css" => (APP_CSS.to_string(), "text/css"),
            "assets/app.js" => (APP_JS.to_string(), "text/javascript"),
            _ => ("<h1>404 — Unknown Kestrel page</h1>".to_string(), "text/html"),
        };

        let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from(body.as_bytes()));
        request.finish(&stream, body.len() as i64, Some(ctype));
    });
}
