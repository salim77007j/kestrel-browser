//! Find-in-page bar backed by WebKitFindController.

use gtk::prelude::*;
use gtk::{glib, RevealerTransitionType};
use webkit::prelude::*;
use webkit::WebView;

#[derive(Clone)]
pub struct FindBar {
    pub revealer: gtk::Revealer,
    entry: gtk::Entry,
    matches: gtk::Label,
    find: RefCellSlot,
}

type RefCellSlot = std::cell::RefCell<Option<webkit::FindController>>;

impl FindBar {
    pub fn new() -> Self {
        let revealer = gtk::Revealer::new();
        revealer.add_css_class("k-findbar");
        revealer.set_transition_type(RevealerTransitionType::SlideUp);
        revealer.set_reveal_child(false);
        revealer.set_visible(false);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("Find in page…"));
        entry.set_hexpand(true);

        let matches = gtk::Label::new(None);
        matches.add_css_class("matches");
        matches.set_text("");

        let prev = gtk::Button::from_icon_name("go-up-symbolic");
        let next = gtk::Button::from_icon_name("go-down-symbolic");
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        for b in [&prev, &next, &close] {
            b.add_css_class("k-btn");
            b.set_focusable(false);
        }

        row.append(&entry);
        row.append(&matches);
        row.append(&prev);
        row.append(&next);
        row.append(&close);
        revealer.set_child(Some(&row));

        let find: RefCellSlot = std::cell::RefCell::new(None);

        let s = Self { revealer, entry, matches, find };

        {
            let s2 = s.clone();
            entry.connect_activate(move |e| {
                let text = e.text().to_string();
                s2.launch_search(&text, true);
            });
        }

        s
    }

    pub fn attach(&self, webview: &WebView) {
        let fc = webview.find_controller();
        *self.find.borrow_mut() = Some(fc.clone());

        fc.connect_counted_matches(glib::clone!(
            #[weak(rename_to = matches)]
            self.matches,
            move |_, count| {
                matches.set_text(&format!("{count} matches"));
            }
        ));
        fc.connect_found_text(glib::clone!(
            #[weak(rename_to = matches)]
            self.matches,
            move |_, count| {
                if count > 0 {
                    matches.set_text(&format!("1 of {count}"));
                }
            }
        ));
        fc.connect_failed_to_find_text(glib::clone!(
            #[weak(rename_to = matches)]
            self.matches,
            move |_| {
                matches.set_text("no matches");
            }
        ));

        let s2 = self.clone();
        entry.connect_changed(move |e| {
            s2.launch_search(&e.text().to_string(), true);
        });
        let s3 = self.clone();
        next.connect_clicked(move |_| {
            if let Some(fc) = s3.find.borrow().as_ref() {
                fc.search_next();
            }
        });
        let s4 = self.clone();
        prev.connect_clicked(move |_| {
            if let Some(fc) = s4.find.borrow().as_ref() {
                fc.search_previous();
            }
        });
        let entry2 = self.entry.clone();
        let revealer2 = self.revealer.clone();
        close.connect_clicked(move |_| {
            Self::hide_static(&entry2, &revealer2);
        });
    }

    fn launch_search(&self, text: &str, _forward: bool) {
        if text.is_empty() {
            if let Some(fc) = self.find.borrow().as_ref() {
                fc.search_finish();
            }
            self.matches.set_text("");
            return;
        }
        if let Some(fc) = self.find.borrow().as_ref() {
            let opts = webkit::FindOptions::CASE_INSENSITIVE | webkit::FindOptions::WRAP_AROUND;
            fc.search(text, opts.into_glib(), 1000);
        }
    }

    fn hide_static(entry: &gtk::Entry, revealer: &gtk::Revealer) {
        revealer.set_reveal_child(false);
        revealer.set_visible(false);
        entry.set_text("");
    }

    pub fn open(&self) {
        self.revealer.set_visible(true);
        self.revealer.set_reveal_child(true);
        self.entry.grab_focus();
    }

    pub fn close(&self) {
        if let Some(fc) = self.find.borrow().as_ref() {
            fc.search_finish();
        }
        Self::hide_static(&self.entry, &self.revealer);
    }
}
