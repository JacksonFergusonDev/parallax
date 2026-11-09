import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ExternalLink } from "lucide-react";
import { bundle } from "../../src-tauri/tauri.conf.json";
import { followTheme, type AppTheme } from "../design/theme";
import "../styles.css";
import "./about.css";

type AboutInfo = {
  version: string;
  theme: AppTheme;
};

function AboutApp({ info }: { info: AboutInfo | null }) {
  const [error, setError] = useState(false);
  useEffect(() => {
    if (isTauri()) void invoke("show_window").catch(console.error);
  }, []);

  const openRepository = () => {
    setError(false);
    void invoke("about_open_repository").catch(() => setError(true));
  };

  return (
    <main className="about" aria-labelledby="about-title">
      <header className="about-header">
        <img src="/parallax.svg" alt="" width={64} height={64} />
        <div>
          <h1 id="about-title">Parallax</h1>
          {info && <p>Version {info.version}</p>}
        </div>
      </header>
      <div className="about-details">
        <section className="about-credits" aria-labelledby="about-credits">
          <h2 id="about-credits">Open source</h2>
          <p>
            Parallax is free software under the GNU General Public License,
            version 3.
          </p>
          <p>
            Built with Tauri, foliate-js for reading, boko for Amazon formats,
            and kepubify for Kobo books.
          </p>
          <p>
            Kobo device support draws on calibre. Parts of the comic processing
            pipeline were adapted from Kindle Comic Converter.
          </p>
        </section>
        <footer className="about-footer">
          <small>{bundle.copyright}</small>
          <button className="primary" onClick={openRepository}>
            Parallax on GitHub
            <ExternalLink aria-hidden="true" />
          </button>
          {error && <p role="alert">Could not open Parallax on GitHub.</p>}
        </footer>
      </div>
    </main>
  );
}

async function start() {
  const theme = followTheme("system");
  let info: AboutInfo | null = null;
  if (isTauri()) {
    let changedTheme: AppTheme | undefined;
    await listen<AppTheme>(
      "app-theme-changed",
      ({ payload }) => {
        changedTheme = payload;
        theme.set(payload);
      },
      { target: "about" },
    ).catch(console.error);
    info = await invoke<AboutInfo>("about_info").catch((error) => {
      console.error(error);
      return null;
    });
    theme.set(changedTheme ?? info?.theme ?? "system");
  }
  createRoot(document.getElementById("root")!).render(<AboutApp info={info} />);
}

void start();
