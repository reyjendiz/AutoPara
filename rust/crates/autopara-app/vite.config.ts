import { defineConfig } from "vite";

export default defineConfig({
  root: "ui",
  clearScreen: false,
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2022",
  },
  server: {
    port: 5173,
    strictPort: true,
    // Relative to `root`: the crates directory, so the string table in autopara-core can be read.
    fs: { allow: ["../.."] },
  },
});
