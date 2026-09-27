//! Find-in-page bar backed by WebKitFindController.

use gtk::prelude::*;
use glib::translate::IntoGlib;
use gtk::{glib, RevealerTransitionType};
use std::rc::Rc;
use webkit::prelude::*;
use webkit::WebView;

#[derive(Clone)]
pub struct FindBar {
    pub revealer: gtk::Revealer,
    entry: gtk::Entry,
    matches: gtk::Label,
    find: std::cell::RefCell<Option<webkit::FindController>>,
}

impl FindBar {
    pub fn new() -> Rc<Self> {
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

        Rc::new_cyclic(|slot: &std::rc::Weak<FindBar>| {
            let weak = slot.clone();
            entry.connect_activate(move |e| {
                if let Some(s) = weak.upgrade() {
                    s.launch_search(&e.text().to_string());
                }
            });
            let weak2 = slot.clone();
            entry.connect_changed(move |e| {
                if let Some(s) = weak2.upgrade() {
                    s.launch_search(&e.text().to_string());
                }
            });
            let weak3 = slot.clone();
            next.connect_clicked(move |_| {
                if let Some(s) = weak3.upgrade() {
                    if let Some(fc) = s.find.borrow().as_ref() {
                        fc.search_next();
                    }
                }
            });
            let weak4 = slot.clone();
            prev.connect_clicked(move |_| {
                if let Some(s) = weak4.upgrade() {
                    if let Some(fc) = s.find.borrow().as_ref() {
                        fc.search_previous();
                    }
                }
            });
            let entry2 = entry.clone();
            let revealer2 = revealer.clone();
            close.connect_clicked(move |_| {
                Self::hide_static(&entry2, &revealer2);
            });

            Self {
                revealer,
                entry,
                matches,
                find: std::cell::RefCell::new(None),
            }
        })
    }

    pub fn attach(&self, webview: &WebView) {
        if let Some(fc) = webview.find_controller() {
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
                    matches.set_text(&format!("1 of {count}"));
                }
            ));
            fc.connect_failed_to_find_text(glib::clone!(
                #[weak(rename_to = matches)]
                self.matches,
                move |_| {
                    matches.set_text("no matches");
                }
            ));
            *self.find.borrow_mut() = Some(fc);
        }
    }

    fn launch_search(&self, text: &str) {
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

