// 独立 vite 配置 — 移动端 SPA（PWA-first 路线）
//
// 启动：pnpm mobile:dev → http://localhost:1520
// 构建：pnpm mobile:build → dist-mobile/（后续由 axum 静态托管给手机访问）
//
// 详见 .claude/skills/mobile-app-architecture/skill.md

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";

export default defineConfig(({ command }) => ({
  plugins: [react(), tailwindcss()],

  // 入口在 src/mobile/index.html
  root: path.resolve(__dirname, "src/mobile"),

  // 用相对路径 "./" 兼容所有部署：Tauri Mobile（webview 从根加载）+ axum 任意子路径
  // dev 模式 vite 默认 "/"
  base: command === "build" ? "./" : "/",

  resolve: {
    alias: {
      "@m": path.resolve(__dirname, "src/mobile"),
    },
  },

  build: {
    outDir: path.resolve(__dirname, "dist-mobile"),
    emptyOutDir: true,
    target: "es2020",
  },

  server: {
    port: 1520,
    strictPort: true,
    host: "0.0.0.0", // 让局域网手机也能访问 vite dev（联调用）
  },

  clearScreen: false,
}));
