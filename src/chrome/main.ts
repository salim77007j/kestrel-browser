import { invoke, listen, emit, windowMinimize, windowToggleMaximize, windowClose } from "../lib/api";
import { icons } from "../lib/icons";
import { brandIcon, appsIcon } from "../lib/brandicons";

// ---------------------------------------------------------------------------
// state
// ---------------------------------------------------------------------------

interface TabMeta {
  id: string; url: string; title: string; pinned: boolean; muted: boolean;
  loading: boolean; can_back: boolean; can_forward: boolean; zoom: number;
  group: { id: string; name: string; color: string; collapsed: boolean } | null;
  favicon: string | null;
}
interface BookmarkNode { id: number; title: string; url: string }
interface Settings {
  theme: string; show_bookmarks_bar: boolean; search_engine: string;
  adblock_enabled: boolean; [k: string]: any;
}

const S = {
  tabs: [] as TabMeta[],
  active: null as string | null,
  settings: null as Settings | null,
  bookmarks: { bar: [] as BookmarkNode[], other: [] as BookmarkNode[] },
  favicons: new Map<string, string>(),
  engineRules: 0,
  blockedToday: 0,
  findStatus: "",
};

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector(sel) as T;
const activeTab = () => S.tabs.find((t) => t.id === S.active) || null;

function hostOf(url: string): string {
  try { return new URL(url).hostname; } catch { return ""; }
}

function isInternal(url: string): boolean {
  return url.includes(".localhost") || url.startsWith("tauri://") || url.startsWith("kestrel://") || url === "about:blank" || url === "";
}

function titleFor(t: TabMeta): string {
  if (t.title && t.title !== "about:blank" && t.title !== "New Tab") return t.title;
  if (!t.url || t.url.endsWith("newtab.html")) return "New Tab";
  const h = hostOf(t.url);
  return h || "New Tab";
}

// ---------------------------------------------------------------------------
// theme + layout
// ---------------------------------------------------------------------------

function applyTheme(theme: string) {
  const dark = theme === "dark" || (theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
  $("#chrome").setAttribute("data-theme", dark ? "dark" : "light");
}

function reportHeight() {
  const bar = $("#bookmarksbar");
  const h = bar.classList.contains("hidden-bar") ? $("#tabstrip").offsetHeight + $("#toolbar").offsetHeight : $("#chrome").offsetHeight;
  invoke("set_chrome_height", { height: h }).catch(() => {});
}

// ---------------------------------------------------------------------------
// tab strip
// ---------------------------------------------------------------------------

function renderTabs() {
  const wrap = $("#tabs");
  wrap.innerHTML = "";
  for (const t of S.tabs) {
    const el = document.createElement("div");
    el.className = "tab" + (t.id === S.active ? " active" : "") + (t.pinned ? " pinned" : "");
    el.dataset.id = t.id;
    el.title = `${titleFor(t)}\n${t.url}`;
    const host = hostOf(t.url);
    const fav = t.favicon || S.favicons.get(host);
    const favHtml = t.loading
      ? `<span class="spinner"></span>`
      : fav
        ? `<img src="${fav}" />`
        : `<span class="letter-chip">${(titleFor(t)[0] || "?").toUpperCase()}</span>`;
    const muteHtml = t.muted ? `<span class="tab-mute">${icons.volumeOff}</span>` : "";
    const groupHtml = t.group
      ? `<span class="tab-group-chip" style="background:${t.group.color}" title="${escapeHtml(t.group.name)}"></span>`
      : "";
    el.innerHTML = `
      ${groupHtml}
      <span class="tab-favicon">${favHtml}</span>
      ${muteHtml}
      <span class="tab-title">${escapeHtml(titleFor(t))}</span>
      <button class="tab-close" title="Close tab (Ctrl+W)">${icons.close}</button>
    `;
    el.addEventListener("mousedown", (e) => {
      if (e.button === 1) { e.preventDefault(); invoke("close_tab", { id: t.id }); return; }
      if (e.button === 0 && !(e.target as HTMLElement).closest(".tab-close")) invoke("activate_tab", { id: t.id });
    });
    el.querySelector(".tab-close")!.addEventListener("click", () => invoke("close_tab", { id: t.id }));
    el.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      tabContextMenu(e as MouseEvent, t);
    });
    wrap.appendChild(el);
  }
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

