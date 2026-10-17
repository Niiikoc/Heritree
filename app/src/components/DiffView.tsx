import { Fragment, useEffect, useMemo, useState } from "react";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import type { RepoApi } from "../api";
import type { BlameLine, CommitInfo, DiffLine, FileDiff, Hunk, LineRef } from "../types";
import type { Ask, Run } from "../ui";
import { highlightLine, languageFor } from "../highlight";
import { fullDate, timeAgo } from "../format";

export type DiffTarget =
  | { kind: "working"; path: string; oldPath: string | null; staged: boolean; untracked: boolean; conflicted: boolean }
  | { kind: "commit"; id: string; path: string; oldPath: string | null };

type Tab = "diff" | "history" | "blame";
type Mode = "inline" | "split";

const MODE_KEY = "heritree.diffMode";
const RENDER_LIMIT = 3000;

interface Props {
  api: RepoApi;
  workdir: string | null;
  target: DiffTarget;
  /** Bumped after every repository change, to reload. */
  version: number;
  run: Run;
  ask: Ask;
  onClose: () => void;
  onShowCommit: (id: string) => void;
}

const key = (r: LineRef) => `${r.hunk}:${r.line}`;
const changed = (l: DiffLine) => l.kind !== "context";

/** The diff of one file, with line and hunk staging, plus its history and blame. */
export function DiffView({ api, workdir, target, version, run, ask, onClose, onShowCommit }: Props) {
  const [diff, setDiff] = useState<FileDiff | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("diff");
  const [mode, setMode] = useState<Mode>(() => (localStorage.getItem(MODE_KEY) as Mode) || "inline");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [anchor, setAnchor] = useState<LineRef | null>(null);
  const [showAll, setShowAll] = useState(false);

  const working = target.kind === "working" ? target : null;
  const selectable = !!working && !working.conflicted;

  useEffect(() => {
    let live = true;
    setError(null);
    const load =
      target.kind === "working"
        ? api.fileDiff(target.path, target.oldPath, target.staged, target.untracked)
        : api.commitFileDiff(target.id, target.path, target.oldPath);
    load.then(
      (d) => live && (setDiff(d), setPicked(new Set())),
      (e) => live && (setDiff(null), setError(String(e))),
    );
    return () => {
      live = false;
    };
  }, [api, target, version]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !document.querySelector(".modal, .context-menu")) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const setModeSaved = (m: Mode) => {
    setMode(m);
    try {
      localStorage.setItem(MODE_KEY, m);
    } catch {
      // Only a preference.
    }
  };

  const language = useMemo(() => languageFor(target.path), [target.path]);
  const pickedRefs = useMemo(
    () => [...picked].map((k) => ({ hunk: +k.split(":")[0], line: +k.split(":")[1] })),
    [picked],
  );

  const toggle = (ref: LineRef, shift: boolean) => {
    const next = new Set(picked);
    if (shift && anchor && anchor.hunk === ref.hunk && diff) {
      const [from, to] = [Math.min(anchor.line, ref.line), Math.max(anchor.line, ref.line)];
      for (let l = from; l <= to; l++) if (changed(diff.hunks[ref.hunk].lines[l])) next.add(key({ hunk: ref.hunk, line: l }));
    } else if (next.has(key(ref))) {
      next.delete(key(ref));
    } else {
      next.add(key(ref));
    }
    setAnchor(ref);
    setPicked(next);
  };

  const hunkRefs = (h: number, hunk: Hunk): LineRef[] =>
    hunk.lines.flatMap((l, line) => (changed(l) ? [{ hunk: h, line }] : []));

  const path = target.path;
  const stageRefs = (refs: LineRef[]) =>
    run("Staging lines", () => api.stageLines(path, refs, !!working?.untracked));
  const unstageRefs = (refs: LineRef[]) => run("Unstaging lines", () => api.unstageLines(path, refs));
  const discardRefs = async (refs: LineRef[]) => {
    const ok = await ask({
      title: "Discard these changes?",
      message: `${refs.length} changed line(s) in ${path} go back to their staged version. You can undo this.`,
      confirm: "Discard",
      danger: true,
    });
    if (!ok) return;
    if (working?.untracked) await run("Discarding", () => api.discard([path]));
    else await run("Discarding lines", () => api.discardLines(path, refs));
  };

  const actionsFor = (refs: LineRef[], label: string) => {
    if (!working || working.conflicted) return null;
    return working.staged ? (
      <button onClick={() => unstageRefs(refs)}>Unstage {label}</button>
    ) : (
      <>
        <button onClick={() => stageRefs(refs)}>Stage {label}</button>
        <button className="danger-text" onClick={() => discardRefs(refs)}>
          Discard {label}
        </button>
      </>
    );
  };

  const fileActions = working && (
    <>
      {working.conflicted ? (
        <>
          <button onClick={() => run("Taking our version", () => api.resolveConflict(path, "ours"))}>Use ours</button>
          <button onClick={() => run("Taking their version", () => api.resolveConflict(path, "theirs"))}>Use theirs</button>
          <button onClick={() => run("Marking resolved", () => api.stage([path]))}>Mark resolved</button>
        </>
      ) : working.staged ? (
        <button onClick={() => run("Unstaging", () => api.unstage([path]))}>Unstage file</button>
      ) : (
        <button onClick={() => run("Staging", () => api.stage([path]))}>Stage file</button>
      )}
      {workdir && (
        <>
          <button onClick={() => openPath(`${workdir}/${path}`).catch(() => {})} title="Open in the default app">
            Open
          </button>
          <button onClick={() => revealItemInDir(`${workdir}/${path}`).catch(() => {})}>Show in folder</button>
        </>
      )}
    </>
  );

  let budget = showAll ? Infinity : RENDER_LIMIT;
  const lineCount = diff?.hunks.reduce((n, h) => n + h.lines.length, 0) ?? 0;

  return (
    <section className="diff-view" aria-label={`Changes in ${path}`}>
      <header className="diff-header">
        <button className="icon-button" onClick={onClose} title="Back to the graph (Esc)">
          ←
        </button>
        <div className="diff-title">
          <strong>{path}</strong>
          {target.oldPath && <span className="muted"> renamed from {target.oldPath}</span>}
          <span className="muted">
            {working ? (working.conflicted ? " · conflicted" : working.staged ? " · staged" : working.untracked ? " · new file" : " · unstaged") : ` · ${target.kind === "commit" ? target.id.slice(0, 7) : ""}`}
          </span>
        </div>
        <div className="segmented">
          {(["diff", "history", "blame"] as Tab[]).map((t) => (
            <button key={t} className={tab === t ? "active" : undefined} onClick={() => setTab(t)}>
              {t === "diff" ? "Changes" : t === "history" ? "History" : "Blame"}
            </button>
          ))}
        </div>
        {tab === "diff" && (
          <div className="segmented">
            <button className={mode === "inline" ? "active" : undefined} onClick={() => setModeSaved("inline")}>
              Inline
            </button>
            <button className={mode === "split" ? "active" : undefined} onClick={() => setModeSaved("split")}>
              Side by side
            </button>
          </div>
        )}
      </header>
      <div className="diff-toolbar">
        {fileActions}
        {picked.size > 0 && (
          <span className="selection-actions">
            {actionsFor(pickedRefs, `${picked.size} line${picked.size === 1 ? "" : "s"}`)}
            <button onClick={() => setPicked(new Set())}>Clear</button>
          </span>
        )}
        {selectable && picked.size === 0 && tab === "diff" && (
          <span className="hint-inline">Click line numbers to pick lines, Shift+click for a range.</span>
        )}
      </div>

      {tab === "history" && <FileHistory api={api} path={path} version={version} onShowCommit={onShowCommit} />}
      {tab === "blame" && <Blame api={api} path={path} version={version} language={language} onShowCommit={onShowCommit} />}
      {tab === "diff" && (
        <div className={`diff-body ${mode}`}>
          {error && <p className="error pad">{error}</p>}
          {diff?.binary && <p className="empty pad">Binary file. There are no lines to show.</p>}
          {diff && !diff.binary && diff.hunks.length === 0 && (
            <p className="empty pad">{working?.untracked ? "Empty file." : "No changes in the file contents (only its mode or name changed)."}</p>
          )}
          {diff?.hunks.map((hunk, h) => {
            if (budget <= 0) return null;
            const lines = hunk.lines.slice(0, budget);
            budget -= lines.length;
            return (
              <div key={h} className="hunk">
                <div className="hunk-header">
                  <code>{hunk.header}</code>
                  <span className="hunk-actions">{actionsFor(hunkRefs(h, hunk), "hunk")}</span>
                </div>
                {mode === "inline" ? (
                  <InlineLines
                    lines={lines}
                    hunk={h}
                    language={language}
                    picked={picked}
                    selectable={selectable}
                    onToggle={toggle}
                  />
                ) : (
                  <SplitLines lines={lines} hunk={h} language={language} picked={picked} selectable={selectable} onToggle={toggle} />
                )}
              </div>
            );
          })}
          {!showAll && lineCount > RENDER_LIMIT && (
            <button className="show-all" onClick={() => setShowAll(true)}>
              Show all {lineCount.toLocaleString()} lines
            </button>
          )}
        </div>
      )}
    </section>
  );
}

