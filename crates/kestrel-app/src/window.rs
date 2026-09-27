//! The browser window: header (tabs), toolbar, page stack, overlays, actions.

use crate::addressbar::AddressBar;
use crate::state::AppState;
use crate::tab::{Tab, PageKind};
use gtk::prelude::*;
use gtk::{self, glib, gio};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use webkit::prelude::*;
use webkit::{WebView, UserContentFilter};

pub struct BrowserWindow {
    pub state: Rc<AppState>,
    pub win: gtk::ApplicationWindow,
    pub handle: gtk::WindowHandle,
    pub stack: gtk::Stack,
    pub tabbar: gtk::Box,
    pub address: Rc<AddressBar>,
    pub back_btn: gtk::Button,
    pub fwd_btn: gtk::Button,
    pub reload_btn: gtk::Button,
    pub stop_btn: gtk::Button,
    pub downloads_btn: gtk::Button,
    pub downloads_badge: gtk::Label,
    pub progress: gtk::ProgressBar,
    pub status: gtk::Label,
    pub content_overlay: gtk::Overlay,
    pub toast: gtk::Revealer,
    toast_label: gtk::Label,
    pub tabs: RefCell<Vec<Rc<Tab>>>,
    pub active: Cell<usize>,
    pub closed: RefCell<Vec<(String, String)>>, // (uri, title)
    pub menu_btn: gtk::MenuButton,
    private_strip: gtk::Label,
}

impl BrowserWindow {
    pub fn new(app: &gtk::Application, state: &Rc<AppState>) -> Rc<Self> {
        let win = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Kestrel")
            .default_width(state.settings.borrow().window_width)
            .default_height(state.settings.borrow().window_height)
            .icon_name("io.kestrel.Browser")
            .build();

        // ---- header: [tabs][+]  [window buttons] ----
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        header.add_css_class("k-header");

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(false)
            .build();
        scroller.add_css_class("k-tabs-scroller");

        let tabs_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        tabs_box.add_css_class("k-tabs");
        scroller.set_child(Some(&tabs_box));

        let newtab_btn = gtk::Button::from_icon_name("tab-new-symbolic");
        newtab_btn.add_css_class("k-newtab");
        newtab_btn.set_tooltip_text(Some("New tab (Ctrl+T)"));
        newtab_btn.set_valign(gtk::Align::Start);

        let winbtns = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        winbtns.add_css_class("k-winbtns");
        let min_btn = gtk::Button::from_icon_name("window-minimize-symbolic");
        let max_btn = gtk::Button::from_icon_name("window-maximize-symbolic");
        let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
        for b in [&min_btn, &max_btn, &close_btn] {
            b.add_css_class("k-winbtn");
        }
        close_btn.add_css_class("close");
        winbtns.append(&min_btn);
        winbtns.append(&max_btn);
        winbtns.append(&close_btn);

        header.append(&scroller);
        header.append(&newtab_btn);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);
        header.append(&winbtns);

        let handle = gtk::WindowHandle::new();
        handle.set_child(Some(&header));

        // ---- toolbar ----
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        toolbar.add_css_class("k-toolbar");

        let back_btn = gtk::Button::from_icon_name("go-previous-symbolic");
        let fwd_btn = gtk::Button::from_icon_name("go-next-symbolic");
        let reload_btn = gtk::Button::from_icon_name("view-refresh-symbolic");
        let stop_btn = gtk::Button::from_icon_name("process-stop-symbolic");
        let home_btn = gtk::Button::from_icon_name("go-home-symbolic");
        for b in [&back_btn, &fwd_btn, &reload_btn, &stop_btn, &home_btn] {
            b.add_css_class("k-btn");
        }
        stop_btn.set_visible(false);
        home_btn.set_tooltip_text(Some("New Tab page"));

        let address = AddressBar::new(state);

