import { invoke, $, escapeHtml, formatBytes, applyThemeFromSettings } from "./shared";
import { listen } from "../lib/api";

interface DownloadRecord {
  id: number; url: string; path: string; filename: string;
  total_bytes: number; received_bytes: number;
  state: "active" | "completed" | "cancelled" | "failed";
  started_at: number;
}

function progressSvg(state: DownloadRecord["state"], received: number, total: number): string {
  if (state === "active") {
    const pct = total > 0 ? Math.round((received / total) * 100) : 12;
    return `<svg viewBox="0 0 36 36" width="32" height="32">
      <circle cx="18" cy="18" r="15" fill="none" stroke="var(--border)" stroke-width="3"/>
      <circle cx="18" cy="18" r="15" fill="none" stroke="var(--accent)" stroke-width="3"
        stroke-dasharray="${(pct * 0.942).toFixed(1)} 200" stroke-linecap="round" transform="rotate(-90 18 18)"/>
      <text x="18" y="22" text-anchor="middle" font-size="9" fill="var(--text-2)">${pct}%</text></svg>`;
  }
  const color = state === "completed" ? "#188038" : "var(--danger)";
  const glyph = state === "completed"
    ? "M9 16.2 4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4z"
    : "M19 6.4 17.6 5 12 10.6 6.4 5 5 6.4 10.6 12 5 17.6 6.4 19 12 13.4 17.6 19 19 17.6 13.4 12z";
  return `<svg viewBox="0 0 24 24" width="30" height="30"><circle cx="12" cy="12" r="11" fill="${color}" opacity="0.15"/><path d="${glyph}" fill="${color}"/></svg>`;
}

function row(d: DownloadRecord) {
  const el = document.createElement("div");
  el.className = "list-row";
  el.innerHTML = `
    <span class="row-icon" style="background:transparent">${progressSvg(d.state, d.received_bytes, d.total_bytes)}</span>
    <span class="row-main">
      <div class="row-title">${escapeHtml(d.filename)}</div>
      <div class="row-sub">${escapeHtml(d.url)}</div>
      ${d.state === "active" ? `<div style="font-size:12px;color:var(--text-2);margin-top:4px">${formatBytes(d.received_bytes)}${d.total_bytes ? " / " + formatBytes(d.total_bytes) : ""}</div>` : ""}
    </span>
    <span class="row-side">
      ${d.state === "completed" ? `<button class="icon-btn op" title="Open file">⤢</button>
      <button class="icon-btn sf" title="Show in folder">📁</button>` : ""}
    </span>`;
  if (d.state === "completed") {
    el.querySelector(".op")?.addEventListener("click", () => invoke("open_download", { id: d.id }));
    el.querySelector(".sf")?.addEventListener("click", () => invoke("show_in_folder", { id: d.id }));
  }
  return el;
}

async function refresh() {
  const items: DownloadRecord[] = await invoke("list_downloads");
  const list = $("#list");
  list.innerHTML = "";
  if (!items.length) {
    list.innerHTML = `<div class="empty-state">Downloads you start in Kestrel will appear here.</div>`;
    return;
  }
  for (const d of items) list.appendChild(row(d));
}

async function boot() {
  await applyThemeFromSettings();
  $("#clear").addEventListener("click", async () => { await invoke("clear_finished_downloads"); refresh(); });
  await listen("downloads-changed", refresh);
  refresh();
  // live refresh while downloads run
  setInterval(refresh, 1500);
}

boot();
