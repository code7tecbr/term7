import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Porta fixa: o Tauri aponta pra ela em src-tauri/tauri.conf.json (devUrl).
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