// ---------------------------------------------------------------------------
// toolbar / omnibox
// ---------------------------------------------------------------------------

function renderToolbar() {
  const t = activeTab();
  $("#btn-back").toggleAttribute("disabled", !t);
  $("#btn-forward").toggleAttribute("disabled", !t);
  $("#btn-stop").classList.toggle("hidden", !t?.loading);
  $("#btn-reload").classList.toggle("hidden", !!t?.loading);
  if (!t) return;
  const input = $("#omnibox-input") as HTMLInputElement;
  if (document.activeElement !== input) {
    input.value = isInternal(t.url) ? "" : t.url;
    if (t.url.includes("newtab.html")) input.value = "";
  }
  // security indicator
  const siteIcon = $("#site-icon");
  const https = t.url.startsWith("https://");
  siteIcon.classList.toggle("not-secure", !https && !isInternal(t.url));
  const host = hostOf(t.url);
  if (isInternal(t.url)) {
    siteIcon.innerHTML = `<span class="letter-chip">K</span>`;
  } else if (host) {
    siteIcon.innerHTML = "";
    const fav = S.favicons.get(host) || t.favicon;
    if (fav) siteIcon.innerHTML = `<img src="${fav}" style="width:16px;height:16px;border-radius:2px" />`;
    else siteIcon.innerHTML = `${icons.globe}`;
  }
  // star state
  const marked = [...S.bookmarks.bar, ...S.bookmarks.other].some((b) => b.url === t.url);
  $("#btn-star").classList.toggle("active", marked);
}

function renderShield() {
  const badge = $("#shield-count");
  if (S.settings?.adblock_enabled && S.blockedToday > 0) {
    badge.textContent = S.blockedToday > 999 ? "1k+" : String(S.blockedToday);
    badge.classList.remove("hidden");
  } else {
    badge.classList.add("hidden");
  }
}

// omnibox suggestions
let suggestions: { kind: string; text: string; secondary: string }[] = [];
let suggIndex = -1;

async function refreshSuggestions(q: string) {
  if (!q.trim()) { hideDropdown(); return; }
  try {
    suggestions = await invoke("omnibox_suggest", { query: q });
  } catch { suggestions = []; }
  renderDropdown();
}

function renderDropdown() {
  const dd = $("#omnibox-dropdown");
  if (!suggestions.length) { hideDropdown(); return; }
  dd.innerHTML = "";
  suggestions.forEach((s, i) => {
    const el = document.createElement("div");
    el.className = "sugg" + (i === suggIndex ? " selected" : "");
    const icon = s.kind === "bookmark" ? icons.star : s.kind === "history" ? icons.history : s.kind === "url" ? icons.globe : icons.search;
    el.innerHTML = `
      <span class="sugg-kind">${icon}</span>
      <span class="sugg-main">${escapeHtml(s.text)}</span>
      <span class="sugg-sub">${escapeHtml(s.secondary || kindLabel(s.kind))}</span>`;
    el.addEventListener("mousedown", (e) => { e.preventDefault(); chooseSuggestion(s); });
    dd.appendChild(el);
  });
  dd.classList.remove("hidden");
}

function kindLabel(k: string) {
  return k === "search" ? "Search" : k === "url" ? "Open URL" : k;
}

function chooseSuggestion(s: { text: string; kind: string }) {
  hideDropdown();
  ($("#omnibox-input") as HTMLInputElement).blur();
  if (s.kind === "search") invoke("navigate_active", { input: s.text });
  else invoke("navigate_active", { input: s.text });
}

function hideDropdown() {
  $("#omnibox-dropdown").classList.add("hidden");
  suggestions = [];
  suggIndex = -1;
}

