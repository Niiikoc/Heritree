import { useEffect, useState } from "react";
import type { RepoApi } from "../api";
import type { CommitDetails, CommitInfo, RefInfo } from "../types";
import { fullDate } from "../format";
import type { DiffTarget } from "./DiffView";
import { FileRow } from "./FileRow";

interface Props {
  api: RepoApi;
  commit: CommitInfo;
  refs: RefInfo[];
  /** Branches created from this commit, from lineage. */
  forks: string[];
  version: number;
  diffTarget: DiffTarget | null;
  onOpenDiff: (target: DiffTarget) => void;
  onSelectCommit: (id: string) => void;
}

/** Details of the selected commit: message, people, parents, files. */
export function CommitPanel({ api, commit, refs, forks, version, diffTarget, onOpenDiff, onSelectCommit }: Props) {
  const [details, setDetails] = useState<CommitDetails | null>(null);
  const [containing, setContaining] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setDetails(null);
    setContaining(null);
    setError(null);
    api.commitDetails(commit.id).then(
      (d) => live && setDetails(d),
      (e) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [api, commit.id, version]);

  const body = details?.message.split("\n").slice(1).join("\n").trim();
  const committedSeparately =
    details && (details.committerName !== commit.authorName || details.commitTime !== commit.authorTime);

  return (
    <aside className="details">
      <h2 className="commit-title">{commit.summary}</h2>
      {body && <pre className="commit-body">{body}</pre>}
      {(refs.length > 0 || forks.length > 0) && (
        <div className="badges">
          {refs.map((r) => (
            <span key={r.name} className={`ref-badge ref-${r.kind}`}>
              {r.shortName}
            </span>
          ))}
          {forks.map((f) => (
            <span key={f} className="ref-badge fork-badge" title={`${f} was created from here`}>
              ⑂ {f}
            </span>
          ))}
        </div>
      )}
      <dl className="meta">
        <dt>Commit</dt>
        <dd className="mono">{commit.id}</dd>
        <dt>Author</dt>
        <dd>
          {commit.authorName} <span className="muted">&lt;{commit.authorEmail}&gt;</span>
          <br />
          <span className="muted">{fullDate(commit.authorTime)}</span>
        </dd>
        {committedSeparately && (
          <>
            <dt>Committer</dt>
            <dd>
              {details.committerName}
              <br />
              <span className="muted">{fullDate(details.commitTime)}</span>
            </dd>
          </>
        )}
        <dt>{commit.parents.length > 1 ? "Parents" : "Parent"}</dt>
        <dd className="mono">
          {commit.parents.length
            ? commit.parents.map((p) => (
                <button key={p} className="link mono" onClick={() => onSelectCommit(p)}>
                  {p.slice(0, 7)}
                </button>
              ))
            : "none (root commit)"}
        </dd>
      </dl>

      <section className="file-group">
        <h3>
          Branches with this commit
          {containing === null && (
            <span className="group-actions">
              <button className="small" onClick={() => api.branchesContaining(commit.id).then(setContaining, (e) => setError(String(e)))}>
                Find
              </button>
            </span>
          )}
        </h3>
        {containing && (containing.length ? <p className="branch-list">{containing.join(", ")}</p> : <p className="empty">No branch has it.</p>)}
      </section>

      <section className="file-group">
        <h3>
          Files changed <span className="count">{details?.files.length ?? ""}</span>
          {commit.parents.length > 1 && <span className="muted small-note"> compared with the first parent</span>}
        </h3>
        {error && <p className="error">{error}</p>}
        {!details && !error && <p className="empty">Loading…</p>}
        <ul>
          {details?.files.map((f) => (
            <FileRow
              key={f.path}
              path={f.path}
              originalPath={f.originalPath}
              kind={f.kind}
              active={diffTarget?.kind === "commit" && diffTarget.id === commit.id && diffTarget.path === f.path}
              onOpen={() => onOpenDiff({ kind: "commit", id: commit.id, path: f.path, oldPath: f.originalPath })}
            />
          ))}
        </ul>
      </section>
    </aside>
  );
}