        let downloads_btn = gtk::Button::new();
        downloads_btn.add_css_class("k-btn");
        let dl_row = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        let dl_icon = gtk::Image::from_icon_name("folder-download-symbolic");
        dl_icon.set_pixel_size(14);
        let downloads_badge = gtk::Label::new(None);
        dl_row.append(&dl_icon);
        dl_row.append(&downloads_badge);
        downloads_btn.set_child(Some(&dl_row));
        downloads_btn.set_tooltip_text(Some("Downloads (Ctrl+J)"));

        let menu_btn = gtk::MenuButton::new();
        menu_btn.add_css_class("k-btn");
        menu_btn.set_icon_name("open-menu-symbolic");
        menu_btn.set_tooltip_text(Some("Menu"));
        menu_btn.set_always_show_arrow(false);

        toolbar.append(&back_btn);
        toolbar.append(&fwd_btn);
        toolbar.append(&reload_btn);
        toolbar.append(&stop_btn);
        toolbar.append(&home_btn);
        toolbar.append(&address.container);
        toolbar.append(&downloads_btn);
        toolbar.append(&menu_btn);

        // ---- content area ----
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_vexpand(true);
        stack.set_hexpand(true);

        let progress = gtk::ProgressBar::new();
        progress.add_css_class("k-progress");
        progress.set_visible(false);
        progress.set_halign(gtk::Align::Fill);

        let status = gtk::Label::new(None);
        status.add_css_class("k-status");
        status.set_halign(gtk::Align::Start);
        status.set_valign(gtk::Align::End);
        status.set_visible(false);

        let toast_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let toast_label = gtk::Label::new(None);
        toast_label.add_css_class("k-toast");
        toast_box.set_halign(gtk::Align::Center);
        toast_box.set_valign(gtk::Align::End);
        toast_box.append(&toast_label);
        let toast = gtk::Revealer::new();
        toast.set_transition_type(gtk::RevealerTransitionType::SlideUp);
        toast.set_child(Some(&toast_box));
        toast.set_reveal_child(false);

        let content_overlay = gtk::Overlay::new();
        content_overlay.set_child(Some(&stack));
        content_overlay.add_overlay(&progress);
        content_overlay.add_overlay(&status);
        content_overlay.add_overlay(&toast);
        progress.set_halign(gtk::Align::Fill);
        progress.set_valign(gtk::Align::Start);

