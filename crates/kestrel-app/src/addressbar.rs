//! Address bar: security indicator, shield badge, star, suggestions popover.

use crate::state::AppState;
use gtk::glib;
use gtk::prelude::*;
use gtk::{Entry, EventControllerKey, ListBox, ListBoxRow, Popover};
use std::rc::Rc;

pub struct AddressBar {
    pub container: gtk::Box,
    pub entry: Entry,
    pub sec_icon: gtk::Image,
    pub shield_btn: gtk::Button,
    pub shield_label: gtk::Label,
    pub star: gtk::Button,
    pub popover: Popover,
    list: ListBox,
    state: Rc<AppState>,
    navigating: std::cell::Cell<bool>,
}

#[derive(Clone)]
struct Suggestion {
    kind: &'static str, // URL | SEARCH | HISTORY | BOOKMARK
    title: String,
    sub: String,
    uri: String,
}

impl AddressBar {
    pub fn new(state: &Rc<AppState>) -> Rc<Self> {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        container.add_css_class("k-address");
        container.set_hexpand(true);
        container.set_valign(gtk::Align::Center);

        let sec_icon = gtk::Image::from_icon_name("changes-prevent-symbolic");
        sec_icon.add_css_class("k-secicon");
        sec_icon.set_pixel_size(14);

        let entry = Entry::new();
        entry.add_css_class("k-urlentry");
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some("Search or enter address"));
        entry.set_focus_on_click(true);

        let shield_btn = gtk::Button::new();
        shield_btn.add_css_class("k-shield");
        shield_btn.set_focusable(false);
        shield_btn.set_tooltip_text(Some("Privacy shield"));
        let shield_row = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        let shield_icon = gtk::Image::from_icon_name("security-high-symbolic");
        shield_icon.set_pixel_size(13);
        let shield_label = gtk::Label::new(Some("0"));
        shield_label.add_css_class("count");
        shield_row.append(&shield_icon);
        shield_row.append(&shield_label);
        shield_btn.set_child(Some(&shield_row));
        shield_btn.set_visible(false);

        let star = gtk::Button::new();
        star.add_css_class("k-btn");
        star.add_css_class("k-star");
        star.set_focusable(false);
        let star_icon = gtk::Image::from_icon_name("star-symbolic");
        star_icon.set_pixel_size(14);
        star.set_child(Some(&star_icon));
        star.set_tooltip_text(Some("Bookmark this page (Ctrl+D)"));

        container.append(&sec_icon);
        container.append(&entry);
        container.append(&shield_btn);
        container.append(&star);

        let popover = Popover::new();
        popover.add_css_class("k-suggest");
        popover.set_parent(&entry);
        popover.set_position(gtk::PositionType::Bottom);
        let list = ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        popover.set_child(Some(&list));

        let this = Rc::new(Self {
            container,
            entry,
            sec_icon,
            shield_btn,
            shield_label,
            star,
            popover,
            list,
            state: state.clone(),
            navigating: std::cell::Cell::new(false),
        });

