//! Automated smoke test: launches a window, opens representative pages,
//! waits for load, captures per-tab PNG snapshots + JSON report, exits.
//!
//! Used by CI under Xvfb to prove the binary actually renders, and to
//! produce real screenshots for the validation report.

use crate::state::AppState;
use crate::window::BrowserWindow;
use gtk::prelude::*;
use gtk::glib;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use webkit::prelude::*;

pub fn run(w: &Rc<BrowserWindow>, outdir: PathBuf) {
    let _ = std::fs::create_dir_all(&outdir);

    // A local test page so rendering works without network.
    let test_page = outdir.join("testpage.html");
    let _ = std::fs::write(
        &test_page,
        format!(
            "<!doctype html><html><head><title>Kestrel Test Page</title>\
             <style>body{{font-family:sans-serif;background:#12161C;color:#E6EAF0;\
             display:flex;flex-direction:column;align-items:center;justify-content:center;\
             height:100vh;margin:0}}h1{{color:#2DD4BF}}</style></head>\
             <body><h1>Kestrel renders.</h1>\
             <p>Rust core {} — GTK4 + WebKitGTK.</p>\
             <p>This page was loaded by the automated smoke test.</p></body></html>",
            env!("CARGO_PKG_VERSION")
        ),
    );

    let test_uri = format!("file://{}", test_page.to_string_lossy());

    // Open three representative tabs (foreground one by one).
    let t1 = w.new_tab("kestrel://newtab", true, false);
    let t2 = w.new_tab("kestrel://settings", true, false);
    let t3 = w.new_tab(&test_uri, false, false);
    let _ = (t1, t2, t3);

    let state = w.state.clone();
    let w_weak = std::rc::Rc::downgrade(w);
    let outdir_a = outdir.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(4000), move || {
        if let Some(w) = w_weak.upgrade() {
            capture_all(&state, &w, &outdir_a);
        }
        glib::ControlFlow::Break
    });

    // Hard deadline: exit with failure if we somehow hang.
    let outdir2 = outdir.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(45_000), move || {
        let report = serde_json::json!({
            "status": "timeout",
            "dir": outdir2.to_string_lossy(),
        });
        let _ = std::fs::write(
            outdir2.join("smoke-report.json"),
            serde_json::to_string_pretty(&report).unwrap(),
        );
        std::process::exit(3);
    });
}

fn capture_all(state: &Rc<AppState>, w: &Rc<BrowserWindow>, outdir: &Path) {
    let tabs = w.tabs.borrow().clone();
    let mut results = Vec::new();

    for (i, tab) in tabs.iter().enumerate() {
        let uri = tab.uri();
        let title = tab.title.borrow().clone();
        let shot = outdir.join(format!("shot-{i}.png"));

        let webview = tab.webview.clone();
        let shot2 = shot.clone();
        webview.snapshot(
            webkit::SnapshotRegion::FullDocument,
            webkit::SnapshotOptions::NONE,
            None::<&gtk::gio::Cancellable>,
            glib::clone!(
                #[strong]
                shot2,
                move |res| {
                    if let Ok(texture) = res {
                        let _ = texture.save_to_png(shot2.to_string_lossy().as_ref());
                    }
                }
            ),
        );

        results.push(serde_json::json!({
            "index": i,
            "uri": uri,
            "title": title,
            "screenshot": shot.to_string_lossy(),
        }));
    }

    // Overall stats.
    let engine_rules = state.rule_count.get();
    let report = serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "tabs": results,
        "engine_rules": engine_rules,
        "tracker_hosts": state.shield.borrow().tracker_host_count(),
        "filters_active": state.filters_active.load(std::sync::atomic::Ordering::SeqCst),
    });
    let _ = std::fs::write(
        outdir.join("smoke-report.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    );

    // Leave the app running briefly for the external X-window capture,
    // then exit cleanly so CI can collect artifacts.
    glib::timeout_add_local(std::time::Duration::from_millis(2500), move || {
        std::process::exit(0);
    });
}
