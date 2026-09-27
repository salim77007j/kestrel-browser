//! Shared application state: settings, database, privacy engine, filters,
//! downloads, and the shared WebKit context.
//!
//! Threading model: GTK owns the UI thread. A dedicated *engine thread*
//! owns the adblock engine (it is !Send, so it never crosses threads);
//! filter-list IO happens on short-lived worker threads. All cross-thread
//! traffic uses `async_channel`, whose receiver is awaited on the main loop.

use crate::dl::DownloadCenter;
use kestrel_privacy::lists::{ListId, ListManager, ListStatus};
use kestrel_privacy::shield::Shield;
use kestrel_privacy::{CosmeticResult, FingerprintMode};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use webkit::prelude::*;
use webkit::{NetworkSession, UserContentFilter, WebContext};

use kestrel_data::Settings;

/// Messages from worker threads to the UI thread.
pub enum WorkerMsg {
    /// Engine thread finished (re)building; UI compiles the WebKit filter.
    FiltersLoaded {
        hosts: Vec<String>,
        cb_json: String,
        statuses: Vec<ListStatus>,
        rule_count: usize,
    },
    ListsFetched,
}

/// Messages into the persistent engine thread.
pub enum EngineMsg {
    Rebuild {
        rules: Vec<String>,
        statuses: Vec<ListStatus>,
    },
    Cosmetic {
        url: String,
        reply: async_channel::Sender<CosmeticResult>,
    },
    Explain {
        url: String,
        source: String,
        reply: async_channel::Sender<String>,
    },
}

pub struct AppState {
    pub settings: RefCell<Settings>,
    pub db: RefCell<rusqlite::Connection>,
    engine_tx: RefCell<Option<async_channel::Sender<EngineMsg>>>,
    pub rule_count: Cell<usize>,
    pub shield: RefCell<Shield>,
    pub network_filter: RefCell<Option<UserContentFilter>>,
    pub filters_active: Arc<AtomicBool>,
    pub webctx: WebContext,
    pub session: NetworkSession,
    pub ephemeral: RefCell<Option<NetworkSession>>,
    pub downloads: DownloadCenter,
    pub windows: RefCell<Vec<Rc<crate::window::BrowserWindow>>>,
    pub list_statuses: RefCell<Vec<ListStatus>>,
    pub filter_compile_ok: Cell<bool>
    ,
    tx: RefCell<Option<async_channel::Sender<WorkerMsg>>>,
}

impl AppState {
    pub fn new(settings: Settings) -> Rc<Self> {
        let data_dir = kestrel_data::dirs::base_data();
        let db = match kestrel_data::db::open(&data_dir.join("kestrel.db")) {
            Ok(c) => c,
            Err(_) => kestrel_data::db::open(&std::env::temp_dir().join("kestrel-fallback.db"))
                .expect("sqlite fallback"),
        };

        let webctx = WebContext::new();
        webctx.set_cache_model(webkit::CacheModel::WebBrowser);

        let session = NetworkSession::new(
            Some(data_dir.join("session").to_string_lossy().as_ref()),
            Some(data_dir.join("cache").to_string_lossy().as_ref()),
        );

        // Security defaults: fail closed on TLS errors, third-party cookie
        // control.
        session.set_tls_errors_policy(webkit::TLSErrorsPolicy::Fail);
        let cookie_policy = if settings.block_third_party_cookies {
            webkit::CookieAcceptPolicy::NoThirdParty
        } else {
            webkit::CookieAcceptPolicy::Always
        };
        if let Some(cm) = session.cookie_manager() {
            cm.set_accept_policy(cookie_policy);
        }

        let this = Rc::new(Self {
            settings: RefCell::new(settings),
            db: RefCell::new(db),
            engine_tx: RefCell::new(None),
            rule_count: Cell::new(0),
            shield: RefCell::new(Shield::new(&[])),
            network_filter: RefCell::new(None),
            filters_active: Arc::new(AtomicBool::new(false)),
            webctx,
            session,
            ephemeral: RefCell::new(None),
            downloads: DownloadCenter::new(),
            windows: RefCell::new(Vec::new()),
            list_statuses: RefCell::new(Vec::new()),
            filter_compile_ok: Cell::new(false),
            tx: RefCell::new(None),
        });

        crate::pages::register_scheme(&this);
        this.install_channels();
        this.reload_filters();

        this
    }