        this.wire();
        this
    }

    fn wire(self: &Rc<Self>) {
        // Text changed -> refresh suggestions.
        {
            let this2 = Rc::downgrade(self);
            self.entry.connect_changed(move |e| {
                let Some(this2) = this2.upgrade() else { return };
                if this2.navigating.get() {
                    return;
                }
                let text = e.text().to_string();
                if text.is_empty() {
                    this2.popover.popdown();
                } else {
                    this2.refresh_suggestions(&text);
                }
            });
        }

        // Enter -> navigate or accept selected suggestion.
        {
            let this3 = Rc::downgrade(self);
            self.entry.connect_activate(move |e| {
                let Some(this3) = this3.upgrade() else { return };
                let selected = this3.selected_uri();
                let text = e.text().to_string();
                match selected {
                    Some(uri) => this3.navigate(&uri),
                    None => this3.navigate(&text),
                }
            });
        }

        // Keyboard navigation inside the popover.
        {
            let this4 = Rc::downgrade(self);
            let key = EventControllerKey::new();
            key.connect_key_pressed(move |_, key, _, _| {
                let Some(this4) = this4.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                match key {
                    gtk::gdk::Key::Down => {
                        this4.move_selection(1);
                        glib::Propagation::Stop
                    }
                    gtk::gdk::Key::Up => {
                        this4.move_selection(-1);
                        glib::Propagation::Stop
                    }
                    gtk::gdk::Key::Escape => {
                        this4.popover.popdown();
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            });
            self.entry.add_controller(key);
        }

        // Row click -> navigate.
        {
            let this5 = Rc::downgrade(self);
            self.list.connect_row_activated(move |_, row| {
                let Some(this5) = this5.upgrade() else { return };
                let name = row.widget_name();
                let uri = name.to_string();
                if !uri.is_empty() {
                    this5.navigate(&uri);
                }
            });
        }

        // Shield button -> open privacy dashboard.
        {
            let this6 = Rc::downgrade(self);
            self.shield_btn.connect_clicked(move |_| {
                let Some(this6) = this6.upgrade() else { return };
                if let Some(w) = this6.state.main_window() {
                    w.open_internal("privacy");
                }
            });
        }

        // Star -> toggle bookmark.
        {
            let this7 = Rc::downgrade(self);
            self.star.connect_clicked(move |_| {
                let Some(this7) = this7.upgrade() else { return };
                if let Some(w) = this7.state.main_window() {
                    w.win.activate_action("bookmark-toggle", None::<&glib::Variant>);
                }
            });
        }
    }

    fn selected_uri(&self) -> Option<String> {
        let row = self.list.selected_row()?;
        Some(row.widget_name().to_string())
    }

    fn move_selection(&self, delta: i32) {
        let n = self.list.observe_children().n_items();
        if n == 0 {
            return;
        }
        let cur = self.list.selected_row().map(|r| r.index()).unwrap_or(-1);
        let next = (cur + delta).clamp(0, n as i32 - 1);
        self.list.select_row(self.list.row_at_index(next).as_ref());
    }

    fn refresh_suggestions(&self, text: &str) {
        let mut child = self.list.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            self.list.remove(&c);
            child = next;
        }

        let suggestions = self.collect_suggestions(text);
        for (i, s) in suggestions.iter().enumerate() {
            let row = ListBoxRow::new();
            row.set_widget_name(&s.uri);
            let h = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let kind = gtk::Label::new(Some(s.kind));
            kind.add_css_class("k-sug-kind");
            kind.set_width_chars(8);
            kind.set_xalign(0.0);
            let v = gtk::Box::new(gtk::Orientation::Vertical, 1);
            let title = gtk::Label::new(Some(&s.title));
            title.add_css_class("k-sug-title");
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let sub = gtk::Label::new(Some(&s.sub));
            sub.add_css_class("k-sug-sub");
            sub.set_xalign(0.0);
            sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
            v.append(&title);
            v.append(&sub);
            h.append(&kind);
            h.append(&v);
            row.set_child(Some(&h));
            self.list.append(&row);
            if i == 0 {
                self.list.select_row(Some(&row));
            }
        }

        if suggestions.is_empty() {
            self.popover.popdown();
        } else {
            self.popover.popup();
        }
    }

    fn collect_suggestions(&self, text: &str) -> Vec<Suggestion> {
        let mut out: Vec<Suggestion> = Vec::new();
        let t = text.trim();
        if t.is_empty() {
            return out;
        }

        // 1) URL interpretation
        let looks_like_url = t.starts_with("http://")
            || t.starts_with("https://")
            || t.starts_with("kestrel://")
            || (t.contains('.') && !t.contains(' ') && t.len() > 3);
        if looks_like_url {
            let uri = crate::session::normalize_uri(t);
            out.push(Suggestion {
                kind: "URL",
                title: t.to_string(),
                sub: String::from("Open address"),
                uri: uri.clone(),
            });
        }

        // 2) Bookmarks + 3) History
        let db = self.state.db.borrow();
        for bm in kestrel_data::bookmarks::search(&db, t, 3) {
            if out.len() >= 6 {
                break;
            }
            if out.iter().any(|s| s.uri == bm.url) {
                continue;
            }
            out.push(Suggestion {
                kind: "BOOKMARK",
                title: if bm.title.is_empty() { bm.url.clone() } else { bm.title },
                sub: bm.url.clone(),
                uri: bm.url,
            });
        }
        for h in kestrel_data::history::search(&db, t, 3) {
            if out.len() >= 6 {
                break;
            }
            if out.iter().any(|s| s.uri == h.url) {
                continue;
            }
            out.push(Suggestion {
                kind: "HISTORY",
                title: if h.title.is_empty() { h.url.clone() } else { h.title },
                sub: h.url.clone(),
                uri: h.url,
            });
        }

        // 4) Search engine fallback
        if out.is_empty() || !looks_like_url {
            let se = self.state.settings.borrow().search_url(t);
            out.push(Suggestion {
                kind: "SEARCH",
                title: format!("Search for \u{201c}{t}\u{201d}"),
                sub: se.clone(),
                uri: se,
            });
        }

        out
    }

    pub fn set_uri_text(&self, uri: &str) {
        self.navigating.set(true);
        self.entry.set_text(uri);
        self.entry.set_position(-1);
        self.navigating.set(false);
    }

    pub fn navigate(&self, text: &str) {
        self.popover.popdown();
        let t = text.trim();
        let looks_like_url = t.starts_with("http://")
            || t.starts_with("https://")
            || t.starts_with("kestrel://")
            || t.starts_with("file://")
            || t.starts_with("localhost")
            || (t.contains('.') && !t.contains(' ') && !t.is_empty());
        let uri = if looks_like_url {
            crate::session::normalize_uri(t)
        } else {
            self.state.settings.borrow().search_url(t)
        };
        if let Some(w) = self.state.main_window() {
            w.load_active(&uri);
        }
        // Move focus back to the page.
        self.entry.set_focusable(false);
        self.entry.set_focusable(true);
    }

    pub fn set_security(&self, uri: &str, lookalike: bool) {
        let secure = uri.starts_with("https://");
        let internal =
            uri.starts_with("kestrel://") || uri.starts_with("file://") || uri.is_empty();
        self.sec_icon.set_visible(!internal);
        if internal {
            return;
        }
        if lookalike {
            self.sec_icon.set_icon_name(Some("channel-insecure-symbolic"));
            self.sec_icon.add_css_class("warn");
            self.sec_icon.remove_css_class("secure");
            self.sec_icon
                .set_tooltip_text(Some("Possible lookalike domain (punycode). Check carefully!"));
        } else if secure {
            self.sec_icon.set_icon_name(Some("changes-prevent-symbolic"));
            self.sec_icon.remove_css_class("warn");
            self.sec_icon.add_css_class("secure");
            self.sec_icon.set_tooltip_text(Some("Connection is secure (HTTPS)"));
        } else {
            self.sec_icon.set_icon_name(Some("channel-insecure-symbolic"));
            self.sec_icon.remove_css_class("secure");
            self.sec_icon.add_css_class("warn");
            self.sec_icon.set_tooltip_text(Some("Not secure (HTTP)"));
        }
    }

    pub fn set_star(&self, active: bool) {
        if let Some(icon) = self.star.first_child().and_downcast::<gtk::Image>() {
            icon.set_icon_name(Some(if active {
                "starred-symbolic"
            } else {
                "star-symbolic"
            }));
        }
        if active {
            self.star.add_css_class("active");
        } else {
            self.star.remove_css_class("active");
        }
    }

    pub fn set_shield_count(&self, n: u64) {
        self.shield_btn.set_visible(true);
        self.shield_label.set_text(&n.to_string());
    }
}
