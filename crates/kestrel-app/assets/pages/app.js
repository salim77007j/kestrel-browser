// Kestrel internal pages runtime: bridge + state binding helpers.
"use strict";
window.__kestrel = (function () {
  let state = null;
  const listeners = [];

  function send(cmd, extra) {
    const payload = Object.assign({ cmd: cmd }, extra || {});
    if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.kestrelHost) {
      window.webkit.messageHandlers.kestrelHost.postMessage(JSON.stringify(payload));
    } else {
      console.warn("no bridge", payload);
    }
  }

  function receiveState(s) {
    state = s;
    document.documentElement.dataset.theme = (s && s.settings && s.settings.theme) || "dark";
    listeners.forEach(function (fn) { try { fn(s); } catch (e) { console.error(e); } });
    render();
  }

  function onMessage(msg) {
    if (msg && msg.type === "test-url-result") {
      const el = document.getElementById("test-url-result");
      if (el) {
        el.textContent = msg.result;
        el.style.display = "block";
      }
    }
  }

  function getState() { return state; }
  function onChange(fn) { listeners.push(fn); }

  // ---- default renderers (pages override by defining window.pageRender) ----
  function esc(s) {
    return String(s == null ? "" : s)
      .replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function render() {
    if (window.pageRender) window.pageRender(state, esc);
  }

  // ---- binding helpers used by pages ----
  function bindToggle(id, key, cb) {
    const el = document.getElementById(id);
    if (!el) return;
    el.checked = key();
    el.addEventListener("change", function () { cb(el.checked); });
  }

  return { send: send, receiveState: receiveState, onMessage: onMessage, getState: getState, onChange: onChange, bindToggle: bindToggle, esc: esc };
})();
