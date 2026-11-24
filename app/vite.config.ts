import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Frontend sources live in `ui/`; the build goes to `dist/`, which Tauri
// embeds into the binary (see tauri.conf.json → build.frontendDist).
export default defineConfig({
  root: "ui",
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: "127.0.0.1",
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    // Never inline assets as data: URIs (keeps the CSP strict).
    assetsInlineLimit: 0,
    // WebView2 on Windows 10/11 is evergreen Chromium.
    target: "chrome120",
    sourcemap: false,
  },
});