        let private_strip = gtk::Label::new(Some("Private tab — Kestrel won\u{2019}t save history, cookies or site data"));
        private_strip.add_css_class("k-private-strip");
        private_strip.set_visible(false);
        private_strip.set_halign(gtk::Align::Fill);
        private_strip.set_xalign(0.5);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&handle);
        root.append(&toolbar);
        root.append(&private_strip);
        root.append(&content_overlay);

        win.set_child(Some(&root));

        let w = Rc::new(Self {
            state: state.clone(),
            win,
            handle,
            stack,
            tabbar: tabs_box,
            address,
            back_btn,
            fwd_btn,
            reload_btn,
            stop_btn,
            downloads_btn,
            downloads_badge,
            progress,
            status,
            content_overlay,
            toast,
            toast_label,
            tabs: RefCell::new(Vec::new()),
            active: Cell::new(0),
            closed: RefCell::new(Vec::new()),
            menu_btn,
            private_strip,
        });

        crate::menus::install(&w, &w.menu_btn);
        crate::actions::install(&w);
        crate::window::wire_window_signals(&w, &newtab_btn, &min_btn, &max_btn, &close_btn, &back_btn, &fwd_btn, &reload_btn, &stop_btn, &home_btn, &downloads_btn);

        w
    }

    pub fn present(&self) {
        self.win.present();
    }

    // ---------------- tabs ----------------

    pub fn new_tab(self: &Rc<Self>, uri: &str, background: bool, private: bool) -> Rc<Tab> {
        let tab = crate::webview::create_tab(self.state.clone(), self.clone(), uri, private);
        let idx = self.tabs.borrow().len();
        self.tabs.borrow_mut().push(tab.clone());
        self.stack.add_named(&tab.content_box, Some(&tab.id.to_string()));
        self.tabbar.append(&tab.button);

        // Selecting a tab shows its page.
        {
            let w = self.clone();
            let tab_weak = Rc::downgrade(&tab);
            tab.button.connect_toggled(move |btn| {
                if btn.is_active() {
                    if let Some(t) = tab_weak.upgrade() {
                        if let Some(i) = w.index_of(&t) {
                            w.select_tab(i);
                        }
                    }
                }
            });
        }

        if background {
            tab.button.set_active(false);
        } else {
            tab.button.set_active(true); // fires select_tab
        }
        let _ = idx;
        self.update_tab_visibility();
        tab
    }

    pub fn open_url_in_new_tab(&self, uri: &str) {
        self.new_tab(&crate::session::normalize_uri(uri), false, false);
    }

    pub fn open_internal(&self, page: &'static str) {
        // Reuse an existing internal tab for that page if present.
        {
            let tabs = self.tabs.borrow();
            for (i, t) in tabs.iter().enumerate() {
                if t.internal == Some(page) {
                    drop(tabs);
                    self.select_tab(i);
                    return;
                }
            }
        }
        self.new_tab(&format!("kestrel://{page}"), false, false);
    }

    pub fn load_active(&self, uri: &str) {
        let active = self.current_tab();
        if let Some(tab) = active {
            tab.webview.load_uri(uri);
        } else {
            self.new_tab(uri, false, false);
        }
    }

    pub fn current_tab(&self) -> Option<Rc<Tab>> {
        let i = self.active.get();
        self.tabs.borrow().get(i).cloned()
    }

    pub fn index_of(&self, tab: &Rc<Tab>) -> Option<usize> {
        let id = tab.id;
        self.tabs.borrow().iter().position(|t| t.id == id)
    }

    pub fn select_tab(&self, i: usize) {
        let tabs = self.tabs.borrow();
        if i >= tabs.len() {
            return;
        }
        self.active.set(i);
        let tab = &tabs[i];
        self.stack.set_visible_child(&tab.content_box);
        tab.webview.grab_focus();
        drop(tabs);
        self.refresh_chrome();
        self.maybe_show_private_strip();
    }

    pub fn close_tab(&self, i: usize) {
        let tab = {
            let mut tabs = self.tabs.borrow_mut();
            if i >= tabs.len() {
                return;
            }
            tabs.remove(i)
        };
        // remember for reopen
        let uri = tab.uri();
        if !uri.is_empty() && !tab.private && tab.internal.is_none() {
            let title = tab.title.borrow().clone();
            self.closed.borrow_mut().push((uri, title));
            let len = self.closed.borrow().len();
            if len > 20 {
                self.closed.borrow_mut().remove(0);
            }
        }

        self.stack.remove(&tab.content_box);
        self.tabbar.remove(&tab.button);

        let was_active = self.active.get() == i;
        if was_active {
            let new_idx = i.min(self.tabs.borrow().len().saturating_sub(1));
            self.active.set(new_idx);
            self.select_tab(new_idx);
        }
        if self.tabs.borrow().is_empty() {
            self.win.close();
        }
    }

    pub fn close_tab_for_button(&self, button: &gtk::ToggleButton) {
        let idx = self
            .tabs
            .borrow()
            .iter()
            .position(|t| t.button == *button);
        if let Some(i) = idx {
            self.close_tab(i);
        }
    }

    pub fn reopen_closed_tab(&self) {
        let last = self.closed.borrow_mut().pop();
        if let Some((uri, _)) = last {
            self.new_tab(&uri, false, false);
        } else {
            self.toast_text("Nothing to reopen");
        }
    }

    pub fn next_tab(&self, delta: i32) {
        let n = self.tabs.borrow().len();
        if n == 0 {
            return;
        }
        let cur = self.active.get() as i32;
        let next = (cur + delta).rem_euclid(n as i32);
        self.select_tab(next as usize);
    }

    pub fn move_tab(&self, delta: i32) {
        let cur = self.active.get() as i32;
        let n = self.tabs.borrow().len() as i32;
        let next = (cur + delta).clamp(0, n - 1);
        if next != cur {
            let tab = self.tabs.borrow()[cur as usize].clone();
            self.tabs.borrow_mut().remove(cur as usize);
            self.tabs.borrow_mut().insert(next as usize, tab);
            self.active.set(next as usize);
            // Rebuild the strip order (Box::reorder is not exposed).
            let buttons: Vec<gtk::Widget> = self
                .tabs
                .borrow()
                .iter()
                .map(|t| t.button.clone().upcast::<gtk::Widget>())
                .collect();
            for b in &buttons {
                self.tabbar.remove(b);
            }
            let buttons: Vec<gtk::Widget> = self
                .tabs
                .borrow()
                .iter()
                .map(|t| t.button.clone().upcast::<gtk::Widget>())
                .collect();
            for b in &buttons {
                self.tabbar.append(b);
            }
            self.select_tab(next as usize);
        }
    }

    fn update_tab_visibility(&self) {
        // Trim tab widths when many tabs are open (simple heuristic).
        let n = self.tabs.borrow().len();
        for t in self.tabs.borrow().iter() {
            let maxw = if n > 8 { 12 } else if n > 5 { 18 } else { 26 };
            t.title_label.set_max_width_chars(maxw);
        }
    }

    fn maybe_show_private_strip(&self) {
        let private = self.current_tab().map(|t| t.private).unwrap_or(false);
        self.private_strip.set_visible(private);
        self.address.container.remove_css_class("private");
        if private {
            self.address.container.add_css_class("private");
        }
    }

    // ---------------- chrome refresh ----------------

    pub fn refresh_chrome(&self) {
        let Some(tab) = self.current_tab() else { return };
        let uri = tab.uri();
        self.address.set_uri_text(&uri);
        let lookalike = self.state.shield.borrow().is_lookalike(&uri);
        self.address.set_security(&uri, lookalike);
        self.back_btn.set_sensitive(tab.webview.can_go_back());
        self.fwd_btn.set_sensitive(tab.webview.can_go_forward());
        self.address.set_star(kestrel_data::bookmarks::has(&self.state.db.borrow(), &uri));
        if tab.shield_hits.get() > 0 {
            self.address.set_shield_count(tab.shield_hits.get());
        } else {
            self.address.shield_btn.set_visible(false);
        }
    }

    pub fn attach_network_filter(&self, filter: &UserContentFilter) {
        for tab in self.tabs.borrow().iter() {
            tab.ucm.add_filter(filter);
        }
    }

    pub fn refresh_internal_pages(&self) {
        for tab in self.tabs.borrow().iter() {
            if tab.is_internal_view {
                crate::bridge::push_state(&tab.webview, &self.state);
            }
        }
    }

    pub fn tab_states(&self) -> Vec<kestrel_data::session::TabState> {
        self.tabs
            .borrow()
            .iter()
            .map(|t| {
                crate::session::tab_state_of(&t.uri(), &t.title.borrow(), t.pinned.get(), t.muted.get(), t.private)
            })
            .collect()
    }

    pub fn active_index(&self) -> usize {
        self.active.get()
    }

    pub fn toast_text(&self, text: &str) {
        self.toast_label.set_text(text);
        self.toast.set_reveal_child(true);
        let toast = self.toast.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(1800), move || {
            toast.set_reveal_child(false);
            glib::ControlFlow::Break
        });
    }

    pub fn show_status(&self, text: &str) {
        if text.is_empty() {
            self.status.set_visible(false);
        } else {
            self.status.set_text(text);
            self.status.set_visible(true);
            let status = self.status.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(1600), move || {
                status.set_visible(false);
                glib::ControlFlow::Break
            });
        }
    }

    pub fn kind_of(&self, tab: &Tab) -> PageKind {
        match tab.internal {
            Some(p) => PageKind::Internal(p),
            None => PageKind::Web,
        }
    }
}

