import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { FileStatus, LogRow, RefInfo, RepoInfo } from "./types";
import { Sidebar } from "./components/Sidebar";
import { CommitList } from "./components/CommitList";
import { CommitPanel, WorkingTreePanel } from "./components/DetailsPanel";

/** Enough for everyday work; paging past it comes with the MVP. */
const LOG_LIMIT = 5000;
const RECENT_KEY = "heritree.recentRepositories";

function loadRecent(): string[] {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
  } catch {
    return [];
  }
}

function saveRecent(path: string) {
  try {
    const list = [path, ...loadRecent().filter((p) => p !== path)].slice(0, 8);
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    // Recent repositories are a convenience; ignore storage failures.
  }
}

interface RepoData {
  info: RepoInfo;
  refs: RefInfo[];
  rows: LogRow[];
  status: FileStatus[];
}

async function fetchAll(info?: RepoInfo): Promise<RepoData> {
  const [current, refs, rows, status] = await Promise.all([
    info ? Promise.resolve(info) : api.repoInfo(),
    api.listRefs(),
    api.loadLog(LOG_LIMIT),
    api.workingTreeStatus().catch(() => [] as FileStatus[]), // bare repositories
  ]);
  return { info: current, refs, rows, status };
}

export function App() {
  const [data, setData] = useState<RepoData | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [scrollRequest, setScrollRequest] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recent, setRecent] = useState(loadRecent);

  const openPath = useCallback(async (path: string) => {
    setBusy(true);
    setError(null);
    try {
      const info = await api.openRepository(path);
      const next = await fetchAll(info);
      setData(next);
      setSelected(next.status.length > 0 ? null : (next.info.head.id ?? next.rows[0]?.commit.id ?? null));
      saveRecent(info.workdir ?? info.gitDir);
      setRecent(loadRecent());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const pickFolder = async () => {
    const path = await open({ directory: true, title: "Open a Git repository" });
    if (typeof path === "string") await openPath(path);
  };

  const refresh = useCallback(async () => {
    if (!data) return;
    try {
      setData(await fetchAll());
    } catch (e) {
      setError(String(e));
    }
  }, [data]);

  // Pick up changes made outside the app (terminal, editor) when coming back.
  useEffect(() => {
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [refresh]);

  const refsByCommit = useMemo(() => {
    const map = new Map<string, RefInfo[]>();
    for (const r of data?.refs ?? []) map.set(r.target, [...(map.get(r.target) ?? []), r]);
    return map;
  }, [data]);

  if (!data) {
    return (
      <main className="welcome">
        <img src="/icon.png" alt="" width={72} height={72} />
        <h1>Heritree</h1>
        <p className="tagline">See where every branch came from.</p>
        <button className="primary" onClick={pickFolder} disabled={busy}>
          {busy ? "Opening…" : "Open repository"}
        </button>
        {error && <p className="error">{error}</p>}
        {recent.length > 0 && (
          <section className="recent">
            <h2>Recent</h2>
            <ul>
              {recent.map((path) => (
                <li key={path}>
                  <button onClick={() => openPath(path)} disabled={busy}>
                    {path}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
      </main>
    );
  }

  const { info, refs, rows, status } = data;
  const selectedRow = selected === null ? undefined : rows.find((r) => r.commit.id === selected);
  const headLabel = info.head.detached
    ? `detached at ${info.head.id?.slice(0, 7)}`
    : (info.head.branch ?? "no branch");

  return (
    <div className="app">
      <header className="toolbar">
        <button className="repo-switch" onClick={() => setData(null)} title="Open another repository">
          <strong>{info.name}</strong>
        </button>
        <span className="branch-pill" title="Current branch">
          {headLabel}
        </span>
        <div className="spacer" />
        {error && (
          <span className="error" role="alert">
            {error}
          </span>
        )}
        <button onClick={refresh} title="Reload refs, history and status">
          Refresh
        </button>
        <button onClick={pickFolder}>Open…</button>
      </header>
      <Sidebar
        refs={refs}
        currentBranch={info.head.detached ? null : info.head.branch}
        onPick={(r) => {
          setSelected(r.target);
          setScrollRequest((n) => n + 1);
        }}
      />
      <CommitList
        rows={rows}
        refsByCommit={refsByCommit}
        headId={info.head.id}
        changeCount={status.length}
        selectedId={selected}
        onSelect={setSelected}
        scrollRequest={scrollRequest}
      />
      {selectedRow ? (
        <CommitPanel commit={selectedRow.commit} refs={refsByCommit.get(selectedRow.commit.id) ?? []} />
      ) : (
        <WorkingTreePanel files={status} />
      )}
      <footer className="statusbar">
        <span>{info.workdir ?? info.gitDir}</span>
        <span>
          {rows.length.toLocaleString()} commits{rows.length === LOG_LIMIT ? " (newest only)" : ""}
        </span>
      </footer>
    </div>
  );
}
