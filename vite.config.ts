import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri espera porta fixa em desenvolvimento e não deve observar src-tauri.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "es2021",
    outDir: "dist",
  },
});
