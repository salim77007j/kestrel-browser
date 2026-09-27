//! Injection scripts: the chrome↔page bridge (keyboard shortcuts, mouse
//! gestures, find-in-page, DNT surface) and the chrome UI bootstrap.

use crate::state::AppState;
use tauri::Manager;

/// Script for the chrome UI webview. The chrome is a local page with full
/// IPC, so this only guards against double-init and adds drag regions.
pub fn chrome_init() -> String {
    String::from(
        r#"(function(){
  if (window.__kestrelChrome) return; window.__kestrelChrome = true;
  // Any middle-click / auxclick default must not navigate
  window.addEventListener('auxclick', e => { if (e.button === 1) e.preventDefault(); }, true);
})();"#,
    )
}

/// Fingerprint protection script (settings-aware, per-session seed).
pub fn fingerprint_init(app: &tauri::AppHandle) -> String {
    let state = app.state::<AppState>();
    let s = state.settings.read().unwrap().fingerprint.clone();
    kestrel_privacy::fingerprint::build_init_script(&s, state.fp_seed)
}

/// Script for every tab (internal pages + remote sites).
///
/// Remote pages only have `core:event:allow-emit` IPC access (capability
/// `page-bridge`), so everything flows through a single validated event:
/// `page-event`. Rust whitelists the accepted payloads.
pub fn page_init() -> String {
    String::from(
        r#"(function(){
  if (window.__kestrelBridge) return; window.__kestrelBridge = true;

  function emit(payload){
    try {
      if (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.emit){
        window.__TAURI__.event.emit('page-event', payload);
      }
    } catch(e){}
  }

  // ---------- keyboard shortcuts from page focus ----------
  // Chrome-like set; the chrome UI executes the real actions.
  window.addEventListener('keydown', function(e){
    const mod = e.ctrlKey || e.metaKey;
    if (!mod && !e.altKey && e.key !== 'F11' && e.key !== 'F12') return;
    const k = e.key.toLowerCase();
    const interesting =
      (mod && ['t','w','l','d','f','r','h','j','p','n','u','q'].includes(k)) ||
      (mod && e.shiftKey && ['t','n','p','r','delete'].includes(k)) ||
      (mod && e.altKey) ||
      (mod && ['+','-','=','0'].includes(k)) ||
      (mod && e.key >= '1' && e.key <= '9') ||
      e.key === 'F11' || e.key === 'F12' || (e.altKey && ['ArrowLeft','ArrowRight','Home'].includes(e.key));
    if (!interesting) return;
    // let plain ctrl+r / F5 reload be handled natively? No: uniform handling.
    e.preventDefault();
    emit({t:'key', key:k, code:e.key, ctrl:mod, shift:e.shiftKey, alt:e.altKey, meta:e.metaKey});
  }, {capture:true});

  // ---------- mouse gestures (right-drag) ----------
  let gx = 0, gy = 0, tracking = false, moved = false;
  window.addEventListener('mousedown', function(e){
    if (e.button === 2){ gx = e.clientX; gy = e.clientY; tracking = true; moved = false; }
  }, true);
  window.addEventListener('mousemove', function(e){
    if (!tracking) return;
    if (Math.abs(e.clientX - gx) > 28 || Math.abs(e.clientY - gy) > 28){ moved = true; }
  }, true);
  window.addEventListener('mouseup', function(e){
    if (e.button !== 2 || !tracking) return;
    tracking = false;
    if (!moved) return; // plain right-click: native menu
    e.preventDefault();
    const dx = e.clientX - gx, dy = e.clientY - gy;
    let name = null;
    if (dx < -40 && Math.abs(dy) < 40) name = 'back';
    else if (dx > 40 && Math.abs(dy) < 40) name = 'forward';
    else if (dy < -40 && Math.abs(dx) < 40) name = 'newtab';
    else if (dy > 40 && Math.abs(dx) < 40) name = 'reload';
    else if (dy < -40 && dx > 40) name = 'closetab';
    if (name) emit({t:'gesture', name});
  }, true);
  window.addEventListener('contextmenu', function(e){
    if (moved || tracking){ e.preventDefault(); moved = false; }
  }, true);

  // ---------- find in page ----------
  window.__kestrelFind = {
    query: null,
    count: 0,
    index: 0,
    find: function(q){
      this.query = q;
      this.count = 0;
      try {
        const text = (document.body && document.body.innerText) || '';
        if (q){
          const hay = text.toLowerCase();
          const needle = q.toLowerCase();
          let i = 0;
          while ((i = hay.indexOf(needle, i)) !== -1){ this.count++; i += needle.length; }
        }
      } catch(e){}
      if (q && this.count > 0 && window.find){ window.find(q, false, false, true, false, false, false); this.index = 1; }
      emit({t:'find', count: this.count, index: this.index, query: q});
      return this.count;
    },
    step: function(dir){
      if (!this.query) return;
      if (window.find){ window.find(this.query, false, dir < 0, true, false, false, false); }
      this.index = Math.max(1, this.index + dir);
      if (this.index > this.count) this.index = 1;
      emit({t:'find', count: this.count, index: this.index, query: this.query});
    },
    exit: function(){
      try { const sel = window.getSelection(); if (sel) sel.removeAllMatches ? sel.removeAllMatches() : sel.collapseToEnd(); } catch(e){}
      this.query = null; this.count = 0; this.index = 0;
      emit({t:'find', count: 0, index: 0, query: ''});
    }
  };

  // ---------- DNT / GPC surface for scripts ----------
  try { Object.defineProperty(navigator, 'doNotTrack', { get: function(){ return __DNT__; }, configurable: true }); } catch(e){}
  try { navigator.globalPrivacyControl = __GPC__; } catch(e){}

  // ---------- save page helper (chunked to stay under IPC limits) ----------
  window.__kestrelSavePage = function(){
    try {
      const html = '<!DOCTYPE html>\n' + document.documentElement.outerHTML;
      const CHUNK = 400000;
      const total = Math.ceil(html.length / CHUNK);
      emit({t:'save-start', total});
      for (let i = 0; i < total; i++){
        emit({t:'save-chunk', seq:i, data: html.substr(i * CHUNK, CHUNK)});
      }
      emit({t:'save-end'});
    } catch(e){
      emit({t:'save-error', error: String(e)});
    }
  };
})();"#
    )
}

/// Same as [`page_init`] but honoring the DNT/GPC settings.
pub fn page_init_with_privacy(app: &tauri::AppHandle) -> String {
    let (dnt, gpc) = {
        let state = app.state::<AppState>();
        let s = state.settings.read().unwrap();
        (s.do_not_track, s.global_privacy_control)
    };
    page_init()
        .replace("__DNT__", if dnt { "'1'" } else { "'0'" })
        .replace("__GPC__", if gpc { "true" } else { "false" })
}
