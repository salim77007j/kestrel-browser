import { invoke, $, escapeHtml, timeAgo, applyThemeFromSettings } from "./shared";

interface Entry { id: number; url: string; title: string; visited_at: number }

function dayLabel(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const y = new Date(Date.now() - 86400000);
  if (d.toDateString() === today.toDateString()) return "Today";
  if (d.toDateString() === y.toDateString()) return "Yesterday";
  return d.toLocaleDateString([], { weekday: "long", month: "short", day: "numeric" });
}

async function refresh(q: string) {
  const items: Entry[] = await invoke("history_search", { query: q, limit: 400 });
  const list = $("#list");
  if (!items.length) {
    list.innerHTML = `<div class="empty-state">No history yet. Pages you visit will show up here.</div>`;
    return;
  }
  let lastDay = "";
  list.innerHTML = "";
  for (const it of items) {
    const day = dayLabel(it.visited_at);
    if (day !== lastDay) {
      const h = document.createElement("div");
      h.className = "day-header";
      h.textContent = day;
      list.appendChild(h);
      lastDay = day;
    }
    const row = document.createElement("div");
    row.className = "list-row";
    const host = (() => { try { return new URL(it.url).hostname; } catch { return it.url; } })();
    const favicon = `https://icons.duckduckgo.com/ip3/${host}.ico`;
    row.innerHTML = `
      <span class="row-icon"><img src="${favicon}" style="width:16px;height:16px" onerror="this.replaceWith(Object.assign(document.createElement('span'),{textContent:'${(host[0]||'?').toUpperCase()}'}))" /></span>
      <span class="row-main">
        <div class="row-title">${escapeHtml(it.title || it.url)}</div>
        <div class="row-sub">${escapeHtml(it.url)}</div>
      </span>
      <span class="row-side">
        ${timeAgo(it.visited_at)}
        <button class="icon-btn" title="Remove">${trashSvg()}</button>
      </span>`;
    (row.querySelector(".row-main") as HTMLElement).addEventListener("click", () =>
      invoke("navigate_active", { input: it.url })
    );
    (row.querySelector(".icon-btn") as HTMLElement).addEventListener("click", async () => {
      await invoke("delete_history_item", { id: it.id });
      refresh(($("#q") as HTMLInputElement).value);
    });
    row.addEventListener("auxclick", (e) => {
      if (e.button === 1) invoke("create_tab", { url: it.url, background: true });
    });
    list.appendChild(row);
  }
}

function trashSvg() {
  return `<svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 19a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"/></svg>`;
}

async function boot() {
  await applyThemeFromSettings();
  const q = $("#q") as HTMLInputElement;
  q.addEventListener("input", () => refresh(q.value.trim()));
  $("#clear").addEventListener("click", async () => {
    if (confirm("Clear ALL browsing history?")) {
      await invoke("clear_history");
      refresh("");
    }
  });
  refresh("");
}

boot();
