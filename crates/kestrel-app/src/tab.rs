//! Tabs: model + widgets (tab strip buttons and stacked content pages).

use crate::state::AppState;
use gtk::prelude::*;
use gtk::{self, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use webkit::prelude::*;
use webkit::WebView;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Web,
    Internal(&'static str), // kestrel:///<name>
}

pub struct Tab {
    pub id: u64,
    pub webview: WebView,
    pub ucm: webkit::UserContentManager,
    pub content_box: gtk::Box,
    pub findbar: std::rc::Rc<crate::findbar::FindBar>,
    pub button: gtk::ToggleButton,
    pub title_label: gtk::Label,
    pub fav_label: gtk::Label,
    pub spinner: gtk::Spinner,
    pub close_btn: gtk::Button,
    pub title: RefCell<String>,
    pub pinned: Cell<bool>,
    pub muted: Cell<bool>,
    pub private: bool,
    pub internal: Option<&'static str>,
    pub is_internal_view: bool,
    pub shield_hits: Cell<u64>,
    pub cosmetic_rules: Cell<u64>,
}

/// Small wrapper so Tab stays Clone-free; we use raw Cell<String>.
pub type RefCellLike = RefCell<String>;

static TAB_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Tab {
    pub fn new_id() -> u64 {
        TAB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    pub fn uri(&self) -> String {
        self.webview.uri().map(|u| u.to_string()).unwrap_or_default()
    }

    pub fn set_title_text(&self, t: &str) {
        let shown = if t.trim().is_empty() { "New Tab" } else { t };
        *self.title.borrow_mut() = shown.to_string();
        self.title_label.set_text(&ellipsize(shown, 28));
        self.title_label.set_tooltip_text(Some(shown));
    }
}

pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

pub fn build_tab_widget(state: &Rc<AppState>) -> (gtk::ToggleButton, gtk::Label, gtk::Label, gtk::Spinner, gtk::Button) {
    let btn = gtk::ToggleButton::new();
    btn.add_css_class("k-tab");
    btn.set_focusable(false);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let fav = gtk::Label::new(Some("K"));
    fav.add_css_class("k-tab-fav");
    fav.set_halign(gtk::Align::Center);
    fav.set_valign(gtk::Align::Center);

    let spinner = gtk::Spinner::new();
    spinner.set_visible(false);

    let title = gtk::Label::new(Some("New Tab"));
    title.add_css_class("k-tab-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_width_chars(8);
    title.set_max_width_chars(22);

    let close = gtk::Button::new();
    close.add_css_class("k-tab-close");
    let close_icon = gtk::Image::from_icon_name("window-close-symbolic");
    close_icon.set_pixel_size(12);
    close.set_child(Some(&close_icon));
    close.set_focusable(false);

    row.append(&fav);
    row.append(&spinner);
    row.append(&title);
    row.append(&close);
    btn.set_child(Some(&row));

    // Middle click closes; click selects (toggle handles selection state).
    let mid = gtk::GestureClick::new();
    mid.set_button(2);
    let btn_weak = btn.downgrade();
    mid.connect_released(glib::clone!(
        #[weak]
        state,
        move |_, _, _, _| {
            if let Some(b) = btn_weak.upgrade() {
                state.main_window().inspect(|w| w.close_tab_for_button(&b));
            }
        }
    ));
    btn.add_controller(mid);

    (btn, title, fav, spinner, close)
}
