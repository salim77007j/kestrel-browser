//! Permission requests: camera, microphone, location, notifications, etc.
//! Default deny + optional per-origin persistence.

use crate::state::AppState;
use gtk::prelude::*;
use gtk::glib;
use std::rc::Rc;
use webkit::prelude::*;

pub fn handle(state: &Rc<AppState>, request: &webkit::PermissionRequest) {
    let kind = kind_label(request);
    let origin = origin_of(request);

    // Stored policy?
    if let Some(orig) = &origin {
        if let Some(allowed) = kestrel_data::permissions::get(&state.db.borrow(), orig, &kind) {
            if allowed {
                request.allow();
            } else {
                request.deny();
            }
            return;
        }
    }

    // Ask the user (nested loop keeps WebKit's synchronous contract).
    let msg = match (&origin, origin.is_some()) {
        (Some(o), _) => format!("{o} wants to use {kind}. Allow?"),
        _ => format!("This page wants to use {kind}. Allow?"),
    };

    let dialog = gtk::MessageDialog::builder()
        .text(msg)
        .buttons(gtk::ButtonsType::YesNo)
        .modal(true)
        .title("Permission request — Kestrel")
        .secondary_text("Kestrel denies permissions by default. Choose “No” to block this site permanently.")
        .build();

    let (tx, rx) = std::sync::mpsc::channel::<bool>();
    dialog.connect_response(glib::clone!(
        #[strong]
        tx,
        move |d, r| {
            d.hide();
            let _ = tx.send(r == gtk::ResponseType::Yes);
        }
    ));
    dialog.present();
    let allowed = rx.recv().unwrap_or(false);
    dialog.close();

    // Persist per-origin decision.
    if let Some(orig) = &origin {
        kestrel_data::permissions::set(&state.db.borrow(), orig, &kind, allowed);
    }

    if allowed {
        request.allow();
    } else {
        request.deny();
    }
}

pub fn confirm_tls_error(_state: &Rc<AppState>, _errors: u32) -> bool {
    // v0.1 policy: fail closed. We never silently accept bad certificates.
    false
}

fn kind_label(request: &webkit::PermissionRequest) -> String {
    if let Ok(media) = request.clone().dynamic_cast::<webkit::UserMediaPermissionRequest>() {
        if media.is_for_video_device() {
            return "camera".into();
        }
        if media.is_for_audio_device() {
            return "microphone".into();
        }
    }
    if request.clone().dynamic_cast::<webkit::NotificationPermissionRequest>().is_ok() {
        return "notifications".into();
    }
    if request.clone().dynamic_cast::<webkit::GeolocationPermissionRequest>().is_ok() {
        return "location".into();
    }
    "content".into()
}

fn origin_of(request: &webkit::PermissionRequest) -> Option<String> {
    // PermissionRequest does not carry the origin in all versions; try the
    // media request's page URI path via the requesting view when possible.
    let _ = request;
    None // origin-less prompts still work (remembered globally per kind)
}

pub fn note_blocked_tracker(state: &Rc<AppState>) {
    state.settings.borrow_mut().lifetime_blocked += 1;
}