// ---------------------------------------------------------------------------
// bookmarks bar
// ---------------------------------------------------------------------------

function renderBookmarksBar() {
  const bar = $("#bookmarksbar");
  bar.innerHTML = "";
  bar.classList.toggle("hidden-bar", !S.settings?.show_bookmarks_bar);
  reportHeight();

  const apps = document.createElement("button");
  apps.className = "bm-item";
  apps.innerHTML = `<span class="bm-icon">${appsIcon(16)}</span><span class="bm-label">Apps</span>`;
  apps.title = "Open start page";
  apps.addEventListener("click", () => invoke("open_internal", { page: "newtab.html" }));
  bar.appendChild(apps);

  for (const b of S.bookmarks.bar) {
    const el = document.createElement("button");
    el.className = "bm-item";
    const brand = brandIcon(b.url, 16);
    const fav = S.favicons.get(hostOf(b.url));
    const icon = brand
      ? brand
      : fav
        ? `<img src="${fav}" style="width:16px;height:16px" />`
        : letterChipIcon(b.title);
    el.innerHTML = `<span class="bm-icon">${icon}</span><span class="bm-label">${escapeHtml(b.title)}</span>`;
    el.addEventListener("click", () => invoke("navigate_active", { input: b.url }));
    el.addEventListener("auxclick", (e) => {
      if (e.button === 1) invoke("create_tab", { url: b.url, background: true });
    });
    el.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      popupMenu(e as MouseEvent, [
        { label: "Open", icon: icons.open32, click: () => invoke("navigate_active", { input: b.url }) },
        { label: "Open in new tab", icon: icons.newtab, click: () => invoke("create_tab", { url: b.url, background: true }) },
        { sep: true },
        { label: "Edit name…", icon: icons.settings, click: () => promptMenu(e as MouseEvent, "Rename bookmark", b.title, (v) => invoke("rename_bookmark", { id: b.id, title: v })) },
        { label: "Remove", icon: icons.trash, danger: true, click: () => invoke("remove_bookmark", { id: b.id }) },
      ]);
    });
    bar.appendChild(el);
  }

  bar.appendChild(Object.assign(document.createElement("span"), { className: "bm-spacer" }));

  const other = document.createElement("button");
  other.className = "bm-item";
  other.innerHTML = `<span class="bm-icon">${icons.folder}</span><span class="bm-label">Other Bookmarks</span>`;
  other.addEventListener("click", (e) => {
    const items = S.bookmarks.other.map((b) => ({
      label: b.title || b.url, icon: icons.bookmark,
      click: () => invoke("navigate_active", { input: b.url }),
    }));
    if (!items.length) items.push({ label: "(empty — use ⭐ to add here from bookmarks page)", icon: icons.folder, click: () => invoke("navigate_tab", { id: S.active, url: "pages/bookmarks.html" }).catch(() => {}) });
    popupMenu(e as MouseEvent, items as any);
  });
  bar.appendChild(other);
}

function letterChipIcon(name: string): string {
  return `<span class="letter-chip">${(name[0] || "?").toUpperCase()}</span>`;
}

// ---------------------------------------------------------------------------
// popup menus (generic)
// ---------------------------------------------------------------------------

type MenuItem = { label?: string; icon?: string; danger?: boolean; kbd?: string; click?: () => void; sep?: boolean; header?: string };

function popupMenu(ev: MouseEvent, items: MenuItem[]) {
  closePopup();
  const layer = $("#popup-layer");
  layer.classList.remove("hidden");
  const menu = document.createElement("div");
  menu.className = "popup-menu";
  for (const it of items) {
    if (it.sep) { menu.appendChild(Object.assign(document.createElement("div"), { className: "menu-sep" })); continue; }
    if (it.header) { const h = document.createElement("div"); h.className = "menu-header"; h.textContent = it.header; menu.appendChild(h); continue; }
    const el = document.createElement("div");
    el.className = "menu-item" + (it.danger ? " danger" : "");
    el.innerHTML = `${it.icon ? `<span class="menu-icon">${it.icon}</span>` : "<span class='menu-icon'></span>"}<span>${escapeHtml(it.label || "")}</span>${it.kbd ? `<kbd>${it.kbd}</kbd>` : ""}`;
    el.addEventListener("click", () => { closePopup(); it.click?.(); });
    menu.appendChild(el);
  }
  positionMenu(menu, ev);
  layer.appendChild(menu);
  layer.addEventListener("mousedown", closePopup, { once: true });
}

