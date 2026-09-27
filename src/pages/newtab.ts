import { invoke, $, escapeHtml, hostOf, applyThemeFromSettings } from "./shared";

interface Shortcut { title: string; url: string }

const DEFAULT_LOGOS: Record<string, string> = {
  "google.com": `<svg viewBox="0 0 48 48" width="30" height="30"><path fill="#FFC107" d="M43.6 20.1H42V20H24v8h11.3C33.7 32.7 29.2 36 24 36c-6.6 0-12-5.4-12-12s5.4-12 12-12c3.1 0 5.9 1.2 8 3l5.7-5.7C34.3 6.1 29.4 4 24 4 13 4 4 13 4 24s9 20 20 20 20-9 20-20c0-1.3-.1-2.6-.4-3.9z"/><path fill="#FF3D00" d="m6.3 14.7 6.6 4.8C14.7 15.1 19 12 24 12c3.1 0 5.9 1.2 8 3l5.7-5.7C34.3 6.1 29.4 4 24 4 16.3 4 9.7 8.3 6.3 14.7z"/><path fill="#4CAF50" d="M24 44c5.2 0 9.9-2 13.4-5.2l-6.2-5.2C29.2 35.1 26.7 36 24 36c-5.2 0-9.6-3.3-11.3-8l-6.5 5C9.5 39.6 16.2 44 24 44z"/><path fill="#1976D2" d="M43.6 20.1H42V20H24v8h11.3a12 12 0 0 1-4.1 5.6l6.2 5.2C41 35.4 44 30.2 44 24c0-1.3-.1-2.6-.4-3.9z"/></svg>`,
  "youtube.com": `<svg viewBox="0 0 48 48" width="30" height="30"><path fill="#FF0000" d="M45 15.1a5.7 5.7 0 0 0-4-4C37.3 10 24 10 24 10s-13.3 0-17 1.1a5.7 5.7 0 0 0-4 4A59.7 59.7 0 0 0 2 24c0 3 .3 6 1 8.9a5.7 5.7 0 0 0 4 4C10.7 38 24 38 24 38s13.3 0 17-1.1a5.7 5.7 0 0 0 4-4c.7-2.9 1-5.9 1-8.9s-.3-6-1-8.9zM19.5 30.5v-13l11 6.5-11 6.5z"/></svg>`,
  "mail.google.com": `<svg viewBox="0 0 48 48" width="30" height="30"><path fill="#4caf50" d="M45 16.2 40 12l-16 12L8 12l-5 4.2V20l17 12.7L45 20z" opacity=".9"/><path fill="#1e88e5" d="M3 20v16a2 2 0 0 0 2 2h4V16.7L3 13.9z"/><path fill="#e53935" d="M39 38h4a2 2 0 0 0 2-2V20l-6 4.2z"/><path fill="#c62828" d="m35 16.7-11 8.2L13 16.7 8 12l22 16.5L45 12z"/></svg>`,
  "drive.google.com": `<svg viewBox="0 0 87.3 78" width="30" height="30"><path fill="#0066da" d="m6.6 66.85 3.85 6.65c.8 1.4 1.95 2.5 3.3 3.3L27.5 53H0c0 1.55.4 3.1 1.2 4.5z"/><path fill="#00ac47" d="M43.65 25 29.9 1.2c-1.35.8-2.5 1.9-3.3 3.3l-25.4 44A9.06 9.06 0 0 0 0 53h27.5z"/><path fill="#ea4335" d="M73.55 76.8c1.35-.8 2.5-1.9 3.3-3.3l1.6-2.75L86.1 57.5c.8-1.4 1.2-2.95 1.2-4.5H59.8l5.85 11.5z"/><path fill="#00832d" d="m43.65 25 13.75-23.8c-1.35-.8-2.9-1.2-4.5-1.2H34.4c-1.6 0-3.15.45-4.5 1.2z"/><path fill="#2684fc" d="M59.8 53H27.5L13.75 76.8c1.35.8 2.9 1.2 4.5 1.2h50.8c1.6 0 3.15-.45 4.5-1.2z"/><path fill="#ffba00" d="m73.4 26.5-12.7-22c-.8-1.4-1.95-2.5-3.3-3.3L43.65 25 59.8 53h27.45c0-1.55-.4-3.1-1.2-4.5z"/></svg>`,
};

