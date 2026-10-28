import { defineConfig } from "vite";

export default defineConfig({
  root: "ui",
  clearScreen: false,
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2022",
    // The overlay is a second window with its own page.
    rollupOptions: { input: { main: "ui/index.html", overlay: "ui/overlay.html" } },
  },
  server: {
    port: 5173,
    strictPort: true,
    // Relative to `root`: the crates directory, so the string table in autopara-core can be read.
    fs: { allow: ["../.."] },
  },
});
