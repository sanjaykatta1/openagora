import { resolve } from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  root: "renderer",
  base: "./",
  plugins: [react()],
  build: {
    outDir: "../dist/renderer",
    emptyOutDir: true,
    rollupOptions: {
      // The window's page, and the empty page window captures run in.
      input: { index: resolve(__dirname, "renderer/index.html"), capture: resolve(__dirname, "renderer/capture.html") },
    },
  },
});
