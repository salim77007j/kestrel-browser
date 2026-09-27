//! Shared application state.

use kestrel_data::{
    BookmarksStore, DownloadsStore, HistoryStore, PermissionStore, SessionState, Settings,
    ShortcutsStore,
};
use kestrel_privacy::SafeBrowsing;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Emitter, Manager};

/// Everything the chrome UI needs to render the tab strip.
#[derive(Debug, Clone, Serialize)]
pub struct TabMeta {
    pub id: String,
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub muted: bool,
    pub loading: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub zoom: f64,
    pub group: Option<kestrel_data::GroupInfo>,
    pub favicon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct UiTabsEvent {
    pub tabs: Vec<TabMeta>,
    pub active: Option<String>,
}

pub struct Stats {
    pub day: String,
    pub blocked_today: u64,
    pub trackers_today: u64,
    pub requests_today: u64,
    pub blocked_all: u64,
    pub trackers_all: u64,
    pub per_domain: BTreeMap<String, u64>,
}

impl Stats {
    pub(crate) fn new_today() -> Self {
        let day = chrono_day();
        Self {
            day,
            blocked_today: 0,
            trackers_today: 0,
            requests_today: 0,
            blocked_all: 0,
            trackers_all: 0,
            per_domain: BTreeMap::new(),
        }
    }
}

pub fn chrono_day() -> String {
    // days since epoch -> YYYY-MM-DD without pulling chrono
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let mut year = 1970i64;
    let mut rem = days as i64;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if rem < len {
            break;
        }
        rem -= len;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1;
    for m in months {
        if rem < m {
            break;
        }
        rem -= m;
        month += 1;
    }
    format!("{year:04}-{month:02}-{rem:02}")
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub settings: RwLock<Settings>,
    pub engine: RwLock<Option<crate::engine_host::PrivacyClient>>,
    pub engine_rules: AtomicUsize,
    /// Hosts extracted from EasyPrivacy (for "trackers blocked" stats).
    pub tracker_hosts: RwLock<Arc<HashSet<String>>>,
    pub safebrowsing: RwLock<Arc<SafeBrowsing>>,
    /// Session-allowed dangerous hosts (interstitial "visit anyway").
    pub allowed_dangerous: RwLock<HashSet<String>>,
    pub history: Mutex<HistoryStore>,
    pub bookmarks: Mutex<BookmarksStore>,
    pub downloads: Mutex<DownloadsStore>,
    pub session: Mutex<SessionState>,
    pub shortcuts: Mutex<ShortcutsStore>,
    pub permissions: Mutex<PermissionStore>,
    pub stats: Mutex<Stats>,
    /// ordered tab metadata
    pub tabs: Mutex<Vec<TabMeta>>,
    pub active_tab: Mutex<Option<String>>,
    pub next_tab: AtomicU64,
    pub chrome_height: AtomicU32,
    pub fp_seed: u64,
    pub engine_ready: AtomicBool,
    /// pending permission prompts shown in the UI: id -> (origin, kind)
    pub pending_permissions: Mutex<HashMap<String, (String, String)>>,
    pub zoom_by_host: Mutex<HashMap<String, f64>>,
    /// active download pollers: id -> keep-going flag
    pub active_downloads: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

pub const CHROME_LABEL: &str = "chrome";

impl AppState {
    pub fn bootstrap(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from(".kestrel"));
        std::fs::create_dir_all(&dir)?;
        std::fs::create_dir_all(dir.join("favicons"))?;
        std::fs::create_dir_all(dir.join("filters"))?;

        let settings = Settings::load_from(&dir);

        // Restore session; decide crash state BEFORE we flip the flag.
        let session_store = kestrel_data::JsonStore::new(dir.join("session.json"));
        let mut session = SessionState::load(&session_store);
        let crashed = session.crashed_last_time();
        if !settings.restore_on_crash && crashed {
            session.tabs.clear();
        }
        session.clean_exit = false;
        session.save(&session_store);

        // Tabs to reopen this launch (crash restore or "continue where you
        // left off").
        let restore_tabs =
            if settings.startup == kestrel_data::StartupMode::Continue || crashed {
                session.tabs.clone()
            } else {
                Vec::new()
            };
        session.tabs = restore_tabs;

        // Build every store up-front (dir is moved into the struct below).
        let history = HistoryStore::new(kestrel_data::JsonStore::new(dir.join("history.json")));
        let bookmarks =
            BookmarksStore::new(kestrel_data::JsonStore::new(dir.join("bookmarks.json")));
        let downloads =
            DownloadsStore::new(kestrel_data::JsonStore::new(dir.join("downloads.json")));
        let shortcuts =
            ShortcutsStore::new(kestrel_data::JsonStore::new(dir.join("shortcuts.json")));
        let permissions =
            PermissionStore::new(kestrel_data::JsonStore::new(dir.join("permissions.json")));
        let stats = crate::stats::load_stats(&dir);
        let safebrowsing = load_safebrowsing(&dir);
        let zoom_by_host = load_zooms(&dir);

        Ok(Self {
            data_dir: dir,
            settings: RwLock::new(settings),
            engine: RwLock::new(None),
            tracker_hosts: RwLock::new(Arc::new(HashSet::new())),
            safebrowsing: RwLock::new(Arc::new(safebrowsing)),
            allowed_dangerous: RwLock::new(HashSet::new()),
            history: Mutex::new(history),
            bookmarks: Mutex::new(bookmarks),
            downloads: Mutex::new(downloads),
            session: Mutex::new(session),
            shortcuts: Mutex::new(shortcuts),
            permissions: Mutex::new(permissions),
            stats: Mutex::new(stats),
            tabs: Mutex::new(Vec::new()),
            active_tab: Mutex::new(None),
            engine_rules: AtomicUsize::new(0),
            next_tab: AtomicU64::new(1),
            chrome_height: AtomicU32::new(118),
            fp_seed: rand_seed(),
            engine_ready: AtomicBool::new(false),
            pending_permissions: Mutex::new(HashMap::new()),
            active_downloads: Mutex::new(HashMap::new()),
            zoom_by_host: Mutex::new(zoom_by_host),
        })
    }

    pub fn new_tab_id(&self) -> String {
        let n = self.next_tab.fetch_add(1, Ordering::SeqCst);
        format!("tab-{n}")
    }

    pub fn snapshot_tabs(&self) -> UiTabsEvent {
        let tabs = self.tabs.lock().unwrap();
        let active = self.active_tab.lock().unwrap().clone();
        UiTabsEvent {
            tabs: tabs.clone(),
            active,
        }
    }

    pub fn emit_tabs(&self, app: &AppHandle) {
        let _ = app.emit_to(CHROME_LABEL, "tabs", self.snapshot_tabs());
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn rand_seed() -> u64 {
    let mut b = [0u8; 8];
    let mut f = std::fs::File::open("/dev/urandom").ok();
    use std::io::Read;
    if f.as_mut().map(|f| f.read_exact(&mut b)).is_some_and(|r| r.is_ok()) {
        u64::from_le_bytes(b)
    } else {
        // Windows fallback: mix time + process id
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        t ^ (std::process::id() as u64).wrapping_mul(0x9E3779B97F4A7C15)
    }
}

pub fn load_safebrowsing(dir: &PathBuf) -> SafeBrowsing {
    let urlhaus = dir.join("filters").join("urlhaus.txt");
    match std::fs::read_to_string(urlhaus) {
        Ok(lines) if !lines.is_empty() => {
            SafeBrowsing::new(lines.lines().map(|s| s.to_string()).collect())
        }
        _ => SafeBrowsing::new(include_gz_lines(include_bytes!(
            "../assets/filters/urlhaus.txt.gz"
        ))),
    }
}

pub fn include_gz_lines(gz: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut dec = flate2::read::GzDecoder::new(std::io::Cursor::new(gz));
    use std::io::Read;
    let mut buf = Vec::new();
    if dec.read_to_end(&mut buf).is_ok() {
        out.extend(String::from_utf8_lossy(&buf).lines().map(|s| s.to_string()));
    }
    out
}

fn load_zooms(dir: &PathBuf) -> HashMap<String, f64> {
    std::fs::read_to_string(dir.join("zooms.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_zooms(state: &AppState) {
    let map = state.zoom_by_host.lock().unwrap().clone();
    std::fs::write(
        state.data_dir.join("zooms.json"),
        serde_json::to_string(&map).unwrap_or_default(),
    )
    .ok();
}
