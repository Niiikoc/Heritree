import { useEffect, useMemo, useRef, useState } from "react";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import type { RepoApi } from "../api";
import type { FileStatus, RepoInfo, RepoState } from "../types";
import type { Ask, OpenMenu, Run } from "../ui";
import type { DiffTarget } from "./DiffView";
import { FileRow } from "./FileRow";

const STATE_LABEL: Record<RepoState, string> = {
  merging: "Merge",
  rebasing: "Rebase",
  cherryPicking: "Cherry-pick",
  reverting: "Revert",
};

interface Props {
  api: RepoApi;
  info: RepoInfo;
  files: FileStatus[];
  /** People seen in the log, for co-author suggestions. */
  authors: string[];
  diffTarget: DiffTarget | null;
  onOpenDiff: (target: DiffTarget) => void;
  run: Run;
  ask: Ask;
  openMenu: OpenMenu;
}

/** Staging area and commit box: the right panel when "Uncommitted changes" is selected. */
export function WorkingTreePanel({ api, info, files, authors, diffTarget, onOpenDiff, run, ask, openMenu }: Props) {
  const conflicted = files.filter((f) => f.staged === "conflicted");
  const unstaged = files.filter((f) => f.unstaged && f.staged !== "conflicted");
  const staged = files.filter((f) => f.staged && f.staged !== "conflicted");

  const [summary, setSummary] = useState("");
  const [body, setBody] = useState("");
  const [amend, setAmend] = useState(false);
  const [coAuthor, setCoAuthor] = useState("");
  const summaryRef = useRef<HTMLInputElement>(null);

  // Start from the repository's commit template, if it has one.
  useEffect(() => {
    api.commitTemplate().then((t) => {
      if (!t) return;
      const [first, ...rest] = t.split("\n").filter((l) => !l.startsWith("#"));
      setSummary((s) => s || first || "");
      setBody((b) => b || rest.join("\n").trim());
    }, () => {});
  }, [api]);

  // Amending starts from the last commit's message.
  const toggleAmend = async (on: boolean) => {
    setAmend(on);
    if (on && info.head.id && !summary.trim()) {
      const details = await api.commitDetails(info.head.id).catch(() => null);
      if (details) {
        const [first, ...rest] = details.message.split("\n");
        setSummary(first);
        setBody(rest.join("\n").trim());
      }
    }
  };

  const message = useMemo(() => {
    const trailer = coAuthor.trim() ? `\n\nCo-authored-by: ${coAuthor.trim()}` : "";
    return `${summary.trim()}${body.trim() ? `\n\n${body.trim()}` : ""}${trailer}`;
  }, [summary, body, coAuthor]);

  const canCommit = summary.trim().length > 0 && (staged.length > 0 || amend) && conflicted.length === 0;
  const commit = async () => {
    if (!canCommit) return;
    if (await run(amend ? "Amending" : "Committing", () => api.commit(message, amend))) {
      setSummary("");
      setBody("");
      setAmend(false);
      setCoAuthor("");
    }
  };

  const isOpen = (path: string, staged: boolean) =>
    diffTarget?.kind === "working" && diffTarget.path === path && diffTarget.staged === staged;

  const open = (f: FileStatus, staged: boolean) =>
    onOpenDiff({
      kind: "working",
      path: f.path,
      oldPath: staged ? f.originalPath : null,
      staged,
      untracked: !staged && f.unstaged === "untracked",
      conflicted: f.staged === "conflicted",
    });

  const discard = async (paths: string[]) => {
    const ok = await ask({
      title: paths.length === 1 ? `Discard changes to ${paths[0]}?` : `Discard changes to ${paths.length} files?`,
      message: "Unstaged changes are thrown away and new files are deleted. You can bring them back with Undo.",
      command: `git checkout -- ${paths.length === 1 ? paths[0] : "<files>"}`,
      confirm: "Discard",
      danger: true,
    });
    if (ok) await run("Discarding", () => api.discard(paths));
  };

  const fileMenu = (f: FileStatus, isStaged: boolean) => (e: React.MouseEvent) => {
    const abs = info.workdir ? `${info.workdir}/${f.path}` : null;
    openMenu(e, [
      isStaged
        ? { label: "Unstage", onSelect: () => run("Unstaging", () => api.unstage([f.path])) }
        : { label: "Stage", onSelect: () => run("Staging", () => api.stage([f.path])) },
      ...(isStaged ? [] : [{ label: "Discard changes…", danger: true, onSelect: () => discard([f.path]) }]),
      "separator",
      { label: "Open", onSelect: abs ? () => openPath(abs).catch(() => {}) : undefined },
      { label: "Show in folder", onSelect: abs ? () => revealItemInDir(abs).catch(() => {}) : undefined },
      { label: "Copy path", onSelect: () => navigator.clipboard.writeText(f.path) },
    ]);
  };

  const conflictMenu = (f: FileStatus) => (e: React.MouseEvent) =>
    openMenu(e, [
      { label: "Use ours (current branch)", onSelect: () => run("Resolving", () => api.resolveConflict(f.path, "ours")) },
      { label: "Use theirs (incoming)", onSelect: () => run("Resolving", () => api.resolveConflict(f.path, "theirs")) },
      { label: "Mark as resolved", onSelect: () => run("Resolving", () => api.stage([f.path])) },
      "separator",
      { label: "Open in editor", onSelect: info.workdir ? () => openPath(`${info.workdir}/${f.path}`).catch(() => {}) : undefined },
    ]);

  return (
    <aside className="details working-tree">
      {info.state && (
        <div className="banner">
          <strong>{STATE_LABEL[info.state]} in progress.</strong>{" "}
          {conflicted.length
            ? `Resolve ${conflicted.length} conflicted file${conflicted.length === 1 ? "" : "s"}, then continue.`
            : "All conflicts are resolved."}
          <div className="banner-actions">
            <button
              className="primary small"
              disabled={conflicted.length > 0}
              onClick={() => run("Continuing", () => api.continueOperation())}
            >
              Continue
            </button>
            <button onClick={() => run("Aborting", () => api.abortOperation())}>Abort</button>
          </div>
        </div>
      )}

      {conflicted.length > 0 && (
        <section className="file-group">
          <h3>
            Conflicts <span className="count">{conflicted.length}</span>
          </h3>
          <ul>
            {conflicted.map((f) => (
              <FileRow
                key={f.path}
                path={f.path}
                originalPath={null}
                kind="conflicted"
                active={isOpen(f.path, false)}
                onOpen={() => open(f, false)}
                onContextMenu={conflictMenu(f)}
                actions={
                  <button className="small" onClick={() => run("Resolving", () => api.stage([f.path]))} title="Mark as resolved">
                    ✓
                  </button>
                }
              />
            ))}
          </ul>
        </section>
      )}

      <section className="file-group">
        <h3>
          Unstaged <span className="count">{unstaged.length}</span>
          {unstaged.length > 0 && (
            <span className="group-actions">
              <button className="small danger-text" onClick={() => discard(unstaged.map((f) => f.path))}>
                Discard all
              </button>
              <button className="small" onClick={() => run("Staging", () => api.stage(unstaged.map((f) => f.path)))}>
                Stage all
              </button>
            </span>
          )}
        </h3>
        {unstaged.length === 0 ? (
          <p className="empty">{files.length === 0 ? "Nothing to commit, the working tree is clean." : "Nothing here"}</p>
        ) : (
          <ul>
            {unstaged.map((f) => (
              <FileRow
                key={f.path}
                path={f.path}
                originalPath={null}
                kind={f.unstaged!}
                active={isOpen(f.path, false)}
                onOpen={() => open(f, false)}
                onContextMenu={fileMenu(f, false)}
                actions={
                  <>
                    <button className="small" title="Discard changes" onClick={() => discard([f.path])}>
                      ↺
                    </button>
                    <button className="small" title="Stage" onClick={() => run("Staging", () => api.stage([f.path]))}>
                      +
                    </button>
                  </>
                }
              />
            ))}
          </ul>
        )}
      </section>

      <section className="file-group">
        <h3>
          Staged <span className="count">{staged.length}</span>
          {staged.length > 0 && (
            <span className="group-actions">
              <button className="small" onClick={() => run("Unstaging", () => api.unstage(staged.map((f) => f.path)))}>
                Unstage all
              </button>
            </span>
          )}
        </h3>
        {staged.length === 0 ? (
          <p className="empty">Stage files or lines to include them in the next commit.</p>
        ) : (
          <ul>
            {staged.map((f) => (
              <FileRow
                key={f.path}
                path={f.path}
                originalPath={f.originalPath}
                kind={f.staged!}
                active={isOpen(f.path, true)}
                onOpen={() => open(f, true)}
                onContextMenu={fileMenu(f, true)}
                actions={
                  <button className="small" title="Unstage" onClick={() => run("Unstaging", () => api.unstage([f.path]))}>
                    −
                  </button>
                }
              />
            ))}
          </ul>
        )}
      </section>

      <form
        className="commit-box"
        onSubmit={(e) => {
          e.preventDefault();
          commit();
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            commit();
          }
        }}
      >
        <div className="summary-field">
          <input
            ref={summaryRef}
            id="commit-summary"
            value={summary}
            onChange={(e) => setSummary(e.target.value)}
            placeholder="Summary (required)"
            spellCheck
            aria-label="Commit summary"
          />
          <span className={`char-count${summary.length > 72 ? " over" : ""}`} title="Keep the summary under 72 characters">
            {summary.length}
          </span>
        </div>
        <textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="Description"
          rows={4}
          aria-label="Commit description"
        />
        <input
          list="co-authors"
          value={coAuthor}
          onChange={(e) => setCoAuthor(e.target.value)}
          placeholder="Co-author: Name <email> (optional)"
          aria-label="Co-author"
        />
        <datalist id="co-authors">
          {authors.map((a) => (
            <option key={a} value={a} />
          ))}
        </datalist>
        <label className="checkbox">
          <input type="checkbox" checked={amend} onChange={(e) => toggleAmend(e.target.checked)} disabled={!info.head.id} />
          Amend last commit
        </label>
        <button type="submit" className="primary" disabled={!canCommit} title="Ctrl+Enter">
          {amend ? "Amend" : "Commit"} {staged.length > 0 && `${staged.length} file${staged.length === 1 ? "" : "s"} `}to{" "}
          {info.head.branch ?? "detached HEAD"}
        </button>
      </form>
    </aside>
  );
}
