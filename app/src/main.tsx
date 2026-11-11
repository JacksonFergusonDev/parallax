import React from "react";
import ReactDOM from "react-dom/client";
import { isTauri } from "@tauri-apps/api/core";
import App from "./App";
import { BookWarpFilter } from "./components/BookWarpFilter";

async function start() {
  if (import.meta.env.DEV && !isTauri()) {
    const { installMockBackend } = await import("./dev/mockBackend");
    installMockBackend();
  }
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <BookWarpFilter />
      <App />
    </React.StrictMode>,
  );
}

start();
