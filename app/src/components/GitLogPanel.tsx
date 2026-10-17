import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import type { CommandRecord } from "../types";

interface Props {
  /** Bumped after each action, to reload. */
  version: number;
  onClose: () => void;
}

/** Every git command Heritree ran for you, so nothing happens behind your back. */
export function GitLogPanel({ version, onClose }: Props) {
  const [records, setRecords] = useState<CommandRecord[]>([]);
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.commandLog().then(setRecords, () => {});
  }, [version]);

  useEffect(() => end.current?.scrollIntoView({ block: "end" }), [records]);

  return (
    <section className="git-log" aria-label="Git commands">
      <header>
        <strong>Git log</strong>
        <span className="muted"> every command Heritree ran, newest last</span>
        <div className="spacer" />
        <button className="small" onClick={() => navigator.clipboard.writeText(records.map((r) => r.command).join("\n"))}>
          Copy all
        </button>
        <button className="small" onClick={onClose} aria-label="Close">
          ✕
        </button>
      </header>
      <div className="git-log-body">
        {records.length === 0 && <p className="empty">Nothing yet. Commands appear here as you stage, commit, push…</p>}
        {records.map((r, i) => (
          <div key={i} className={`git-log-row${r.ok ? "" : " failed"}`}>
            <span className="muted">{new Date(r.time * 1000).toLocaleTimeString()}</span>
            <code>{r.command}</code>
            <span className="muted">{r.durationMs} ms</span>
            {r.error && <pre className="error">{r.error}</pre>}
          </div>
        ))}
        <div ref={end} />
      </div>
    </section>
  );
}
