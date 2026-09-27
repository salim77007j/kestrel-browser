//! Menus: hamburger menu model, tab context menu, web context menu, About.

use crate::window::BrowserWindow;
use gtk::prelude::*;
use gtk::{glib, gio};
use std::rc::Rc;

pub fn install(w: &Rc<BrowserWindow>, btn: &gtk::MenuButton) {
    let menu = gio::Menu::new();

    let sec_new = gio::Menu::new();
    sec_new.append(Some("New Tab"), Some("win.new-tab"));
    sec_new.append(Some("New Private Tab"), Some("win.new-private-tab"));
    sec_new.append(Some("Reopen Closed Tab"), Some("win.reopen-closed"));
    menu.append_section(None, &sec_new);

    let sec_nav = gio::Menu::new();
    sec_nav.append(Some("Zoom In"), Some("win.zoom-in"));
    sec_nav.append(Some("Zoom Out"), Some("win.zoom-out"));
    sec_nav.append(Some("Reset Zoom"), Some("win.zoom-reset"));
    sec_nav.append(Some("Find in Page…"), Some("win.find"));
    sec_nav.append(Some("Print…"), Some("win.print"));
    sec_nav.append(Some("Save Page As…"), Some("win.save-page"));
    sec_nav.append(Some("Full Screen"), Some("win.fullscreen-toggle"));
    sec_nav.append(Some("Developer Tools"), Some("win.devtools"));
    menu.append_section(None, &sec_nav);

    let sec_data = gio::Menu::new();
    sec_data.append(Some("Bookmarks"), Some("win.open-bookmarks"));
    sec_data.append(Some("History"), Some("win.open-history"));
    sec_data.append(Some("Downloads"), Some("win.open-downloads"));
    menu.append_section(None, &sec_data);

    let sec_priv = gio::Menu::new();
    sec_priv.append(Some("Privacy Dashboard"), Some("win.open-privacy"));
    sec_priv.append(Some("Settings"), Some("win.open-settings"));
    menu.append_section(None, &sec_priv);

    let sec_about = gio::Menu::new();
    sec_about.append(Some("About Kestrel"), Some("win.about"));
    menu.append_section(None, &sec_about);

    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.add_css_class("k-menu");
    btn.set_popover(Some(&popover));
}

pub fn show_web_context_menu(w: &Rc<BrowserWindow>, is_link: bool, link: &str) {
    let menu = gio::Menu::new();

    let sec_nav = gio::Menu::new();
    sec_nav.append(Some("Back"), Some("win.go-back"));
    sec_nav.append(Some("Forward"), Some("win.go-forward"));
    sec_nav.append(Some("Reload"), Some("win.reload"));
    menu.append_section(None, &sec_nav);

    if is_link && !link.is_empty() {
        let sec_link = gio::Menu::new();
        let open_item = gio::MenuItem::new(Some("Open Link in New Tab"), None);
        open_item.set_action_and_target_value(Some("win.open-link"), Some(&link.to_variant()));
        sec_link.append_item(&open_item);
        let copy_item = gio::MenuItem::new(Some("Copy Link"), None);
        copy_item.set_action_and_target_value(Some("win.copy-link"), Some(&link.to_variant()));
        sec_link.append_item(&copy_item);
        menu.append_section(None, &sec_link);
    }

    let sec_page = gio::Menu::new();
    sec_page.append(Some("Save Page As…"), Some("win.save-page"));
    sec_page.append(Some("Print…"), Some("win.print"));
    sec_page.append(Some("Find in Page…"), Some("win.find"));
    sec_page.append(Some("Developer Tools"), Some("win.devtools"));
    menu.append_section(None, &sec_page);

    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.add_css_class("k-menu");
    popover.set_parent(&w.stack);
    popover.connect_closed(|p| {
        p.unparent();
    });
    popover.popup();
}

pub fn show_about(w: &Rc<BrowserWindow>) {
    let about = gtk::AboutDialog::builder()
        .program_name("Kestrel")
        .version(env!("CARGO_PKG_VERSION"))
        .comments("An ultra-fast, privacy-first browser with a tiny footprint.")
        .website("https://github.com/salim77007j/kestrel-browser")
        .website_label("Project on GitHub")
        .license_type(gtk::License::Mpl20)
        .logo_icon_name("io.kestrel.Browser")
        .title("About Kestrel")
        .build();
    about.set_transient_for(Some(&w.win));
    about.present();
}

fn copy_to_clipboard(text: &str) {
    let display = gtk::gdk::Display::default().unwrap();
    let clipboard = display.clipboard();
    clipboard.set_text(text);
}

// Keep glib in scope for to_variant()/to_string() trait usage above.
#[allow(unused_imports)]
use glib::prelude::*;
