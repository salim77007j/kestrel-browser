import { defineConfig } from "vite";
import { resolve } from "path";

const page = (name: string) => resolve(__dirname, `${name}.html`);

export default defineConfig({
  base: "./",
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    target: "chrome110",
    minify: "esbuild",
    rollupOptions: {
      input: {
        chrome: page("chrome"),
        newtab: page("pages/newtab"),
        settings: page("pages/settings"),
        history: page("pages/history"),
        bookmarks: page("pages/bookmarks"),
        downloads: page("pages/downloads"),
        privacy: page("pages/privacy"),
        blocked: page("pages/blocked"),
        about: page("pages/about"),
      },
    },
  },
});
