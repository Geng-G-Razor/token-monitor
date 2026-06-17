import { defineConfig } from "vite";

// Tauri expects a fixed port and the dev server to be reachable on localhost
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "localhost",
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
  },
});
