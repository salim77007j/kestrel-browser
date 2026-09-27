//! Kestrel Browser — fast, private, feather-light.
//!
//! Tauri 2 shell: one OS window hosting the chrome UI webview plus one
//! webview per tab (multiwebview). The privacy engine, data layer and
//! platform integration live in this crate.

mod bridge;
mod commands;
mod engine_host;
mod intercept;
mod net;
mod omnibox;
mod platform;
mod state;
mod stats;
mod tabs;

use state::AppState;
use tauri::{Manager, WindowEvent};

pub fn run() {
    let builder = tauri::Builder::default()
        .setup(|app| {
            let app = app.handle().clone();
            let state = AppState::bootstrap(&app)?;
            app.manage(state);

            // Page-content events (shortcuts, gestures, find, save-page)
            // arrive from any webview — remote pages may only EMIT this
            // event (capability `page-bridge`); every payload is validated
            // in commands::handle_page_event.
            {
                let app2 = app.clone();
                app.listen_any("page-event", move |e| {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(e.payload()) {
                        commands::handle_page_event(&app2, v);
                    }
                });
            }

            // Build the privacy engine in the background; the shell works
            // fail-open until it is ready (usually well under a second on
            // modern hardware for the bundled seed lists).
            engine_host::spawn_engine_build(app.clone());
            net::spawn_favicon_dir(app.clone());

            tabs::create_main_window(&app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::init_ui,
            commands::create_tab,
            commands::close_tab,
            commands::close_other_tabs,
            commands::activate_tab,
            commands::reorder_tab,
            commands::pin_tab,
            commands::mute_tab,
            commands::duplicate_tab,
            commands::reopen_closed_tab,
            commands::group_tabs,
            commands::ungroup_tab,
            commands::set_group_collapsed,
            commands::navigate_active,
            commands::navigate_tab,
            commands::nav_back,
            commands::nav_forward,
            commands::reload_active,
            commands::stop_active,
            commands::set_zoom,
            commands::add_bookmark,
            commands::remove_bookmark,
            commands::rename_bookmark,
            commands::move_bookmark_to_bar,
            commands::toggle_bookmark_current,
            commands::list_bookmarks,
            commands::history_search,
            commands::delete_history_item,
            commands::forget_site,
            commands::clear_history,
            commands::list_downloads,
            commands::cancel_download,
            commands::clear_finished_downloads,
            commands::open_download,
            commands::show_in_folder,
            commands::get_settings,
            commands::update_settings,
            commands::add_custom_filter,
            commands::remove_custom_filter,
            commands::update_filter_lists_now,
            commands::get_adblock_stats,
            commands::run_privacy_selftest,
            commands::open_devtools,
            commands::save_page,
            commands::print_page,
            commands::find_in_page,
            commands::find_step,
            commands::find_exit,
            commands::set_chrome_height,
            commands::get_favicon,
            commands::omnibox_suggest,
            commands::add_shortcut,
            commands::get_shortcuts,
            commands::open_internal,
            commands::remove_shortcut,
            commands::get_weather,
            commands::allow_dangerous_site,
            commands::set_permission,
            commands::list_permissions,
            commands::clear_permissions,
            commands::clear_browsing_data,
            commands::force_quit,
            commands::open_external,
            commands::set_fullscreen,
            commands::page_event,
        ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running Kestrel");
}

/// Shared window-event wiring: relayout tab webviews on resize and persist
/// the session on close.
pub fn handle_window_event(app: &tauri::AppHandle, event: &WindowEvent) {
    if let WindowEvent::Resized(_) = event {
        tabs::relayout(app);
    }
}