interface LinesProps {
  lines: DiffLine[];
  hunk: number;
  language: string | null;
  picked: Set<string>;
  selectable: boolean;
  onToggle: (ref: LineRef, shift: boolean) => void;
}

const SIGN = { added: "+", removed: "-", context: " " } as const;

function Code({ line, language }: { line: DiffLine; language: string | null }) {
  const html = useMemo(() => highlightLine(line.text.replace(/\r$/, ""), language), [line.text, language]);
  return (
    <code className="code">
      <span dangerouslySetInnerHTML={{ __html: html }} />
      {line.noNewline && <span className="no-newline" title="No newline at end of file">⏎̸</span>}
    </code>
  );
}

function InlineLines({ lines, hunk, language, picked, selectable, onToggle }: LinesProps) {
  return (
    <>
      {lines.map((line, i) => {
        const ref = { hunk, line: i };
        const isPicked = picked.has(key(ref));
        const canPick = selectable && changed(line);
        return (
          <div key={i} className={`diff-line ${line.kind}${isPicked ? " picked" : ""}`}>
            <span
              className={`gutter${canPick ? " pickable" : ""}`}
              onMouseDown={(e) => canPick && (e.preventDefault(), onToggle(ref, e.shiftKey))}
              title={canPick ? "Pick this line" : undefined}
            >
              <span className="ln">{line.oldLine ?? ""}</span>
              <span className="ln">{line.newLine ?? ""}</span>
            </span>
            <span className="sign">{SIGN[line.kind]}</span>
            <Code line={line} language={language} />
          </div>
        );
      })}
    </>
  );
}

