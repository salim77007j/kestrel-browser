//! Mouse gestures: right-button drag for back/forward/reload navigation.

use crate::window::BrowserWindow;
use gtk::prelude::*;
use gtk::glib;
use std::cell::Cell;
use std::rc::Rc;

pub fn install(w: &Rc<BrowserWindow>) {
    if !w.state.settings.borrow().gestures {
        return;
    }

    let start_x = Cell::new(0.0);
    let start_y = Cell::new(0.0);
    let dragging = Cell::new(false);
    let suppress_menu = Cell::new(false);

    // Right-button press/release on the stack area.
    let click = gtk::GestureClick::new();
    click.set_button(3);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);

    click.connect_pressed(glib::clone!(
        #[weak]
        w,
        move |gesture, _, x, y| {
            start_x.set(x);
            start_y.set(y);
            dragging.set(false);
            suppress_menu.set(false);
            let _ = w;
            gesture.set_state(gtk::EventSequenceState::None);
        }
    ));

    click.connect_released(glib::clone!(
        #[weak]
        w,
        move |gesture, _, _, _| {
            if dragging.get() {
                suppress_menu.set(true);
                gesture.set_state(gtk::EventSequenceState::Claimed);
            }
            dragging.set(false);
        }
    ));
    w.content_overlay.add_controller(click);

    // Drag tracking.
    let drag = gtk::GestureDrag::new();
    drag.set_button(3);
    drag.set_propagation_phase(gtk::PropagationPhase::Capture);
    drag.connect_drag_update(glib::clone!(
        #[weak]
        w,
        move |_, ox, oy| {
            let dx = ox - start_x.get();
            let dy = oy - start_y.get();
            if dx.abs() > 80.0 && !dragging.get() {
                dragging.set(true);
                if dx > 0.0 {
                    if let Some(t) = w.current_tab() {
                        if t.webview.can_go_back() {
                            t.webview.go_back();
                            w.toast_text("Gesture: Back");
                        }
                    }
                } else if let Some(t) = w.current_tab() {
                    if t.webview.can_go_forward() {
                        t.webview.go_forward();
                        w.toast_text("Gesture: Forward");
                    }
                }
            } else if dy.abs() > 90.0 && !dragging.get() {
                dragging.set(true);
                if dy < 0.0 {
                    if let Some(t) = w.current_tab() {
                        t.webview.reload();
                        w.toast_text("Gesture: Reload");
                    }
                }
            }
        }
    ));
    w.content_overlay.add_controller(drag);

    let _ = suppress_menu;
}
