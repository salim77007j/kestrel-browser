//! Platform integration: engine-native back/forward + tab muting.
//!
//! Tauri 2 does not expose navigation history on the webview, so we go
//! through the platform webview: ICoreWebView2 on Windows. Unsupported
//! platforms report `false` so the UI can stay honest.

#[cfg(target_os = "windows")]
pub mod windows_impl {
    use tauri::Webview;
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2, ICoreWebView2_4};
    use windows::core::Interface;
    use windows::Win32::Foundation::BOOL;

    fn with_core<F>(webview: &Webview<tauri::Wry>, f: F) -> bool
    where
        F: FnOnce(&ICoreWebView2) -> bool,
    {
        let mut ok = false;
        let cell = std::sync::Mutex::new(&mut ok);
        let _ = webview.with_webview(move |platform| {
            match unsafe { platform.controller.CoreWebView2() } {
                Ok(core) => {
                    let r = f(&core);
                    if let Ok(mut c) = cell.lock() {
                        **c = r;
                    }
                }
                Err(_) => {}
            }
        });
        ok
    }

    pub fn go_back(webview: &Webview<tauri::Wry>) -> bool {
        with_core(webview, |core| unsafe { core.GoBack().is_ok() })
    }

    pub fn go_forward(webview: &Webview<tauri::Wry>) -> bool {
        with_core(webview, |core| unsafe { core.GoForward().is_ok() })
    }

    /// Engine-level stop (Esc).
    pub fn stop(webview: &Webview<tauri::Wry>) -> bool {
        with_core(webview, |core| unsafe { core.Stop().is_ok() })
    }

    pub fn can_go_back(webview: &Webview<tauri::Wry>) -> bool {
        with_core(webview, |core| {
            let mut b = BOOL::default();
            unsafe { core.CanGoBack(&mut b).is_ok() && b.as_bool() }
        })
    }

    pub fn can_go_forward(webview: &Webview<tauri::Wry>) -> bool {
        with_core(webview, |core| {
            let mut b = BOOL::default();
            unsafe { core.CanGoForward(&mut b).is_ok() && b.as_bool() }
        })
    }

    /// Mute a tab through ICoreWebView2_4::SetIsMuted.
    pub fn set_muted(webview: &Webview<tauri::Wry>, muted: bool) -> bool {
        let mut ok = false;
        let cell = std::sync::Mutex::new(&mut ok);
        let _ = webview.with_webview(move |platform| {
            if let Ok(core) = unsafe { platform.controller.CoreWebView2() } {
                if let Ok(core4) = core.cast::<ICoreWebView2_4>() {
                    let r = unsafe { core4.SetIsMuted(muted.into()) };
                    if let Ok(mut c) = cell.lock() {
                        **c = r.is_ok();
                    }
                }
            }
        });
        ok
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::{
    can_go_back as can_back, can_go_forward as can_forward, go_back, go_forward, set_muted, stop,
};

#[cfg(not(target_os = "windows"))]
pub fn go_back(_webview: &tauri::Webview<tauri::Wry>) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn go_forward(_webview: &tauri::Webview<tauri::Wry>) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn stop(_webview: &tauri::Webview<tauri::Wry>) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn set_muted(_webview: &tauri::Webview<tauri::Wry>, _muted: bool) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn can_back(_webview: &tauri::Webview<tauri::Wry>) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn can_forward(_webview: &tauri::Webview<tauri::Wry>) -> bool {
    false
}
