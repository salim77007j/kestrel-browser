<div align="center">
  <img src="src-tauri/icons/128x128.png" width="72" alt="Kestrel logo">
  <h1>Kestrel Browser</h1>
  <p><b>Fast. Private. Feather-light.</b></p>
  <p>An independent, privacy-first browser for <b>Windows</b> — a memory-safe Rust core,
  Tauri 2 multi-webview shell and the Chromium-based WebView2 renderer that ships with
  Windows 10/11. UI follows the approved design in <a href="wed1.png"><code>wed1.png</code></a>.</p>
  <p>
    <a href="https://github.com/salim77007j/kestrel-browser/actions"><img alt="CI" src="https://img.shields.io/badge/CI-GitHub_Actions-2DD4BF"></a>
    <img alt="license" src="https://img.shields.io/badge/license-MPL--2.0-93A0B4">
    <img alt="core" src="https://img.shields.io/badge/core-Rust-DEA584">
    <img alt="engine" src="https://img.shields.io/badge/rendering-WebView2_(Chromium)-4285F4">
  </p>
</div>

---

## Why Kestrel

Mainstream browsers carry decades of legacy and an ad business. Kestrel takes the
opposite bet:

- **Memory-safe Rust core** — the privacy engine, data layer and platform glue are
  Rust (`crates/kestrel-privacy`, `crates/kestrel-data`, `src-tauri`). No C/C++ browser
  shell code to exploit.
- **Lean by construction** — mature building blocks only: Brave's `adblock` engine,
  WebView2, Tauri 2. Small, auditable codebase; no hand-rolled infrastructure.
- **Privacy is the default, not a mode** — network-level ad/tracker blocking with
  cosmetic hiding, HTTPS-only upgrades, third-party cookie stripping, anti-fingerprinting,
  malware/lookalike interstitials. No telemetry, ever. Everything local.
- **Real Windows executable** — portable `kestrel.exe` plus an NSIS installer,
  both built automatically by GitHub Actions on every commit.

## Feature overview

| Area | What you get |
| --- | --- |
| Tabs | Real per-tab webviews: create/close/reopen (Ctrl+Shift+T), pin, mute, duplicate, tab groups (name + color + collapse), drag-free reorder, middle-click close, session restore + crash recovery |
| Address bar | Security indicator, letter/favicon chip, live suggestions (history + bookmarks + engine suggestions), "search or URL" semantics |
| Privacy | Brave adblock-rs engine (EasyList + EasyPrivacy + Fanboy Annoyance + Peter Lowe, ~200k rules, bundled offline), cosmetic element hiding, third-party Set-Cookie stripping, HTTPS-only mode, Do-Not-Track & GPC surface, fingerprint shields (canvas/audio/WebGL/fonts/hardware), permission manager with remembered per-site decisions |
| Safe browsing | URLhaus malware hosts + brand-lookalike detection (IDN-aware, edit distance) with real interstitials |
| Data | Bookmarks bar matching the design (Apps / Google / YouTube / Gmail / Maps / Drive + Other Bookmarks), history with search & forget-site, download manager with real progress, clear-browsing-data |
| UI | Light theme per `wed1.png` (+ dark mode), new-tab start page with greeting, real weather (open-meteo) & clock, editable shortcuts, find-in-page, zoom with per-site memory, fullscreen, print, save page, DevTools, custom menus, toasts |
| Input | Full keyboard shortcut set, mouse gestures (right-drag back/forward/new tab/reload/close), content-page shortcut forwarding via a validated event bridge |
| Engine extras | Filter-list auto-updater (one click on the privacy dashboard), custom ABP-syntax filters, in-app privacy self-test |

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│ Windows (WebView2 / Chromium)                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ chrome webview: tab strip · toolbar · omnibox · bar  │  │
│  ├──────────────────────────────────────────────────────┤  │
│  │ tab webview 1 … N (multiwebview, hidden per inactivity)│ │
│  └──────────────────────────────────────────────────────┘  │
└──────────────┬─────────────────────────────────────────────┘
               │ Tauri IPC (commands + events)
┌──────────────┴─────────────────────────────────────────────┐
│ kestrel-browser (Rust, src-tauri)                          │
│  tabs.rs      — window + per-tab webview lifecycle/bounds  │
│  intercept.rs — request interceptor: blocking, cookies,    │
│                 HTTPS-only, safe browsing, page bookkeeping│
│  engine_host  — filter lists → adblock engine (background) │
│  bridge.rs    — injected scripts: shortcuts, gestures, find│
│  commands.rs  — typed IPC surface                          │
│  platform/    — ICoreWebView2: back/forward/stop/mute      │
├────────────────────────────────────────────────────────────┤
│ crates/kestrel-privacy — adblock engine wrapper,           │
│                          fingerprint farbling, safe browsing│
│ crates/kestrel-data    — settings/history/bookmarks/       │
│                          downloads/session (JSON, atomic)  │
└────────────────────────────────────────────────────────────┘
```

Every request a tab makes passes through `intercept::handle_request` **before it
leaves the machine** — blocked requests never reach the ad server. Pages get
per-URL cosmetic CSS + scriptlets, farbling init scripts, and a DNT/GPC surface.

## Build

Requirements: Node 20+, Rust (stable), Windows 10/11 (WebView2 is preinstalled).

```bash
npm install
npm run tauri build     # → src-tauri/target/release/kestrel.exe + NSIS installer
```

CI builds both artifacts on every push (see **Actions** → `kestrel-windows-portable`,
`kestrel-windows-installer`).

The Linux GTK4 implementation of Kestrel v0.1 lives on the
[`gtk-linux-legacy`](https://github.com/salim77007j/kestrel-browser/tree/gtk-linux-legacy)
branch.

## Privacy notes & honest limitations

- Third-party cookie control strips **Set-Cookie on third-party responses** (no new
  cross-site cookies). WebView2 marks request headers read-only at the interception
  layer, so already-stored cookies of third-party *requests* rely on Chromium's
  cookie partitioning; use *Clear browsing data* to wipe them.
- "Hard reload" is a normal reload — WebView2 does not expose a cache-bypassing
  reload API.
- Tab muting requires a recent WebView2 Runtime (uses `ICoreWebView2_4::IsMuted`);
  unsupported runtimes show a warning instead of a fake state.
