// Shared helpers for internal pages.
export function invoke<T = any>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return (window as any).__TAURI__.core.invoke(cmd, args);
}
export function listen(event: string, cb: (payload: any) => void): Promise<() => void> {
  return (window as any).__TAURI__.event.listen(event, (e: any) => cb(e.payload));
}
export const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector(sel) as T;
export const $$ = <T extends HTMLElement = HTMLElement>(sel: string) => Array.from(document.querySelectorAll(sel)) as unknown as T[];

export function escapeHtml(s: string): string {
  return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

export function hostOf(url: string): string {
  try { return new URL(url).hostname; } catch { return url; }
}

export async function applyThemeFromSettings() {
  try {
    const s = await invoke("get_settings");
    const theme = s.theme || "system";
    const dark = theme === "dark" || (theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
    document.documentElement.setAttribute("data-theme", dark ? "dark" : "light");
    document.body.setAttribute("data-theme", dark ? "dark" : "light");
  } catch {}
}

export function timeAgo(ms: number): string {
  const diff = Date.now() - ms;
  const m = Math.floor(diff / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d} d ago`;
  return new Date(ms).toLocaleDateString();
}

export function formatBytes(n: number): string {
  if (!n) return "—";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}

export function navigate(input: string) {
  return invoke("navigate_active", { input });
}