    fn install_channels(self: &Rc<Self>) {
        // ---- UI channel (worker -> main loop) ----
        let (tx, rx) = async_channel::unbounded::<WorkerMsg>();
        *self.tx.borrow_mut() = Some(tx);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        let Some(state) = weak.upgrade() else { return };
                        state.handle_worker_msg(msg);
                    }
                    Err(_) => return,
                }
            }
        });

        // ---- persistent engine thread (owns the !Send adblock engine) ----
        let (etx, erx) = async_channel::unbounded::<EngineMsg>();
        *self.engine_tx.borrow_mut() = Some(etx);
        let ui_tx = self.tx.borrow().as_ref().map(|t| t.clone()).unwrap();
        std::thread::Builder::new()
            .name("kestrel-engine".into())
            .spawn(move || {
                let mut engine: Option<kestrel_privacy::PrivacyEngine> = None;
                loop {
                    match erx.recv_blocking() {
                        Ok(EngineMsg::Rebuild { rules, statuses }) => {
                            let rule_count = rules.len();
                            engine = kestrel_privacy::PrivacyEngine::from_rules(&rules).ok();
                            let hosts = kestrel_privacy::lists::extract_tracker_hosts(&rules);
                            let cb_json =
                                kestrel_privacy::converter::to_content_blocker(&rules, 120_000)
                                    .unwrap_or_else(|_| "[]".into());
                            let _ = ui_tx.send(WorkerMsg::FiltersLoaded { hosts, cb_json, statuses, rule_count });
                        }
                        Ok(EngineMsg::Cosmetic { url, reply }) => {
                            let res = engine
                                .as_ref()
                                .map(|e| e.cosmetic(&url))
                                .unwrap_or_default();
                            let _ = reply.send_blocking(res);
                        }
                        Ok(EngineMsg::Explain { url, source, reply }) => {
                            let res = engine
                                .as_ref()
                                .map(|e| e.explain(&url, &source))
                                .unwrap_or_else(|| "engine still loading…".into());
                            let _ = reply.send_blocking(res);
                        }
                        Err(_) => return,
                    }
                }
            })
            .expect("engine thread");
    }

    fn handle_worker_msg(self: &Rc<Self>, msg: WorkerMsg) {
        match msg {
            WorkerMsg::FiltersLoaded { hosts, cb_json, statuses, rule_count } => {
                self.on_filters_loaded(hosts, cb_json, statuses, rule_count);
            }
            WorkerMsg::ListsFetched => self.reload_filters(),
        }
    }

    pub fn fingerprint_mode(&self) -> FingerprintMode {
        FingerprintMode::from_key(&self.settings.borrow().fingerprint_protection)
    }

    /// Ask the engine thread for cosmetic decisions; reply arrives on the UI loop.
    pub fn query_cosmetic(&self, url: &str) -> async_channel::Receiver<CosmeticResult> {
        let (rtx, rrx) = async_channel::bounded::<CosmeticResult>(1);
        if let Some(tx) = self.engine_tx.borrow().as_ref() {
            let _ = tx.send_blocking(EngineMsg::Cosmetic { url: url.to_string(), reply: rtx });
        } else {
            let _ = rtx.send_blocking(CosmeticResult::default());
        }
        rrx
    }

    pub fn query_explain(&self, url: &str, source: &str) -> async_channel::Receiver<String> {
        let (rtx, rrx) = async_channel::bounded::<String>(1);
        if let Some(tx) = self.engine_tx.borrow().as_ref() {
            let _ = tx.send_blocking(EngineMsg::Explain { url: url.to_string(), source: source.to_string(), reply: rtx });
        } else {
            let _ = rtx.send_blocking("engine still loading…".into());
        }
        rrx
    }

    /// Called once on app startup: session bookkeeping.
    pub fn on_startup(&self) {
        let _crashed = kestrel_data::session::previous_run_crashed();
        kestrel_data::session::clear_marker();
    }

    /// Called when the main loop is about to end.
    pub fn on_shutdown(&self) {
        kestrel_data::session::mark_clean();
        self.save_settings();
    }

    pub fn add_window(&self, w: &Rc<crate::window::BrowserWindow>) {
        self.windows.borrow_mut().push(w.clone());
    }

    pub fn main_window(&self) -> Option<Rc<crate::window::BrowserWindow>> {
        self.windows.borrow().first().cloned()
    }

    pub fn ephemeral_session(&self) -> NetworkSession {
        let mut slot = self.ephemeral.borrow_mut();
        if slot.is_none() {
            *slot = Some(NetworkSession::new_ephemeral());
        }
        slot.as_ref().unwrap().clone()
    }

    /// Rebuild engine + tracker set + compiled network filter (worker threads).
    pub fn reload_filters(&self) {
        let enabled: HashMap<String, bool> = self
            .settings
            .borrow()
            .lists
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        let custom = self.settings.borrow().custom_filters.clone();
        let lists_dir = kestrel_data::dirs::base_data().join("filters");
        let etx = self.engine_tx.borrow().as_ref().map(|t| t.clone());
        let Some(etx) = etx else { return };

        std::thread::spawn(move || {
            let manager = ListManager::new(&lists_dir);
            let (rules, statuses) = manager.load_all(&enabled, &custom);
            let _ = etx.send_blocking(EngineMsg::Rebuild { rules, statuses });
        });
    }

    fn on_filters_loaded(
        self: &Rc<Self>,
        hosts: Vec<String>,
        cb_json: String,
        statuses: Vec<ListStatus>,
        rule_count: usize,
    ) {
        self.rule_count.set(rule_count);
        self.shield.borrow_mut().update_hosts(&hosts);
        *self.list_statuses.borrow_mut() = statuses;

        let store_path = kestrel_data::dirs::base_data().join("filters/compiled");
        let _ = std::fs::create_dir_all(&store_path);
        let store = webkit::UserContentFilterStore::new(store_path.to_string_lossy().as_ref());
        let bytes = glib::Bytes::from(cb_json.as_bytes());
        let weak = Rc::downgrade(self);
        store.save(
            "kestrel-blocklists",
            &bytes,
            None::<&gtk::gio::Cancellable>,
            move |res| {
                let Some(state) = weak.upgrade() else { return };
                match res {
                    Ok(filter) => {
                        state.filters_active.store(true, Ordering::SeqCst);
                        state.filter_compile_ok.set(true);
                        for w in state.windows.borrow().iter() {
                            w.attach_network_filter(&filter);
                        }
                        *state.network_filter.borrow_mut() = Some(filter);
                    }
                    Err(e) => {
                        state.filter_compile_ok.set(false);
                        eprintln!("kestrel: filter compile failed: {e}");
                    }
                }
                for w in state.windows.borrow().iter() {
                    w.refresh_internal_pages();
                }
            },
        );
    }

    pub fn update_lists_now(&self) {
        let lists_dir = kestrel_data::dirs::base_data().join("filters");
        let tx = self.tx.borrow().as_ref().map(|t| t.clone());
        let Some(tx) = tx else { return };
        std::thread::spawn(move || {
            let manager = ListManager::new(&lists_dir);
            let _ = manager.fetch_all(&ListId::ALL);
            let _ = tx.send_blocking(WorkerMsg::ListsFetched);
        });
    }

    pub fn clear_browsing_data(&self, history: bool, cookies: bool, permissions: bool) {
        if history {
            kestrel_data::history::clear_all(&self.db.borrow());
        }
        if cookies {
            if let Some(cm) = self.session.cookie_manager() {
                let _ = glib::spawn_future_local(async move {
                    if let Ok(cookies) = cm.all_cookies_future().await {
                        for c in cookies {
                            let _ = cm.delete_cookie_future(&c).await;
                        }
                    }
                });
            }
        }
        if permissions {
            kestrel_data::permissions::clear(&self.db.borrow());
            self.settings.borrow_mut().permissions.clear();
            self.save_settings();
        }
    }

    pub fn save_settings(&self) {
        let _ = self.settings.borrow().save();
    }
}