function logoFor(url: string, title: string): string {
  const host = hostOf(url);
  const key = Object.keys(DEFAULT_LOGOS).find((k) => host === k || host.endsWith("." + k));
  if (key) return DEFAULT_LOGOS[key];
  const letter = (title || host)[0]?.toUpperCase() || "?";
  return `<span style="font-size:26px;font-weight:600;color:var(--accent)">${letter}</span>`;
}

async function boot() {
  await applyThemeFromSettings();

  // greeting by hour
  const h = new Date().getHours();
  $("#greeting").textContent =
    h < 12 ? "Good morning!" : h < 18 ? "Good afternoon!" : "Good evening!";

  // clock
  const tick = () => {
    const now = new Date();
    $("#clock-time").textContent = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    $("#clock-date").textContent = now.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric", year: "numeric" });
  };
  tick();
  setInterval(tick, 1000);

  // weather (real; open-meteo via shell)
  invoke("get_weather").then((w) => {
    if (w?.available) {
      $("#w-temp").textContent = `${w.tempC}°C`;
      $("#w-cond").textContent = w.condition || "";
      $("#weather").title = w.city ? `${w.city} — ${w.condition}` : $("#weather").title;
    } else {
      $("#w-temp").textContent = "--°C";
      $("#w-cond").textContent = "Weather unavailable";
    }
  }).catch(() => { $("#w-cond").textContent = "Weather unavailable"; });

  // shortcuts
  const scs = await invoke("get_shortcuts");
  renderShortcuts(scs);

  // search box
  $("#searchform").addEventListener("submit", (e) => {
    e.preventDefault();
    const q = ($("#q") as HTMLInputElement).value.trim();
    if (q) invoke("navigate_active", { input: q });
  });
  $("#privacy-quick").addEventListener("click", () => invoke("open_internal", { page: "privacy.html" }).catch(() => {}));
  $("#open-settings").addEventListener("click", () => invoke("open_internal", { page: "settings.html" }).catch(() => {}));
}

function renderShortcuts(scs: Shortcut[]) {
  const wrap = $("#shortcuts");
  wrap.innerHTML = "";
  for (const sc of scs) {
    const el = document.createElement("button");
    el.className = "shortcut";
    el.innerHTML = `<span class="sc-circle">${logoFor(sc.url, sc.title)}</span><span class="sc-label">${escapeHtml(sc.title)}</span>`;
    el.addEventListener("click", () => invoke("navigate_active", { input: sc.url }));
    el.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      if (confirm(`Remove "${sc.title}" from shortcuts?`)) {
        invoke("remove_shortcut", { url: sc.url }).then(() => invoke("get_shortcuts").then(renderShortcuts));
      }
    });
    wrap.appendChild(el);
  }
  // add shortcut tile
  const add = document.createElement("button");
  add.className = "shortcut";
  add.innerHTML = `<span class="sc-circle"><svg viewBox="0 0 24 24" fill="#5f6368" style="width:26px;height:26px"><path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"/></svg></span><span class="sc-label">Add shortcut</span>`;
  add.addEventListener("click", async () => {
    const name = prompt("Shortcut name:");
    if (!name) return;
    let url = prompt("URL (e.g. example.com):");
    if (!url) return;
    if (!url.startsWith("http")) url = "https://" + url;
    await invoke("add_shortcut", { title: name, url });
    invoke("get_shortcuts").then(renderShortcuts);
  });
  wrap.appendChild(add);
}

// get_shortcuts: not a command — map to a generic listing via get_weather-style addition
declare module "./shared" {}

boot().catch((e) => document.body.insertAdjacentHTML("beforeend", `<pre>${String(e)}</pre>`));
