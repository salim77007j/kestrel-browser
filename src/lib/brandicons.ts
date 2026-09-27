// Inline brand logos for the bookmarks bar / start page shortcuts /
// omnibox — matches the wed1.png design exactly without network fetches.

const G = (size: number) =>
  `<svg viewBox="0 0 48 48" width="${size}" height="${size}"><path fill="#FFC107" d="M43.6 20.1H42V20H24v8h11.3C33.7 32.7 29.2 36 24 36c-6.6 0-12-5.4-12-12s5.4-12 12-12c3.1 0 5.9 1.2 8 3l5.7-5.7C34.3 6.1 29.4 4 24 4 13 4 4 13 4 24s9 20 20 20 20-9 20-20c0-1.3-.1-2.6-.4-3.9z"/><path fill="#FF3D00" d="m6.3 14.7 6.6 4.8C14.7 15.1 19 12 24 12c3.1 0 5.9 1.2 8 3l5.7-5.7C34.3 6.1 29.4 4 24 4 16.3 4 9.7 8.3 6.3 14.7z"/><path fill="#4CAF50" d="M24 44c5.2 0 9.9-2 13.4-5.2l-6.2-5.2C29.2 35.1 26.7 36 24 36c-5.2 0-9.6-3.3-11.3-8l-6.5 5C9.5 39.6 16.2 44 24 44z"/><path fill="#1976D2" d="M43.6 20.1H42V20H24v8h11.3a12 12 0 0 1-4.1 5.6l6.2 5.2C41 35.4 44 30.2 44 24c0-1.3-.1-2.6-.4-3.9z"/></svg>`;

const YT = (size: number) =>
  `<svg viewBox="0 0 48 48" width="${size}" height="${size}"><path fill="#FF0000" d="M45 15.1a5.7 5.7 0 0 0-4-4C37.3 10 24 10 24 10s-13.3 0-17 1.1a5.7 5.7 0 0 0-4 4A59.7 59.7 0 0 0 2 24c0 3 .3 6 1 8.9a5.7 5.7 0 0 0 4 4C10.7 38 24 38 24 38s13.3 0 17-1.1a5.7 5.7 0 0 0 4-4c.7-2.9 1-5.9 1-8.9s-.3-6-1-8.9zM19.5 30.5v-13l11 6.5-11 6.5z"/></svg>`;

const GM = (size: number) =>
  `<svg viewBox="0 0 48 48" width="${size}" height="${size}"><path fill="#4caf50" d="M45 16.2 40 12l-16 12L8 12l-5 4.2V20l17 12.7L45 20z" opacity=".9"/><path fill="#1e88e5" d="M3 20v16a2 2 0 0 0 2 2h4V16.7L3 13.9z"/><path fill="#e53935" d="M39 38h4a2 2 0 0 0 2-2V20l-6 4.2z"/><path fill="#c62828" d="m35 16.7-11 8.2L13 16.7 8 12l22 16.5L45 12z"/></svg>`;

const MAPS = (size: number) =>
  `<svg viewBox="0 0 24 24" width="${size}" height="${size}"><path fill="#1a73e8" d="m12 2a7 7 0 0 0-7 7c0 5.25 7 13 7 13s7-7.75 7-13a7 7 0 0 0-7-7z"/><circle cx="12" cy="9" r="2.6" fill="#fff"/><path fill="#34a853" d="M3.6 9.6 2 10.5V20l2-.8z" opacity="0"/><path fill="#ea4335" d="m19.9 5.6-2.4 1.1L21 11.7l1.5-1z" opacity="0"/><path fill="#fbbc04" d="m2.2 10.2 7.1 4.1L2 17.9z" opacity="0"/></svg>`;

const DRIVE = (size: number) =>
  `<svg viewBox="0 0 87.3 78" width="${size}" height="${size}"><path fill="#0066da" d="m6.6 66.85 3.85 6.65c.8 1.4 1.95 2.5 3.3 3.3L27.5 53H0c0 1.55.4 3.1 1.2 4.5z"/><path fill="#00ac47" d="M43.65 25 29.9 1.2c-1.35.8-2.5 1.9-3.3 3.3l-25.4 44A9.06 9.06 0 0 0 0 53h27.5z"/><path fill="#ea4335" d="M73.55 76.8c1.35-.8 2.5-1.9 3.3-3.3l1.6-2.75L86.1 57.5c.8-1.4 1.2-2.95 1.2-4.5H59.8l5.85 11.5z"/><path fill="#00832d" d="m43.65 25 13.75-23.8c-1.35-.8-2.9-1.2-4.5-1.2H34.4c-1.6 0-3.15.45-4.5 1.2z"/><path fill="#2684fc" d="M59.8 53H27.5L13.75 76.8c1.35.8 2.9 1.2 4.5 1.2h50.8c1.6 0 3.15-.45 4.5-1.2z"/><path fill="#ffba00" d="m73.4 26.5-12.7-22c-.8-1.4-1.95-2.5-3.3-3.3L43.65 25 59.8 53h27.45c0-1.55-.4-3.1-1.2-4.5z"/></svg>`;

const GMAIL_HOST = GM;
const GOOGLE_HOST = G;

const LOGOS: Record<string, (size: number) => string> = {
  "mail.google.com": GMAIL_HOST,
  "drive.google.com": DRIVE,
  "maps.google.com": MAPS,
  "google.com": GOOGLE_HOST,
  "youtube.com": YT,
  "youtu.be": YT,
};

/** Colored brand logo for a host, or null when unknown. */
export function brandIcon(url: string, size = 16): string | null {
  let host = "";
  try { host = new URL(url).hostname; } catch { return null; }
  const keys = Object.keys(LOGOS).sort((a, b) => b.length - a.length);
  const key = keys.find((k) => host === k || host.endsWith("." + k));
  return key ? LOGOS[key](size) : null;
}

/** Colored apps-grid (Kestrel start page shortcut). */
export function appsIcon(size = 16): string {
  const c = ["#4285f4", "#34a853", "#fbbc04", "#ea4335", "#4285f4", "#34a853", "#fbbc04", "#ea4335", "#4285f4"];
  const s = size;
  const cells = c
    .map((col, i) => {
      const x = (i % 3) * (s * 0.38) + s * 0.06;
      const y = Math.floor(i / 3) * (s * 0.38) + s * 0.06;
      const r = s * 0.13;
      return `<circle cx="${(x + r).toFixed(1)}" cy="${(y + r).toFixed(1)}" r="${r.toFixed(1)}" fill="${col}"/>`;
    })
    .join("");
  return `<svg viewBox="0 0 ${s} ${s}" width="${s}" height="${s}">${cells}</svg>`;
}
