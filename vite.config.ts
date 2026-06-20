import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },

  clearScreen: false,
  optimizeDeps: {
    include: [
      "@uiw/react-codemirror",
      "@uiw/codemirror-theme-github",
      "@codemirror/commands",
      "@codemirror/lang-css",
      "@codemirror/lang-cpp",
      "@codemirror/lang-go",
      "@codemirror/lang-html",
      "@codemirror/lang-java",
      "@codemirror/lang-javascript",
      "@codemirror/lang-json",
      "@codemirror/lang-markdown",
      "@codemirror/lang-php",
      "@codemirror/lang-python",
      "@codemirror/lang-rust",
      "@codemirror/lang-sql",
      "@codemirror/lang-xml",
      "@codemirror/lang-yaml",
      "@codemirror/language",
      "@codemirror/view",
    ],
  },
  server: {
    port: 1422,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1432,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
