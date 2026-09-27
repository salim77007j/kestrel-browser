<div align="center">
  <img src="assets/icons/io.kestrel.Browser.svg" width="72" alt="Kestrel logo">
  <h1>Kestrel Browser</h1>
  <p><b>Fast. Private. Feather-light.</b></p>
  <p>An independent, privacy-first web browser with a memory-safe Rust core, GTK4 UI and WebKitGTK rendering.</p>
  <p>
    <a href="https://github.com/salim77007j/kestrel-browser/actions"><img alt="CI" src="https://img.shields.io/badge/CI-GitHub_Actions-2DD4BF"></a>
    <img alt="license" src="https://img.shields.io/badge/license-MPL--2.0-93A0B4">
    <img alt="lang" src="https://img.shields.io/badge/core-Rust-DEA584">
  </p>
</div>

---

## Why Kestrel

Mainstream browsers carry decades of legacy architecture — millions of lines,
heavy processes, and an ad business. Kestrel takes the opposite bet:

- **Rust core** — engine integration, privacy systems, data layer and network
  policy are memory-safe Rust; UI is GTK4; rendering is WebKitGTK
  (multi-process, bubblewrap-sandboxed, GPU-accelerated).
- **Lean by construction** — mature libraries (adblock-rust, WebKitGTK,
  SQLite) instead of hand-rolled infrastructure. Small, auditable codebase.
- **Privacy is the default, not a mode** — ads and trackers are blocked at the
  network level via compiled WebKit content filters, cosmetic filtering hides
  the leftovers, and fingerprint readbacks are randomized.
- **No telemetry. Ever.** Local SQLite, no accounts, no cloud.

## Feature overview

| Area | What you get |
| --- | --- |
| Tabs | Create/close/reopen, pin, mute, private tabs (ephemeral session), keyboard navigation |
| Address bar | Security indicator, shield badge with live counters, history/bookmark suggestions, search integration |
| Privacy | Network + cosmetic ad blocking (EasyList/EasyPrivacy/Annoyances + custom filters), tracker audit, anti-fingerprinting (canvas/WebGL/audio/hardware/fonts), third-party cookie blocking, WebRTC off, fail-closed TLS, per-origin permissions, popup blocking |
| Data | Bookmarks, history, downloads managers, session restore + crash recovery |
| UI | Dark/light themes, internal pages (`kestrel://`), privacy dashboard, find-in-page, zoom, fullscreen, print, save page (MHTML), DevTools, custom context menus, mouse gestures |
| Security | Memory-safe core, process isolation (WebKit), sandboxing (bubblewrap), OS-keyring password vault, local phishing-lookalike warnings |
| Performance | Fast cold start, thin LTO binary, GPU compositing, background-tab throttling (WebKit), tab discarding under memory pressure |

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│ kestrel-app (bin)  GTK4 UI: tabs, toolbar, internal pages │
│   │  webview.rs      — per-page policy, shield wiring     │
│   │  bridge.rs        — JS⇄Rust bridge (internal pages)   │
│   │  pages.rs         — kestrel:// scheme (embedded HTML) │
├───┴──────────────────────────────────────────────────────┤
│ kestrel-privacy        kestrel-data                       │
│  adblock-rust engine    SQLite: history/bookmarks/        │
│  ABP→WebKit converter   downloads/permissions/vault index │
│  fingerprint scripts    JSON settings · session journal   │
├──────────────────────────────────────────────────────────┤
│ WebKitGTK 6.0 (multi-process + bubblewrap sandbox)        │
│  network-process content filters · WebKit settings API    │
└──────────────────────────────────────────────────────────┘
```

### Why WebKitGTK (not Qt6, not CEF, not Electron)

| Option | Verdict |
| --- | --- |
| **WebKitGTK + GTK4 (chosen)** | Lightest embeddable production engine; GTK4 is native-Rust-bound (gtk-rs); multi-process + sandboxed out of the box; what Epiphany runs in production |
| Qt6 + QtWebEngine | Wraps Chromium — ships an entire second engine, C++ boundary, heavier RAM |
| CEF | Chromium again — hundreds of MB, bloated |
| Tauri multi-webview | Tab-in-one-window support on Linux still immature |
| Custom engine | Not production-viable in this scope |

## Build

```bash
# Debian/Ubuntu
sudo apt install libgtk-4-dev libwebkitgtk-6.0-dev
cargo build --release
./target/release/kestrel
```

## CI

GitHub Actions builds the release binary, runs unit tests (privacy engine,
converter, data stores), executes a **real smoke test under Xvfb** (opens
tabs, renders pages, captures screenshots, asserts filter activity),
measures RAM/CPU, and publishes `.tar.gz` + `.deb` artifacts on every push.

## Privacy design notes

- Network blocking is compiled into WebKit's content-filter store: rules run
  at C speed inside the network process — no per-request overhead in the UI.
- The `kestrelHost` JS bridge is registered **only** on internal pages; web
  pages cannot invoke browser commands.
- Fingerprint scripts run per-origin with a session salt — noise is stable
  within a session per site (so pages don't break) but randomizes across
  sessions.
- The password vault uses the OS secret service (libsecret); if no keyring is
  available the vault disables itself rather than falling back to plaintext.
- TLS errors fail closed. There is no silent bypass.

## License

MPL-2.0. Filter lists are the property of the EasyList/uBlock maintainers and
are fetched at runtime under their own terms.