use std::ops::Deref;
impl Deref for BrowserWindow {
    type Target = gtk::ApplicationWindow;
    fn deref(&self) -> &Self::Target {
        &self.win
    }
}

crate::impl_rc_downgrade!(BrowserWindow);

/// Wire up window-level buttons and lifecycle.
pub fn wire_window_signals(
    w: &Rc<BrowserWindow>,
    newtab_btn: &gtk::Button,
    min_btn: &gtk::Button,
    max_btn: &gtk::Button,
    close_btn: &gtk::Button,
    back_btn: &gtk::Button,
    fwd_btn: &gtk::Button,
    reload_btn: &gtk::Button,
    stop_btn: &gtk::Button,
    home_btn: &gtk::Button,
    downloads_btn: &gtk::Button,
) {
    {
        let w2 = std::rc::Rc::downgrade(w);
        newtab_btn.connect_clicked(move |_| {
            if let Some(w2) = w2.upgrade() {
                w2.new_tab("kestrel://newtab", false, false);
            }
        });
    }
    {
        let w2 = std::rc::Rc::downgrade(w);
        min_btn.connect_clicked(move |_| {
            if let Some(w2) = w2.upgrade() {
                w2.win.minimize();
            }
        });
    }
    {
        let w2 = std::rc::Rc::downgrade(w);
        max_btn.connect_clicked(move |_| {
            if let Some(w2) = w2.upgrade() {
                if w2.win.is_maximized() {
                    w2.win.unmaximize();
                } else {
                    w2.win.maximize();
                }
            }
        });
    }
    {
        let w2 = std::rc::Rc::downgrade(w);
        close_btn.connect_clicked(move |_| {
            if let Some(w2) = w2.upgrade() {
                w2.win.close();
            }
        });
    }
    {
        let w2 = std::rc::Rc::downgrade(w);
        back_btn.connect_clicked(move |_| {
            if let Some(w2) = w2.upgrade() {
                if let Some(t) = w2.current_tab() {
                    t.webview.go_back();
                }
            }
        });
    }
    {
        let w2 = std::rc::Rc::downgrade(w);
        fwd_btn.connect_clicked(move |_| {
            if let Some(w2) = w2.upgrade() {
                if let Some(t) = w2.current_tab() {
                    t.webview.go_forward();
                }
            }
        });
    }
    {
        let w2 = std::rc::Rc::downgrade(w);
        reload_btn.connect_clicked(move |_| {
            if let Some(w2) = w2.upgrade() {
                if let Some(t) = w2.current_tab() {
                    t.webview.reload();
                }
            }
        });
    }
    {
        let w2 = std::rc::Rc::downgrade(w);
        stop_btn.connect_clicked(move |_| {
            if let Some(w2) = w2.upgrade() {
                if let Some(t) = w2.current_tab() {
                    t.webview.stop_loading();
                }
            }
        });
    }
    {
        let w2 = std::rc::Rc::downgrade(w);
        home_btn.connect_clicked(move |_| {
            if let Some(w2) = w2.upgrade() {
                w2.load_active("kestrel://newtab");
            }
        });
    }
    {
        let w2 = std::rc::Rc::downgrade(w);
        downloads_btn.connect_clicked(move |_| {
            if let Some(w2) = w2.upgrade() {
                w2.open_internal("downloads");
            }
        });
    }

    // Save session on close, drop the window from the registry, allow closing.
    {
        let w2: std::rc::Weak<BrowserWindow> = std::rc::Rc::downgrade(&w);
        let state2 = w.state.clone();
        w.win.connect_close_request(move |_| {
            if let Some(w2) = w2.upgrade() {
                crate::session::save_session(&w2.state);
                let this_ptr = std::rc::Rc::as_ptr(&w2);
                state2.windows.borrow_mut().retain(|x| !std::ptr::eq(std::rc::Rc::as_ptr(x), this_ptr));
            }
            false // allow close
        });
    }

    // Mouse gestures (right-drag navigation) on the page area.
    crate::gestures::install(w);
}