function promptMenu(ev: MouseEvent, title: string, initial: string, submit: (v: string) => void) {
  closePopup();
  const layer = $("#popup-layer");
  layer.classList.remove("hidden");
  const menu = document.createElement("div");
  menu.className = "popup-menu";
  menu.style.minWidth = "300px";
  const h = document.createElement("div");
  h.className = "menu-header"; h.textContent = title;
  const input = document.createElement("input");
  input.className = "menu-input";
  input.value = initial;
  const go = () => { const v = input.value.trim(); if (v) { closePopup(); submit(v); } };
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") go(); });
  menu.appendChild(h); menu.appendChild(input);
  positionMenu(menu, ev);
  layer.appendChild(menu);
  setTimeout(() => input.focus(), 30);
  layer.addEventListener("mousedown", closePopup, { once: true });
}

function positionMenu(menu: HTMLElement, ev: MouseEvent) {
  const x = Math.min(ev.clientX, innerWidth - 280);
  const y = Math.min(ev.clientY, innerHeight - (menu.offsetHeight || 320) - 12);
  menu.style.left = `${Math.max(8, x)}px`;
  menu.style.top = `${Math.max(8, y)}px`;
}

function closePopup() {
  const layer = $("#popup-layer");
  layer.classList.add("hidden");
  layer.innerHTML = "";
}

// ---------------------------------------------------------------------------
// menus
// ---------------------------------------------------------------------------

function mainMenu(ev: MouseEvent) {
  popupMenu(ev, [
    { header: "Kestrel" },
    { label: "New tab", icon: icons.newtab, kbd: "Ctrl+T", click: () => invoke("create_tab", {}) } as any,
    { label: "Bookmark this tab", icon: icons.star, kbd: "Ctrl+D", click: () => invoke("toggle_bookmark_current") },
    { sep: true },
    { label: "History", icon: icons.history, kbd: "Ctrl+H", click: () => openInternal("history.html") },
    { label: "Bookmarks", icon: icons.bookmark, click: () => openInternal("bookmarks.html") },
    { label: "Downloads", icon: icons.download, kbd: "Ctrl+J", click: () => openInternal("downloads.html") },
    { label: "Privacy dashboard", icon: icons.shield, kbd: "Ctrl+Shift+P", click: () => openInternal("privacy.html") },
    { label: "Settings", icon: icons.settings, click: () => openInternal("settings.html") },
    { sep: true },
    { label: "Print…", icon: icons.print, kbd: "Ctrl+P", click: () => invoke("print_page") },
    { label: "Save page…", icon: icons.save, kbd: "Ctrl+S", click: () => invoke("save_page") },
    { label: "Find in page…", icon: icons.search, kbd: "Ctrl+F", click: () => openFind() },
    { label: "Developer tools", icon: icons.devtools, kbd: "F12", click: () => invoke("open_devtools") },
    { label: "Fullscreen", icon: icons.fullscreen, kbd: "F11", click: () => toggleFullscreen() },
    { sep: true },
    { label: "About Kestrel", icon: icons.shield, click: () => openInternal("about.html") },
    { label: "Quit", icon: icons.close, kbd: "Ctrl+Q", click: () => invoke("force_quit") },
  ]);
}

