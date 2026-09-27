import { invoke, $, $$, escapeHtml } from "./shared";

interface Settings { [k: string]: any }
let S: Settings = {};

const SECTIONS: Record<string, () => string> = {
  appearance: () => `
    <h1>Appearance</h1>
    <div class="settings-section">
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Theme</div><div class="row-sub">Light, dark, or follow your system.</div></div>
        <select class="kestrel-select" id="theme">
          <option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option>
        </select>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Show bookmarks bar</div><div class="row-sub">The bar under the address box (Apps, Google, YouTube…).</div></div>
        <label class="switch"><input type="checkbox" id="showBookmarksBar"><span class="slider"></span></label>
      </div>
    </div>`,

  startup: () => `
    <h1>On startup</h1>
    <div class="settings-section">
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Behavior</div><div class="row-sub">What Kestrel opens when it starts.</div></div>
        <select class="kestrel-select" id="startup">
          <option value="continue">Continue where you left off</option>
          <option value="newtab">Open the start page</option>
        </select>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Restore after a crash</div><div class="row-sub">Reopen your tabs if Kestrel closed unexpectedly.</div></div>
        <label class="switch"><input type="checkbox" id="restoreOnCrash"><span class="slider"></span></label>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Ask before closing multiple tabs</div><div class="row-sub">Confirm when quitting with several tabs open.</div></div>
        <label class="switch"><input type="checkbox" id="confirmCloseMultiple"><span class="slider"></span></label>
      </div>
    </div>`,

  search: () => `
    <h1>Search engine</h1>
    <div class="settings-section">
      <div class="row-text"></div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Engine used in the address box</div><div class="row-sub">DuckDuckGo is the privacy-friendly default.</div></div>
        <select class="kestrel-select" id="searchEngine">
          <option value="duckduckgo">DuckDuckGo</option>
          <option value="google">Google</option>
          <option value="bing">Bing</option>
          <option value="brave">Brave Search</option>
        </select>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Search suggestions</div><div class="row-sub">Send what you type to the engine's suggestion service.</div></div>
        <label class="switch"><input type="checkbox" id="searchSuggestions"><span class="slider"></span></label>
      </div>
    </div>`,

  privacy: () => {
    const fp = S.fingerprint || {};
    return `
    <h1>Privacy &amp; security</h1>
    <div class="settings-section">
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Block ads &amp; trackers</div><div class="row-sub">Network-level filtering with Brave's adblock engine + cosmetic hiding.</div></div>
        <label class="switch"><input type="checkbox" id="adblockEnabled"><span class="slider"></span></label>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Filter lists</div><div class="row-sub">EasyList, EasyPrivacy, Fanboy Annoyance, Peter Lowe's list. Bundled offline; use the Privacy dashboard to update them.</div></div>
        <div style="display:flex;flex-direction:column;gap:8px">
          ${["easylist", "easyprivacy", "fanboy_annoyance", "peter_lowe"].map((k) => `
            <label style="display:flex;align-items:center;gap:8px;font-size:13px;justify-content:flex-end">
              ${k.replace("_", " ")} <label class="switch"><input type="checkbox" data-list="${k}"><span class="slider"></span></label>
            </label>`).join("")}
        </div>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Custom filters</div><div class="row-sub">One ABP-syntax rule per line (e.g. ||ads.example.com^).</div></div>
        <div style="width:100%;margin-top:10px">
          <textarea class="filters-area" id="customFilters" placeholder="||annoying-banners.example^"></textarea>
          <div style="display:flex;gap:8px;margin-top:8px;justify-content:flex-end">
            <button class="kbtn secondary" id="saveFilters">Apply filters</button>
          </div>
        </div>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Cookie policy</div><div class="row-sub">Third-party responses get their Set-Cookie stripped (no new cross-site cookies).</div></div>
        <select class="kestrel-select" id="cookiePolicy">
          <option value="blockthirdparty">Block third-party cookies</option>
          <option value="allowall">Allow all cookies</option>
        </select>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">HTTPS-only mode</div><div class="row-sub">Automatically upgrade http:// pages to https://.</div></div>
        <label class="switch"><input type="checkbox" id="httpsOnly"><span class="slider"></span></label>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Safe browsing</div><div class="row-sub">URLhaus malware blocklist + phishing-lookalike warnings.</div></div>
        <label class="switch"><input type="checkbox" id="safeBrowsing"><span class="slider"></span></label>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Send "Do Not Track"</div><div class="row-sub">Expose navigator.doNotTrack to sites (header control is not available in WebView2).</div></div>
        <label class="switch"><input type="checkbox" id="doNotTrack"><span class="slider"></span></label>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Global Privacy Control</div><div class="row-sub">Expose navigator.globalPrivacyControl (GPC) to sites.</div></div>
        <label class="switch"><input type="checkbox" id="globalPrivacyControl"><span class="slider"></span></label>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Fingerprint protection</div><div class="row-sub">Per-site noise and normalization applied before page scripts run.</div></div>
        <label class="switch"><input type="checkbox" id="fpEnabled"><span class="slider"></span></label>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Shields detail</div><div class="row-sub">Which surfaces are farbled / normalized.</div></div>
        <div style="display:flex;flex-direction:column;gap:8px">
          ${["canvas", "audio", "webgl", "fonts", "hardware"].map((k) => `
            <label style="display:flex;align-items:center;gap:8px;font-size:13px;justify-content:flex-end">
              ${k} <label class="switch"><input type="checkbox" data-fp="${k}"><span class="slider"></span></label>
            </label>`).join("")}
        </div>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Site permissions</div><div class="row-sub">Remembered allow/deny decisions for camera, microphone, location…</div></div>
        <div style="text-align:right">
          <button class="kbtn secondary" id="viewPerms">View</button>
          <button class="kbtn secondary" id="clearPerms">Clear all</button>
          <div id="permsBox" style="margin-top:10px;text-align:left;display:none"></div>
        </div>
      </div>
    </div>`;
  },

  downloads: () => `
    <h1>Downloads</h1>
    <div class="settings-section">
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Location</div><div class="row-sub" id="dlDir">${escapeHtml(S.download_dir || "Default Downloads folder")}</div></div>
        <button class="kbtn secondary" id="pickDir">Change</button>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Ask where to save each file</div><div class="row-sub">Shows a Save-As dialog for every download.</div></div>
        <label class="switch"><input type="checkbox" id="downloadAskEachTime"><span class="slider"></span></label>
      </div>
    </div>`,

  advanced: () => `
    <h1>Advanced</h1>
    <div class="settings-section">
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Clear browsing data</div><div class="row-sub">History, cookies and the favicon cache.</div></div>
        <button class="kbtn danger" id="clearAll">Clear now</button>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Keyboard shortcuts</div><div class="row-sub">Ctrl+T new tab · Ctrl+W close · Ctrl+L address · Ctrl+F find · Ctrl+H history · Ctrl+J downloads · Ctrl+D bookmark · Ctrl+Shift+T reopen · F11 fullscreen · F12 devtools · Ctrl+mouse wheel zoom</div></div>
      </div>
      <div class="settings-row">
        <div class="row-text"><div class="row-title">Mouse gestures</div><div class="row-sub">Right-drag: left = back, right = forward, up = new tab, down = reload, up-right = close.</div></div>
        <label class="switch"><input type="checkbox" id="gesturesEnabled"><span class="slider"></span></label>
      </div>
    </div>`,
};

