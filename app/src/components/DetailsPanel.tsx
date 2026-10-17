import type { ChangeKind, CommitInfo, FileStatus, RefInfo } from "../types";
import { fullDate } from "../format";

const CHANGE_LABEL: Record<ChangeKind, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  typeChanged: "T",
  untracked: "U",
  conflicted: "!",
};

function FileList({ title, files, pick }: { title: string; files: FileStatus[]; pick: (f: FileStatus) => ChangeKind | null }) {
  return (
    <section className="file-group">
      <h3>
        {title} <span className="count">{files.length}</span>
      </h3>
      {files.length === 0 ? (
        <p className="empty">Nothing here</p>
      ) : (
        <ul>
          {files.map((f) => {
            const kind = pick(f)!;
            return (
              <li key={f.path} title={f.originalPath ? `${f.originalPath} → ${f.path}` : f.path}>
                <span className={`change change-${kind}`} aria-label={kind}>
                  {CHANGE_LABEL[kind]}
                </span>
                <span className="file-path">{f.path}</span>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

export function WorkingTreePanel({ files }: { files: FileStatus[] }) {
  const staged = files.filter((f) => f.staged);
  const unstaged = files.filter((f) => f.unstaged);
  return (
    <aside className="details">
      <h2>Working tree</h2>
      {files.length === 0 && <p className="empty">Nothing to commit, the working tree is clean.</p>}
      <FileList title="Unstaged" files={unstaged} pick={(f) => f.unstaged} />
      <FileList title="Staged" files={staged} pick={(f) => f.staged} />
      <p className="hint">Staging and committing arrive in the MVP.</p>
    </aside>
  );
}

export function CommitPanel({ commit, refs }: { commit: CommitInfo; refs: RefInfo[] }) {
  return (
    <aside className="details">
      <h2 className="commit-title">{commit.summary}</h2>
      {refs.length > 0 && (
        <div className="badges">
          {refs.map((r) => (
            <span key={r.name} className={`ref-badge ref-${r.kind}`}>
              {r.shortName}
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
        </dd>
        <dt>Date</dt>
        <dd>{fullDate(commit.authorTime)}</dd>
        <dt>{commit.parents.length > 1 ? "Parents" : "Parent"}</dt>
        <dd className="mono">{commit.parents.length ? commit.parents.map((p) => p.slice(0, 7)).join(", ") : "none (root commit)"}</dd>
      </dl>
    </aside>
  );
}
