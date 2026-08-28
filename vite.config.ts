import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri 固定使用 1420 端口；忽略 src-tauri 变更，避免前端热更新误触发 Rust 重编译
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "esnext",
    sourcemap: true,
  },
});
