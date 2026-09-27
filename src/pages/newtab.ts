import { invoke, $, escapeHtml, hostOf, applyThemeFromSettings } from "./shared";
import { brandIcon } from "../lib/brandicons";

interface Shortcut { title: string; url: string }

function logoFor(url: string, title: string): string {
  return (
    brandIcon(url, 30) ||
    `<span style="font-size:26px;font-weight:600;color:var(--accent)">${(title || hostOf(url))[0]?.toUpperCase() || "?"}</span>`
  );
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