function tabContextMenu(ev: MouseEvent, t: TabMeta) {
  const items: MenuItem[] = [
    { label: t.pinned ? "Unpin tab" : "Pin tab", icon: icons.pin, click: () => invoke("pin_tab", { id: t.id, pinned: !t.pinned }) },
    { label: t.muted ? "Unmute site" : "Mute site", icon: t.muted ? icons.volume : icons.volumeOff, click: () => invoke("mute_tab", { id: t.id, muted: !t.muted }) },
    { label: "Duplicate", icon: icons.duplicate, click: () => invoke("duplicate_tab", { id: t.id }) },
    { sep: true },
    { label: "Add to group…", icon: icons.group, click: () => groupDialog(ev, t) },
  ];
  if (t.group) items.push({ label: "Remove from group", icon: icons.close, click: () => invoke("ungroup_tab", { id: t.id }) });
  items.push(
    { sep: true },
    { label: "Close", icon: icons.close, kbd: "Ctrl+W", click: () => invoke("close_tab", { id: t.id }) },
    { label: "Close other tabs", icon: icons.trash, click: () => invoke("close_other_tabs", { id: t.id }) },
  );
  popupMenu(ev, items);
}

const GROUP_COLORS = ["#1a73e8", "#d93025", "#188038", "#e37400", "#9334e6", "#12b5cb"];
let selectedForGroup: string[] = [];

function groupDialog(ev: MouseEvent, t: TabMeta) {
  closePopup();
  const layer = $("#popup-layer");
  layer.classList.remove("hidden");
  const menu = document.createElement("div");
  menu.className = "popup-menu";
  menu.style.minWidth = "280px";
  menu.innerHTML = `<div class="menu-header">New tab group</div>`;
  const input = document.createElement("input");
  input.className = "menu-input";
  input.placeholder = "Group name";
  menu.appendChild(input);
  const colors = document.createElement("div");
  colors.className = "menu-colors";
  let color = GROUP_COLORS[0];
  GROUP_COLORS.forEach((c, i) => {
    const dot = document.createElement("div");
    dot.className = "color-dot";
    dot.style.background = c;
    if (i === 0) dot.style.borderColor = "var(--text)";
    dot.addEventListener("click", () => {
      color = c;
      colors.querySelectorAll(".color-dot").forEach((d) => ((d as HTMLElement).style.borderColor = "transparent"));
      dot.style.borderColor = "var(--text)";
    });
    colors.appendChild(dot);
  });
  menu.appendChild(colors);
  const create = document.createElement("div");
  create.className = "menu-item";
  create.innerHTML = `<span class="menu-icon">${icons.group}</span><span>Create group with this tab</span>`;
  create.addEventListener("click", () => {
    closePopup();
    invoke("group_tabs", { ids: [t.id], name: input.value.trim() || "Group", color });
  });
  menu.appendChild(create);
  positionMenu(menu, ev);
  layer.appendChild(menu);
  setTimeout(() => input.focus(), 30);
  layer.addEventListener("mousedown", closePopup, { once: true });
  void selectedForGroup;
}

function openInternal(page: string) {
  invoke("open_internal", { page }).catch(() => invoke("create_tab", { url: null }));
}

async function toggleFullscreen() {
  try { await invoke("set_fullscreen", {}); } catch { /* window fullscreen handled by the OS layer */ }
}

// ---------------------------------------------------------------------------
// find bar
// ---------------------------------------------------------------------------

function openFind() {
  $("#findbar").classList.remove("hidden");
  ($("#find-input") as HTMLInputElement).focus();
  ($("#find-input") as HTMLInputElement).select();
}
function closeFind() {
  $("#findbar").classList.add("hidden");
  invoke("find_exit");
}

// ---------------------------------------------------------------------------
// shortcuts
// ---------------------------------------------------------------------------

