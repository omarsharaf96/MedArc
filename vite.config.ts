import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [react()],

  // Prevent vite from obscuring Rust errors
  clearScreen: false,

  server: {
    // Tauri expects a fixed port; fail if that port is not available
    port: 1420,
    strictPort: true,
    // Allow Tauri dev server to access
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },

  // Environment variables starting with TAURI_ are exposed to the frontend
  envPrefix: ["VITE_", "TAURI_"],
});
