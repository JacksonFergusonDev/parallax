import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

export default function App() {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    invoke<string>("app_version")
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);

  return (
    <main className="shell">
      <h1>E-Reader Companion</h1>
      <p className="muted">
        {version ? `v${version}` : "Backend not connected"}
      </p>
    </main>
  );
}