function dispatchShortcut(e: KeyboardEvent): boolean {
  const mod = e.ctrlKey || e.metaKey;
  const k = e.key.toLowerCase();
  if (e.key === "F12") { invoke("open_devtools"); return true; }
  if (e.key === "F11") { toggleFullscreen(); return true; }
  if (!mod) return false;
  if (e.shiftKey) {
    switch (k) {
      case "t": invoke("reopen_closed_tab"); return true;
      case "n": invoke("create_tab", {}); return true;
      case "p": openInternal("privacy.html"); return true;
      case "r": invoke("reload_active", {}); return true;
      case "delete": invoke("clear_browsing_data", { history: true, cookies: false, cache: false, downloadsList: false }); toast("History cleared"); return true;
    }
    return false;
  }
  if (e.altKey) {
    if (k === "t") { openInternal("settings.html"); return true; }
  }
  switch (k) {
    case "t": invoke("create_tab", {}); return true;
    case "w": if (S.active) invoke("close_tab", { id: S.active }); return true;
    case "l": ($("#omnibox-input") as HTMLInputElement).focus(); ($("#omnibox-input") as HTMLInputElement).select(); return true;
    case "d": invoke("toggle_bookmark_current").then((added: any) => toast(added ? "Bookmark added" : "Bookmark removed")); return true;
    case "f": openFind(); return true;
    case "r": invoke("reload_active", {}); return true;
    case "h": openInternal("history.html"); return true;
    case "j": openInternal("downloads.html"); return true;
    case "p": invoke("print_page"); return true;
    case "s": invoke("save_page"); return true;
    case "q": invoke("force_quit"); return true;
    case "+": case "=": zoomStep(0.1); return true;
    case "-": zoomStep(-0.1); return true;
    case "0": invoke("set_zoom", { level: 1 }); return true;
  }
  if (e.key >= "1" && e.key <= "9") {
    const idx = e.key === "9" ? S.tabs.length - 1 : Number(e.key) - 1;
    const t = S.tabs[idx];
    if (t) invoke("activate_tab", { id: t.id });
    return true;
  }
  return false;
}

function zoomStep(delta: number) {
  const t = activeTab();
  if (!t) return;
  invoke("set_zoom", { level: Math.min(5, Math.max(0.25, (t.zoom || 1) + delta)) });
}

// page-focused keys come through the validated page-event bridge
async function handlePageKey(e: any) {
  const ev = new KeyboardEvent("keydown", {
    key: e.code, ctrlKey: e.ctrl, shiftKey: e.shift, altKey: e.alt, metaKey: e.meta,
  });
  if (!dispatchShortcut(ev)) {
    // Escape / arrows for find navigation while find is open
    if (e.code === "Escape" && !$("#findbar").classList.contains("hidden")) closeFind();
  }
}

// ---------------------------------------------------------------------------
// toasts
// ---------------------------------------------------------------------------

function toast(text: string, level: "info" | "warn" | "error" = "info") {
  const el = document.createElement("div");
  el.className = `toast ${level}`;
  el.textContent = text;
  $("#toasts").appendChild(el);
  setTimeout(() => el.remove(), 4000);
}

// ---------------------------------------------------------------------------
// boot
// ---------------------------------------------------------------------------

async function boot() {
  const init = await invoke("init_ui");
  S.tabs = init.tabs || [];
  S.active = init.active;
  S.settings = init.settings;
  S.engineRules = init.engineRules || 0;
  S.blockedToday = init.stats?.blockedToday || 0;
  applyTheme(S.settings?.theme || "system");
  $("#bookmarksbar").classList.toggle("hidden-bar", !S.settings?.show_bookmarks_bar);

  const bm = await invoke("list_bookmarks");
  S.bookmarks = bm;

  renderTabs();
  renderToolbar();
  renderBookmarksBar();
  renderShield();
  setTimeout(reportHeight, 60);

  wireChrome();
  await wireEvents();
}

