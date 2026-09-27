import { invoke, $, escapeHtml, applyThemeFromSettings } from "./shared";

interface Stats {
  blockedToday: number; trackersToday: number; requestsToday: number;
  blockedAll: number; trackersAll: number; rules: number; engineReady: boolean;
  topDomains: { domain: string; count: number }[];
}

function statCard(num: string | number, label: string, sub = ""): string {
  return `<div class="stat-card"><div class="num">${num}</div><div class="lbl">${label}${sub ? ` · <span>${sub}</span>` : ""}</div></div>`;
}

async function refresh() {
  const s: Stats = await invoke("get_adblock_stats");
  $("#stats").innerHTML = [
    statCard(s.blockedToday.toLocaleString(), "Ads & trackers blocked", "today"),
    statCard(s.trackersToday.toLocaleString(), "Trackers stopped", "today"),
    statCard(s.blockedAll.toLocaleString(), "Blocked all time"),
    statCard(s.rules.toLocaleString(), "Rules in engine", s.engineReady ? "active" : "loading…"),
  ].join("");
  const top = $("#top-domains");
  if (!s.topDomains.length) {
    top.innerHTML = `<div class="empty-state" style="padding:20px 0;text-align:left">Nothing blocked yet — browse a news site and come back.</div>`;
  } else {
    top.innerHTML = "";
    for (const d of s.topDomains) {
      const row = document.createElement("div");
      row.className = "list-row";
      row.innerHTML = `
        <span class="row-icon"><span style="font-weight:700">${(d.domain[0] || "?").toUpperCase()}</span></span>
        <span class="row-main"><div class="row-title">${escapeHtml(d.domain)}</div></span>
        <span class="row-side">${d.count}</span>`;
      top.appendChild(row);
    }
  }
}

async function runSelfTest() {
  const box = $("#test");
  box.className = "";
  box.innerHTML = `<div style="color:var(--text-2)">Running…</div>`;
  const results: { name: string; pass: boolean; detail: string }[] = await invoke("run_privacy_selftest");
  box.innerHTML = results
    .map(
      (r) => `<div class="selftest-row">
        <span class="${r.pass ? "ok" : "fail"}">${r.pass ? "PASS" : "FAIL"}</span>
        <span>${escapeHtml(r.name)}</span>
        <span style="color:var(--text-2);margin-left:auto">${escapeHtml(r.detail)}</span>
      </div>`
    )
    .join("");
}

async function boot() {
  await applyThemeFromSettings();
  $("#selftest").addEventListener("click", runSelfTest);
  $("#update").addEventListener("click", async () => {
    const btn = $("#update") as HTMLButtonElement;
    btn.disabled = true;
    btn.textContent = "Updating…";
    const box = $("#test");
    try {
      const report: string = await invoke("update_filter_lists_now");
      box.className = "";
      box.innerHTML = `<pre style="font-size:12px;white-space:pre-wrap">${escapeHtml(report)}</pre>`;
    } catch (e) {
      box.className = "";
      box.innerHTML = `<div class="fail">Update failed: ${escapeHtml(String(e))}</div>`;
    }
    btn.disabled = false;
    btn.textContent = "Update filter lists";
    refresh();
  });
  refresh();
  setInterval(refresh, 3000);
}

boot();