async function save(patch: Record<string, any>) {
  S = await invoke("update_settings", { patch });
}

function bind(id: string, key: string, transform?: (v: any) => any) {
  const el = document.getElementById(id);
  if (!el) return;
  const read = () =>
    el instanceof HTMLInputElement
      ? el.type === "checkbox" ? el.checked : el.value
      : (el as HTMLSelectElement).value;
  if (el instanceof HTMLInputElement && el.type === "checkbox") el.checked = !!S[key];
  else (el as any).value = S[key];
  el.addEventListener("change", async () => {
    const v = read();
    await save({ [key]: transform ? transform(v) : v });
    if (key === "theme" || key === "showBookmarksBar") applyAll();
  });
}

function applyAll() {
  const theme = S.theme || "system";
  const dark = theme === "dark" || (theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.body.setAttribute("data-theme", dark ? "dark" : "light");
}

async function showSection(sec: string) {
  $$(".nav-item").forEach((n) => n.classList.toggle("active", n.dataset.sec === sec));
  const main = $("#main");
  main.innerHTML = SECTIONS[sec]();

  if (sec === "appearance") {
    bind("theme", "theme");
    bind("showBookmarksBar", "show_bookmarks_bar");
  } else if (sec === "startup") {
    bind("startup", "startup");
    bind("restoreOnCrash", "restore_on_crash");
    bind("confirmCloseMultiple", "confirm_close_multiple");
  } else if (sec === "search") {
    bind("searchEngine", "search_engine");
    bind("searchSuggestions", "search_suggestions");
  } else if (sec === "privacy") {
    bind("adblockEnabled", "adblock_enabled");
    bind("httpsOnly", "https_only");
    bind("safeBrowsing", "safe_browsing");
    bind("doNotTrack", "do_not_track");
    bind("globalPrivacyControl", "global_privacy_control");
    bind("fpEnabled", "fingerprint", (v) => ({ ...S.fingerprint, enabled: v }));
    bind("cookiePolicy", "cookie_policy");
    document.querySelectorAll<HTMLInputElement>("input[data-list]").forEach((el) => {
      el.checked = !!S.filter_lists?.[el.dataset.list!];
      el.addEventListener("change", async () => {
        const fl = { ...S.filter_lists, [el.dataset.list!]: el.checked };
        S.filter_lists = fl;
        await save({ filterLists: fl });
      });
    });
    document.querySelectorAll<HTMLInputElement>("input[data-fp]").forEach((el) => {
      el.checked = !!S.fingerprint?.[el.dataset.fp!];
      el.addEventListener("change", async () => {
        const fp = { ...S.fingerprint, [el.dataset.fp!]: el.checked };
        S.fingerprint = fp;
        await save({ fingerprint: fp });
      });
    });
    const ta = document.getElementById("customFilters") as HTMLTextAreaElement;
    ta.value = (S.custom_filters || []).join("\n");
    document.getElementById("saveFilters")?.addEventListener("click", async () => {
      const rules = ta.value.split("\n").map((l) => l.trim()).filter(Boolean);
      await save({ customFilters: rules });
      ta.value = rules.join("\n");
    });
    document.getElementById("clearPerms")?.addEventListener("click", async () => {
      await invoke("clear_permissions");
      viewPerms();
    });
    document.getElementById("viewPerms")?.addEventListener("click", viewPerms);
  } else if (sec === "downloads") {
    bind("downloadAskEachTime", "download_ask_each_time");
    document.getElementById("pickDir")?.addEventListener("click", async () => {
      // folder pick through the dialog via rfd happens on the Rust side for
      // downloads; here we ask for a path through prompt (honest, simple)
      const v = prompt("Downloads folder:", S.download_dir || "");
      if (v !== null) {
        await save({ downloadDir: v });
        (document.getElementById("dlDir") as HTMLElement).textContent = v || "Default Downloads folder";
      }
    });
  } else if (sec === "advanced") {
    bind("gesturesEnabled", "gestures_enabled");
    document.getElementById("clearAll")?.addEventListener("click", async () => {
      if (!confirm("Clear history, cookies and cached icons?")) return;
      const r = await invoke("clear_browsing_data", { history: true, cookies: true, cache: true, downloadsList: false });
      alert("Cleared: " + JSON.stringify(r));
    });
  }
}

async function viewPerms() {
  const box = document.getElementById("permsBox")!;
  box.style.display = "block";
  const perms = await invoke("list_permissions");
  const entries = Object.entries(perms as Record<string, any>);
  box.innerHTML = entries.length
    ? entries.map(([origin, kinds]) => `
        <div style="padding:6px 0;border-bottom:1px solid var(--border)">
          <b style="font-size:13px">${escapeHtml(origin)}</b>
          ${Object.entries(kinds as Record<string, string>).map(([k, v]) => `<span class="badge" style="margin-left:6px">${k}: ${v}</span>`).join("")}
        </div>`).join("")
    : `<div style="color:var(--text-2);font-size:13px">No remembered decisions. Sites are denied by default; decisions you record appear here.</div>`;
}

async function boot() {
  S = await invoke("get_settings");
  applyAll();
  $$(".nav-item").forEach((n) =>
    n.addEventListener("click", () => showSection(n.dataset.sec!))
  );
  await showSection("appearance");
}

boot().catch((e) => document.body.insertAdjacentHTML("beforeend", `<pre>${String(e)}</pre>`));
