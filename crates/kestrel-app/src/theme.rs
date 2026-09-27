//! Theme engine: applies the Kestrel GTK stylesheet (dark / light).

use gtk::gdk;
use gtk::prelude::*;
use gtk::Settings;

const DARK_CSS: &str = include_str!("../assets/kestrel-gtk.css");
const LIGHT_CSS: &str = include_str!("../assets/kestrel-gtk-light.css");

pub fn apply(theme: &str) {
    let display = gdk::Display::default().expect("no display");
    let dark = theme != "light";

    let settings = Settings::default();
    settings.set_gtk_application_prefer_dark_theme(dark);

    let provider = gtk::CssProvider::new();
    let css = if dark { DARK_CSS } else { LIGHT_CSS };
    provider.load_from_data(css.as_bytes());

    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
