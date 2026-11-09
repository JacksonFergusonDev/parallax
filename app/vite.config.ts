import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

const root = (path: string) => new URL(path, import.meta.url).pathname;

// The dev server needs a socket for reloads, which a built reader never gets.
const readerDevPolicy = {
  name: "reader-dev-policy",
  apply: "serve" as const,
  transformIndexHtml: (html: string) =>
    html.replace(
      "connect-src ipc: http://ipc.localhost",
      "connect-src ipc: http://ipc.localhost ws://localhost:1420 http://localhost:1420",
    ),
};

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), readerDevPolicy],

  resolve: {
    alias: [
      { find: "foliate-js", replacement: root("../vendor/foliate-js") },
      // The vendored engine sits outside app/, so it can't find node_modules.
      {
        find: "construct-style-sheets-polyfill",
        replacement: root("node_modules/construct-style-sheets-polyfill"),
      },
      // Formats the reader doesn't open; see vendor/foliate-js/VENDORED.md.
      {
        find: /^\.\/(comic-book|fb2|mobi|pdf|vendor\/zip|vendor\/fflate)\.js$/,
        replacement: root("src/reader/unsupportedFormat.ts"),
      },
    ],
  },

  build: {
    // The reader's engine uses top-level await and private fields.
    target: "esnext",
    rollupOptions: {
      input: [
        root("index.html"),
        root("reader.html"),
        root("settings.html"),
        root("about.html"),
      ],
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    fs: { allow: [root("..")] },
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
