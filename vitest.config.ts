import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";

// 前端组件测试配置：jsdom 环境 + Vue 插件，用例与源码同目录（*.spec.ts）
export default defineConfig({
  plugins: [vue()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.spec.ts"],
    setupFiles: ["./src/test/setup.ts"],
  },
});
