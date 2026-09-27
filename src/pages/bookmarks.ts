import { invoke, $, escapeHtml, applyThemeFromSettings } from "./shared";

interface BookmarkNode { id: number; title: string; url: string }

function trashSvg() {
  return `<svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 19a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"/></svg>`;
}
function barSvg() {
  return `<svg viewBox="0 0 24 24" fill="currentColor"><path d="M4 8h4V4H4v4zm6 12h4v-4h-4v4zm-6 0h4v-4H4v4zm0-6h4v-4H4v4zm6 0h4v-4h-4v4zm6-10v4h4V4h-4zm-6 4h4V4h-4v4zm6 6h4v-4h-4v4zm0 6h4v-4h-4v4z"/></svg>`;
}

function row(b: BookmarkNode, toOther: boolean) {
  const el = document.createElement("div");
  el.className = "list-row";
  const host = (() => { try { return new URL(b.url).hostname; } catch { return b.url; } })();
  el.innerHTML = `
    <span class="row-icon"><span style="font-weight:600">${(b.title[0] || "?").toUpperCase()}</span></span>
    <span class="row-main">
      <div class="row-title">${escapeHtml(b.title)}</div>
      <div class="row-sub">${escapeHtml(b.url)}</div>
    </span>
    <span class="row-side">
      ${toOther ? `<button class="icon-btn mv" title="Move to bookmarks bar">${barSvg()}</button>` : ""}
      <button class="icon-btn rm" title="Delete">${trashSvg()}</button>
    </span>`;
  (el.querySelector(".row-main") as HTMLElement).addEventListener("click", () =>
    invoke("navigate_active", { input: b.url })
  );
  el.querySelector(".rm")!.addEventListener("click", async () => {
    await invoke("remove_bookmark", { id: b.id });
    refresh();
  });
  el.querySelector(".mv")?.addEventListener("click", async () => {
    await invoke("move_bookmark_to_bar", { id: b.id });
    refresh();
  });
  return el;
}

async function refresh() {
  const bm = await invoke("list_bookmarks");
  const bar = $("#bar-list");
  const other = $("#other-list");
  bar.innerHTML = "";
  other.innerHTML = "";
  if (!bm.bar.length) bar.innerHTML = `<div class="empty-state">No bookmarks on the bar yet.</div>`;
  if (!bm.other.length) other.innerHTML = `<div class="empty-state">Nothing in Other Bookmarks.</div>`;
  for (const b of bm.bar) bar.appendChild(row(b, false));
  for (const b of bm.other) other.appendChild(row(b, true));
}

async function boot() {
  await applyThemeFromSettings();
  $("#add").addEventListener("click", () => {
    const title = prompt("Name:");
    if (!title) return;
    let url = prompt("URL:");
    if (!url) return;
    if (!url.startsWith("http")) url = "https://" + url;
    invoke("add_bookmark", { title, url, toOther: true }).then(refresh);
  });
  refresh();
}

boot();
