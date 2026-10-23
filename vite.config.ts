import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

// Deux pages : fenêtre principale et overlay (fenêtre Tauri séparée, transparente).
export default defineConfig({
  plugins: [react()],
  resolve: { alias: { "@": resolve(import.meta.dirname, "src") } },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // "target" (sortie de compilation Rust, à la racine du dépôt dans cette configuration en workspace)
    // et "src-tauri" ne doivent jamais être surveillés par Vite : ce sont des fichiers qui changent en
    // continu pendant que cargo compile, sans rapport avec le frontend. Sur Windows, surveiller un
    // fichier verrouillé par cargo (ex. un .pdb en cours d'écriture) fait planter le serveur de dev
    // avec une erreur EBUSY.
    watch: { ignored: ["**/target/**", "**/src-tauri/**"] },
  },
  build: {
    target: "es2021",
    rolldownOptions: { input: { main: resolve(import.meta.dirname, "index.html"), overlay: resolve(import.meta.dirname, "overlay.html") } },
  },
});
