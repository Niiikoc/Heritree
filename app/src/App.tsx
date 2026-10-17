import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { Answers, AskOptions } from "./ui";
import { RepoView } from "./RepoView";
import { Dialog } from "./components/Dialog";
import type { PaletteCommand } from "./components/Palette";

const RECENT_KEY = "heritree.recentRepositories";
const TABS_KEY = "heritree.openTabs";

function load(key: string): string[] {
  try {
    return JSON.parse(localStorage.getItem(key) ?? "[]");
  } catch {
    return [];
  }
}

function save(key: string, list: string[]) {
  try {
    localStorage.setItem(key, JSON.stringify(list));
  } catch {
    // Recent repositories and tabs are a convenience; ignore storage failures.
  }
}

const folderName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

/** "https://github.com/acme/tool.git" → "tool". */
const repoNameFromUrl = (url: string) =>
  url
    .trim()
    .replace(/\/+$/, "")
    .split(/[/:]/)
    .pop()
    ?.replace(/\.git$/, "") ?? "";

export function App() {
  const [tabs, setTabs] = useState<string[]>(() => load(TABS_KEY));
  const [active, setActive] = useState(() => (load(TABS_KEY).length ? 0 : -1));
  const [recent, setRecent] = useState(() => load(RECENT_KEY));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [dialog, setDialog] = useState<(AskOptions & { resolve: (a: Answers | null) => void }) | null>(null);

  useEffect(() => save(TABS_KEY, tabs), [tabs]);

  const ask = (options: AskOptions) => new Promise<Answers | null>((resolve) => setDialog({ ...options, resolve }));

  /** Validates `path` as a repository and shows it in a tab. */
  const openPath = useCallback(async (path: string) => {
    setError(null);
    setBusy("Opening");
    try {
      const info = await api.openRepository(path);
      const root = info.workdir ?? info.gitDir;
      setTabs((t) => {
        const existing = t.indexOf(root);
        if (existing >= 0) {
          setActive(existing);
          return t;
        }
        setActive(t.length);
        return [...t, root];
      });
      const list = [root, ...load(RECENT_KEY).filter((p) => p !== root)].slice(0, 10);
      save(RECENT_KEY, list);
      setRecent(list);
    } catch (e) {
      setError(String(e));
      setActive(-1);
    } finally {
      setBusy(null);
    }
  }, []);

  const pickFolder = async () => {
    const path = await open({ directory: true, title: "Open a Git repository" });
    if (typeof path === "string") await openPath(path);
  };

  const clone = async () => {
    const a = await ask({
      title: "Clone a repository",
      fields: [
        { name: "url", label: "Repository URL", placeholder: "https://github.com/owner/repo.git" },
        { name: "parent", label: "Save in folder", browse: true },
        { name: "name", label: "Folder name (default: from the URL)", optional: true },
      ],
      confirm: "Clone",
    });
    if (!a) return;
    const url = String(a.url).trim();
    const name = String(a.name).trim() || repoNameFromUrl(url);
    setError(null);
    setBusy(`Cloning ${name}`);
    try {
      const path = await api.cloneRepository(url, String(a.parent), name);
      await openPath(path);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const init = async () => {
    const path = await open({ directory: true, title: "Choose a folder for the new repository" });
    if (typeof path !== "string") return;
    setError(null);
    try {
      await api.initRepository(path);
      await openPath(path);
    } catch (e) {
      setError(String(e));
    }
  };

  const closeTab = (index: number) => {
    setTabs((t) => t.filter((_, i) => i !== index));
    setActive((a) => (a > index ? a - 1 : a === index ? Math.min(index, tabs.length - 2) : a));
  };

  const appCommands: PaletteCommand[] = [
    { id: "open", label: "Open repository…", keywords: "folder", run: pickFolder },
    { id: "clone", label: "Clone repository…", keywords: "download url", run: clone },
    { id: "init", label: "Create new repository…", keywords: "init", run: init },
    ...tabs.map((t, i) => ({ id: `tab-${t}`, label: `Switch to ${folderName(t)}`, keywords: t, run: () => setActive(i) })),
  ];

  const current = active >= 0 ? tabs[active] : undefined;

  return (
    <div className="shell">
      <nav className="tabs" aria-label="Open repositories">
        {tabs.map((t, i) => (
          <div key={t} className={`tab${i === active ? " active" : ""}`} title={t}>
            <button className="tab-label" onClick={() => setActive(i)}>
              {folderName(t)}
            </button>
            <button className="tab-close" onClick={() => closeTab(i)} aria-label={`Close ${folderName(t)}`}>
              ✕
            </button>
          </div>
        ))}
        <button className={`tab-new${active === -1 ? " active" : ""}`} onClick={() => setActive(-1)} title="Open, clone or create a repository">
          +
        </button>
      </nav>

      {current ? (
        <RepoView key={current} path={current} appCommands={appCommands} />
      ) : (
        <main className="welcome">
          <img src="/icon.png" alt="" width={72} height={72} />
          <h1>Heritree</h1>
          <p className="tagline">See where every branch came from.</p>
          <div className="welcome-actions">
            <button className="primary" onClick={pickFolder} disabled={!!busy}>
              Open repository
            </button>
            <button onClick={clone} disabled={!!busy}>
              Clone…
            </button>
            <button onClick={init} disabled={!!busy}>
              New repository…
            </button>
          </div>
          {busy && <p className="muted">{busy}…</p>}
          {error && <p className="error">{error}</p>}
          {recent.length > 0 && (
            <section className="recent">
              <h2>Recent</h2>
              <ul>
                {recent.map((path) => (
                  <li key={path}>
                    <button onClick={() => openPath(path)} disabled={!!busy} title={path}>
                      <strong>{folderName(path)}</strong> <span className="muted">{path}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </main>
      )}

      {dialog && (
        <Dialog
          {...dialog}
          onDone={(answers) => {
            dialog.resolve(answers);
            setDialog(null);
          }}
        />
      )}
    </div>
  );
}
