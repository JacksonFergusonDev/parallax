import React from "react";
import ReactDOM from "react-dom/client";
import { isTauri } from "@tauri-apps/api/core";
import { SettingsApp } from "./SettingsApp";
import "../styles.css";
import "./settings.css";

async function start() {
  if (import.meta.env.DEV && !isTauri()) {
    const { installMockBackend } = await import("../dev/mockBackend");
    installMockBackend("settings");
  }
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <SettingsApp />
    </React.StrictMode>,
  );
}

start();