/** Pairs each run of removed lines with the added lines that follow it. */
function splitRows(lines: DiffLine[]) {
  const rows: { left?: number; right?: number }[] = [];
  let i = 0;
  while (i < lines.length) {
    if (lines[i].kind === "context") {
      rows.push({ left: i, right: i });
      i++;
      continue;
    }
    const removed: number[] = [];
    const added: number[] = [];
    while (i < lines.length && lines[i].kind === "removed") removed.push(i++);
    while (i < lines.length && lines[i].kind === "added") added.push(i++);
    for (let k = 0; k < Math.max(removed.length, added.length); k++) rows.push({ left: removed[k], right: added[k] });
  }
  return rows;
}

function SplitLines({ lines, hunk, language, picked, selectable, onToggle }: LinesProps) {
  const rows = useMemo(() => splitRows(lines), [lines]);
  const side = (index: number | undefined, which: "old" | "new") => {
    if (index === undefined) return <div className="split-side empty-side" />;
    const line = lines[index];
    const ref = { hunk, line: index };
    const canPick = selectable && changed(line);
    return (
      <div className={`split-side diff-line ${line.kind}${picked.has(key(ref)) ? " picked" : ""}`}>
        <span
          className={`gutter single${canPick ? " pickable" : ""}`}
          onMouseDown={(e) => canPick && (e.preventDefault(), onToggle(ref, e.shiftKey))}
        >
          <span className="ln">{which === "old" ? line.oldLine : line.newLine}</span>
        </span>
        <Code line={line} language={language} />
      </div>
    );
  };
  return (
    <>
      {rows.map((row, i) => (
        <div key={i} className="split-row">
          {side(row.left, "old")}
          {side(row.right, "new")}
        </div>
      ))}
    </>
  );
}

function FileHistory({ api, path, version, onShowCommit }: { api: RepoApi; path: string; version: number; onShowCommit: (id: string) => void }) {
  const [commits, setCommits] = useState<CommitInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.fileHistory(path, 500).then(setCommits, (e) => setError(String(e)));
  }, [api, path, version]);
  if (error) return <p className="error pad">{error}</p>;
  if (!commits) return <p className="empty pad">Loading…</p>;
  if (!commits.length) return <p className="empty pad">No commits touch this file yet.</p>;
  return (
    <ul className="history-list">
      {commits.map((c) => (
        <li key={c.id}>
          <button onClick={() => onShowCommit(c.id)}>
            <span className="mono muted">{c.shortId}</span>
            <span className="history-summary">{c.summary}</span>
            <span className="muted">{c.authorName}</span>
            <span className="muted" title={fullDate(c.authorTime)}>
              {timeAgo(c.authorTime)}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

function Blame({
  api,
  path,
  version,
  language,
  onShowCommit,
}: {
  api: RepoApi;
  path: string;
  version: number;
  language: string | null;
  onShowCommit: (id: string) => void;
}) {
  const [lines, setLines] = useState<BlameLine[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setError(null);
    api.blame(path).then(setLines, (e) => setError(String(e)));
  }, [api, path, version]);
  if (error) return <p className="error pad">{error}</p>;
  if (!lines) return <p className="empty pad">Loading…</p>;
  return (
    <div className="diff-body blame">
      {lines.map((l, i) => {
        const first = i === 0 || lines[i - 1].commit !== l.commit;
        const uncommitted = /^0+$/.test(l.commit);
        return (
          <Fragment key={i}>
            <div className={`blame-line${first ? " group-start" : ""}`}>
              <span className="blame-info" title={first ? `${l.summary}\n${l.authorName}, ${fullDate(l.authorTime)}` : undefined}>
                {first &&
                  (uncommitted ? (
                    <span className="muted">Not committed yet</span>
                  ) : (
                    <button className="link" onClick={() => onShowCommit(l.commit)}>
                      <span className="mono">{l.commit.slice(0, 7)}</span> {l.authorName} · {timeAgo(l.authorTime)}
                    </button>
                  ))}
              </span>
              <span className="ln">{l.line}</span>
              <Code line={{ kind: "context", oldLine: null, newLine: l.line, text: l.text, noNewline: false }} language={language} />
            </div>
          </Fragment>
        );
      })}
    </div>
  );
}