function wireChrome() {
  // window controls
  $("#win-min").addEventListener("click", windowMinimize);
  $("#win-max").addEventListener("click", windowToggleMaximize);
  $("#win-close").addEventListener("click", windowClose);
  $("#tabstrip").setAttribute("data-tauri-drag-region", "");
  $("#tabs").setAttribute("data-tauri-drag-region", "");

  $("#newtab-btn").addEventListener("click", () => invoke("create_tab", {}));
  $("#btn-back").addEventListener("click", () => invoke("nav_back").catch(() => {}));
  $("#btn-forward").addEventListener("click", () => invoke("nav_forward").catch(() => {}));
  $("#btn-reload").addEventListener("click", () => invoke("reload_active", {}));
  $("#btn-stop").addEventListener("click", () => invoke("stop_active"));
  $("#btn-menu").addEventListener("click", (e) => mainMenu(e as MouseEvent));
  $("#btn-shield").addEventListener("click", () => openInternal("privacy.html"));
  $("#btn-star").addEventListener("click", () => invoke("toggle_bookmark_current").then((added: any) => {
    toast(added ? "Bookmark added" : "Bookmark removed");
    refreshBookmarks();
  }));

  const input = $("#omnibox-input") as HTMLInputElement;
  input.addEventListener("focus", () => { $("#omnibox").classList.add("focused"); input.select(); });
  input.addEventListener("blur", () => { $("#omnibox").classList.remove("focused"); setTimeout(hideDropdown, 140); });
  input.addEventListener("input", () => refreshSuggestions(input.value));
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      const chosen = suggIndex >= 0 ? suggestions[suggIndex] : null;
      hideDropdown();
      input.blur();
      invoke("navigate_active", { input: chosen ? chosen.text : input.value });
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (suggestions.length) { suggIndex = (suggIndex + 1) % suggestions.length; renderDropdown(); }
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (suggestions.length) { suggIndex = suggIndex <= 0 ? suggestions.length - 1 : suggIndex - 1; renderDropdown(); }
    } else if (e.key === "Escape") {
      hideDropdown();
      input.blur();
      invoke("stop_active");
    }
  });

  // find bar
  $("#find-close").addEventListener("click", closeFind);
  $("#find-next").addEventListener("click", () => invoke("find_step", { dir: 1 }));
  $("#find-prev").addEventListener("click", () => invoke("find_step", { dir: -1 }));
  $("#find-input").addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      invoke(e.shiftKey ? "find_step" : "find_in_page", e.shiftKey ? { dir: -1 } : { query: ($("#find-input") as HTMLInputElement).value });
    } else if (e.key === "Escape") closeFind();
  });

  // chrome-level shortcuts
  window.addEventListener("keydown", (e) => {
    if (dispatchShortcut(e)) { e.preventDefault(); e.stopPropagation(); }
    else if (e.key === "Escape" && !$("#findbar").classList.contains("hidden")) closeFind();
  }, { capture: true });

  window.addEventListener("resize", () => reportHeight());
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => applyTheme(S.settings?.theme || "system"));
}

async function refreshBookmarks() {
  S.bookmarks = await invoke("list_bookmarks");
  renderBookmarksBar();
  renderToolbar();
}

async function wireEvents() {
  await listen("tabs", (p) => {
    S.tabs = p.tabs || [];
    S.active = p.active;
    renderTabs();
    renderToolbar();
  });
  await listen("page-key", handlePageKey);
  await listen("favicon", (p) => {
    if (p?.host && p?.data) {
      S.favicons.set(p.host, p.data);
      renderTabs();
      renderToolbar();
      renderBookmarksBar();
    }
  });
  await listen("bookmarks-changed", refreshBookmarks);
  await listen("shortcuts-changed", () => {});
  await listen("adblock-stats", (p) => { S.blockedToday = p.blockedToday || 0; renderShield(); });
  await listen("engine-ready", (p) => { S.engineRules = p.rules || 0; });
  await listen("find-result", (p) => {
    $("#find-status").textContent = p.count ? `${p.index}/${p.count}` : "0/0";
  });
  await listen("toast", (p) => toast(p.text || String(p), p.level || "info"));
  await listen("confirm-quit", () => {
    const ok = confirm("Close all tabs and quit Kestrel?");
    if (ok) invoke("force_quit");
  });
  await listen("layout-changed", async () => {
    const s = await invoke("get_settings");
    S.settings = s;
    applyTheme(s.theme || "system");
    renderBookmarksBar();
    renderTabs();
  });
  await listen("downloads-changed", () => {});
}

boot().catch((e) => {
  document.body.innerHTML = `<pre style="padding:20px;color:#d93025">${String(e)}</pre>`;
});
