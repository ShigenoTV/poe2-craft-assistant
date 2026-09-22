import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

// Deux pages : fenêtre principale et overlay (fenêtre Tauri séparée, transparente).
export default defineConfig({
  plugins: [react()],
  resolve: { alias: { "@": resolve(__dirname, "src") } },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2021",
    rollupOptions: { input: { main: resolve(__dirname, "index.html"), overlay: resolve(__dirname, "overlay.html") } },
  },
});
