import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { repoApi, unwatchRepository, watchRepository } from "./api";
import type { BranchLineage, CommitInfo, FileStatus, LogRow, RefInfo, RepoInfo, ResetMode, StashEntry, Undo } from "./types";
import { type Answers, type AskOptions, isTyping, type MenuItem, type OpenMenu } from "./ui";
import { Sidebar, type BranchView, type DragPayload } from "./components/Sidebar";
import { CommitList } from "./components/CommitList";
import { CommitPanel } from "./components/CommitPanel";
import { WorkingTreePanel } from "./components/WorkingTreePanel";
import { DiffView, type DiffTarget } from "./components/DiffView";
import { ContextMenu } from "./components/ContextMenu";
import { Dialog } from "./components/Dialog";
import { Palette, type PaletteCommand } from "./components/Palette";
import { GitLogPanel } from "./components/GitLogPanel";

/** Enough for everyday work; paging past it comes later. */
const LOG_LIMIT = 5000;
const UNDO_DEPTH = 50;

interface RepoData {
  info: RepoInfo;
  refs: RefInfo[];
  rows: LogRow[];
  status: FileStatus[];
  stashes: StashEntry[];
}

interface Props {
  path: string;
  /** Only the visible tab reacts to shortcuts and refreshes right away. */
  active: boolean;
  /** App-wide commands for the palette. */
  appCommands: PaletteCommand[];
  /** Opens the repository switcher. */
  onSwitchRepo: () => void;
}

function stored<T extends string>(key: string, fallback: T): T {
  try {
    return (localStorage.getItem(key) as T) || fallback;
  } catch {
    return fallback;
  }
}

function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Preferences only.
  }
}

/** One open repository: toolbar, sidebar, graph or diff, details. */
export function RepoView({ path, active, appCommands, onSwitchRepo }: Props) {
  const api = useMemo(() => repoApi(path), [path]);

  const [data, setData] = useState<RepoData | null>(null);
  const [lineage, setLineage] = useState<BranchLineage[] | null>(null);
  const [version, setVersion] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [extraCommit, setExtraCommit] = useState<CommitInfo | null>(null);
  const [scrollRequest, setScrollRequest] = useState(0);
  const [diffTarget, setDiffTarget] = useState<DiffTarget | null>(null);
  const [undos, setUndos] = useState<Undo[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [dialog, setDialog] = useState<(AskOptions & { resolve: (a: Answers | null) => void }) | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [showLog, setShowLog] = useState(() => stored<string>("heritree.showGitLog", "no") === "yes");
  const [query, setQuery] = useState("");
  const [branchView, setBranchView] = useState<BranchView>(() => stored<BranchView>("heritree.branchView", "tree"));
  const [pullRebase, setPullRebase] = useState(() => stored<string>("heritree.pullRebase", "no") === "yes");
  const searchRef = useRef<HTMLInputElement>(null);
  const lastError = useRef<string | null>(null);

  // ---- Loading -------------------------------------------------------------

  const refresh = useCallback(
    async (withLineage = true) => {
      try {
        const [info, refs, rows, status, stashes] = await Promise.all([
          api.info(),
          api.refs(),
          api.log(LOG_LIMIT),
          api.status().catch(() => [] as FileStatus[]), // bare repositories
          api.stashes().catch(() => [] as StashEntry[]),
        ]);
        setData({ info, refs, rows, status, stashes });
        setVersion((v) => v + 1);
        if (withLineage) api.lineage().then(setLineage, () => setLineage([]));
        return { info, rows, status };
      } catch (e) {
        setError(String(e));
        return null;
      }
    },
    [api],
  );

  useEffect(() => {
    refresh().then((d) => {
      if (d) setSelected(d.status.length > 0 ? null : (d.info.head.id ?? d.rows[0]?.commit.id ?? null));
    });
  }, [refresh]);

  // Pick up changes made outside the app (editor, terminal) as they happen.
  // A hidden tab only notes that it is out of date and catches up when shown.
  const stale = useRef(false);
  const activeRef = useRef(active);
  activeRef.current = active;
  useEffect(() => {
    watchRepository(path).catch(() => {});
    const unlisten = listen<string>("repo-changed", (e) => {
      if (e.payload !== path) return;
      if (activeRef.current) refresh();
      else stale.current = true;
    });
    const onFocus = () => activeRef.current && refresh(false);
    window.addEventListener("focus", onFocus);
    return () => {
      unwatchRepository(path).catch(() => {});
      unlisten.then((stop) => stop());
      window.removeEventListener("focus", onFocus);
    };
  }, [path, refresh]);

  useEffect(() => {
    if (active && stale.current) {
      stale.current = false;
      refresh();
    }
  }, [active, refresh]);

  // ---- Plumbing for actions --------------------------------------------------

  const run = useCallback(
    async (label: string, action: () => Promise<Undo | void>) => {
      setBusy(label);
      setError(null);
      lastError.current = null;
      try {
        const undo = await action();
        if (undo) setUndos((u) => [...u.slice(-(UNDO_DEPTH - 1)), undo]);
        return true;
      } catch (e) {
        lastError.current = String(e);
        setError(String(e));
        return false;
      } finally {
        await refresh();
        setBusy(null);
      }
    },
    [refresh],
  );

  const ask = useCallback(
    (options: AskOptions) => new Promise<Answers | null>((resolve) => setDialog({ ...options, resolve })),
    [],
  );

  const openMenu: OpenMenu = useCallback((e, items) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, items });
  }, []);

  const info = data?.info;
  const current = info && !info.head.detached ? info.head.branch : null;
  const currentLabel = current ?? "the detached HEAD";

  // ---- Remote --------------------------------------------------------------

  const doFetch = () => run("Fetching", () => api.fetch());
  const doPull = (rebase = pullRebase) => run(rebase ? "Pulling (rebase)" : "Pulling", () => api.pull(rebase));
  const doPush = async (force = false) => {
    if (force) {
      const ok = await ask({
        title: `Force push ${currentLabel}?`,
        message:
          "This replaces the remote branch with yours. It uses --force-with-lease, so it stops if someone else pushed in the meantime.",
        command: "git push --force-with-lease",
        confirm: "Force push",
        danger: true,
      });
      if (!ok) return;
    }
    const ok = await run(force ? "Force pushing" : "Pushing", () => api.push(force));
    if (!ok && !force && /rejected|non-fast-forward|fetch first/i.test(lastError.current ?? "")) {
      const choice = await ask({
        title: "The remote has commits you don't have",
        message: "Pull first to combine them with yours, or force push to replace them (with lease).",
        confirm: "Pull now",
      });
      if (choice) await doPull();
    }
  };

  // ---- Branches -------------------------------------------------------------

  const newBranch = async (start: string | null = null, startLabel?: string) => {
    const a = await ask({
      title: "New branch",
      message: `Starts from ${startLabel ?? start ?? currentLabel}. Heritree remembers where it came from.`,
      fields: [
        { name: "name", label: "Name", placeholder: "feature/my-change" },
        { name: "checkout", label: "Check it out", checkbox: true, checked: true },
      ],
      confirm: "Create branch",
    });
    if (!a) return;
    const name = String(a.name).trim();
    await run(`Creating ${name}`, () => api.createBranch(name, start, !!a.checkout));
  };

  const checkout = (ref: RefInfo) => {
    if (ref.kind === "localBranch") return run(`Checking out ${ref.shortName}`, () => api.checkoutBranch(ref.shortName));
    if (ref.kind === "remoteBranch")
      return run(`Checking out ${ref.shortName}`, () => api.checkoutRemoteBranch(ref.shortName));
    return checkoutCommit(ref.target, ref.shortName);
  };

  const checkoutCommit = async (id: string, label = id.slice(0, 7)) => {
    const ok = await ask({
      title: `Check out ${label}?`,
      message: "You will be on a detached HEAD: new commits belong to no branch until you create one.",
      command: `git checkout --detach ${id.slice(0, 7)}`,
      confirm: "Check out",
    });
    if (ok) await run(`Checking out ${label}`, () => api.checkoutCommit(id));
  };

  const merge = (name: string) => run(`Merging ${name} into ${currentLabel}`, () => api.merge(name));
  const rebase = async (onto: string) => {
    const ok = await ask({
      title: `Rebase ${currentLabel} onto ${onto}?`,
      message: `Your commits on ${currentLabel} are replayed on top of ${onto}. Undo puts the branch back.`,
      command: `git rebase ${onto}`,
      confirm: "Rebase",
    });
    if (ok) await run(`Rebasing onto ${onto}`, () => api.rebase(onto));
  };

  const renameBranch = async (name: string) => {
    const a = await ask({
      title: `Rename ${name}`,
      fields: [{ name: "to", label: "New name", value: name }],
      confirm: "Rename",
    });
    if (a && String(a.to).trim() !== name) await run("Renaming", () => api.renameBranch(name, String(a.to).trim()));
  };

  const deleteBranch = async (name: string) => {
    const ok = await ask({
      title: `Delete ${name}?`,
      message: "Only the local branch is deleted. You can undo this.",
      command: `git branch -d ${name}`,
      confirm: "Delete",
      danger: true,
    });
    if (!ok) return;
    const deleted = await run(`Deleting ${name}`, () => api.deleteBranch(name, false));
    if (!deleted && /not fully merged/i.test(lastError.current ?? "")) {
      const force = await ask({
        title: `${name} has unmerged commits`,
        message: "No other branch contains some of its commits. Delete it anyway? Undo can still bring it back.",
        command: `git branch -D ${name}`,
        confirm: "Delete anyway",
        danger: true,
      });
      if (force) await run(`Deleting ${name}`, () => api.deleteBranch(name, true));
    }
  };

  // ---- Commits ----------------------------------------------------------------

  const createTag = async (id: string) => {
    const a = await ask({
      title: `Tag ${id.slice(0, 7)}`,
      fields: [
        { name: "name", label: "Tag name", placeholder: "v1.0.0" },
        { name: "message", label: "Message (makes an annotated tag)", optional: true, multiline: true },
      ],
      confirm: "Create tag",
    });
    if (a) await run("Creating tag", () => api.createTag(String(a.name).trim(), id, String(a.message) || null));
  };

  const reset = async (id: string, mode: ResetMode) => {
    const lost =
      mode === "hard"
        ? "Uncommitted changes are thrown away (Heritree keeps a backup, so Undo brings them back)."
        : mode === "mixed"
          ? "Your files stay as they are; changes from the dropped commits become unstaged."
          : "Your files and staging stay as they are; changes from the dropped commits stay staged.";
    const ok = await ask({
      title: `Reset ${currentLabel} to ${id.slice(0, 7)} (${mode})?`,
      message: `Commits after ${id.slice(0, 7)} leave the branch. ${lost}`,
      command: `git reset --${mode} ${id.slice(0, 7)}`,
      confirm: "Reset",
      danger: mode === "hard",
    });
    if (ok) await run("Resetting", () => api.reset(id, mode));
  };

  const commitMenu = (commit: CommitInfo, e: React.MouseEvent) =>
    openMenu(e, [
      { label: "Create branch here…", hint: "Ctrl+B", onSelect: () => newBranch(commit.id, commit.shortId) },
      { label: "Create tag here…", onSelect: () => createTag(commit.id) },
      { label: "Check out this commit…", onSelect: () => checkoutCommit(commit.id) },
      "separator",
      { label: `Cherry-pick into ${currentLabel}`, onSelect: () => run("Cherry-picking", () => api.cherryPick(commit.id)) },
      { label: "Revert this commit", onSelect: () => run("Reverting", () => api.revert(commit.id)) },
      { label: `Reset ${currentLabel} here: soft…`, onSelect: () => reset(commit.id, "soft") },
      { label: `Reset ${currentLabel} here: mixed…`, onSelect: () => reset(commit.id, "mixed") },
      { label: `Reset ${currentLabel} here: hard…`, danger: true, onSelect: () => reset(commit.id, "hard") },
      "separator",
      { label: "Copy SHA", onSelect: () => navigator.clipboard.writeText(commit.id) },
      { label: "Copy message", onSelect: () => navigator.clipboard.writeText(commit.summary) },
    ]);

  const refMenu = (ref: RefInfo): MenuItem[] => {
    const isCurrent = ref.kind === "localBranch" && ref.shortName === current;
    if (ref.kind === "tag")
      return [
        { label: "Check out tag…", onSelect: () => checkoutCommit(ref.target, ref.shortName) },
        { label: "Create branch from tag…", onSelect: () => newBranch(ref.shortName) },
        { label: "Push tag", onSelect: () => run(`Pushing ${ref.shortName}`, () => api.pushTag(ref.shortName)) },
        "separator",
        { label: "Copy name", onSelect: () => navigator.clipboard.writeText(ref.shortName) },
        {
          label: "Delete tag…",
          danger: true,
          onSelect: async () => {
            const ok = await ask({
              title: `Delete tag ${ref.shortName}?`,
              message: "Only the local tag is deleted. You can undo this.",
              command: `git tag -d ${ref.shortName}`,
              confirm: "Delete",
              danger: true,
            });
            if (ok) await run("Deleting tag", () => api.deleteTag(ref.shortName));
          },
        },
      ];
    return [
      { label: isCurrent ? "Checked out" : "Check out", onSelect: isCurrent ? undefined : () => checkout(ref) },
      { label: "New branch from here…", onSelect: () => newBranch(ref.shortName) },
      "separator",
      { label: `Merge into ${currentLabel}`, onSelect: isCurrent || !current ? undefined : () => merge(ref.shortName) },
      { label: `Rebase ${currentLabel} onto this…`, onSelect: isCurrent || !current ? undefined : () => rebase(ref.shortName) },
      ...(isCurrent
        ? ([
            "separator",
            { label: "Push", hint: "Ctrl+Shift+P", onSelect: () => doPush() },
            { label: "Pull", hint: "Ctrl+Shift+L", onSelect: () => doPull() },
          ] as MenuItem[])
        : []),
      "separator",
      { label: "Copy name", onSelect: () => navigator.clipboard.writeText(ref.shortName) },
      ...(ref.kind === "localBranch"
        ? ([
            { label: "Rename…", onSelect: () => renameBranch(ref.shortName) },
            { label: "Delete…", danger: true, onSelect: isCurrent ? undefined : () => deleteBranch(ref.shortName) },
          ] as MenuItem[])
        : []),
    ];
  };

  // ---- Stash ------------------------------------------------------------------

  const stash = async () => {
    const a = await ask({
      title: "Stash changes",
      message: "Puts your uncommitted changes aside and cleans the working tree.",
      fields: [
        { name: "message", label: "Message", optional: true, placeholder: "What you were doing" },
        { name: "untracked", label: "Include new (untracked) files", checkbox: true, checked: true },
      ],
      confirm: "Stash",
    });
    if (a) await run("Stashing", () => api.stashPush(String(a.message) || null, !!a.untracked));
  };

  const stashMenu = (s: StashEntry): MenuItem[] => [
    { label: "Apply", onSelect: () => run("Applying stash", () => api.stashApply(s.index, false)) },
    { label: "Pop (apply and remove)", onSelect: () => run("Popping stash", () => api.stashApply(s.index, true)) },
    "separator",
    {
      label: "Drop…",
      danger: true,
      onSelect: async () => {
        const ok = await ask({
          title: "Drop this stash?",
          message: `"${s.message}" is removed. You can undo this.`,
          command: `git stash drop stash@{${s.index}}`,
          confirm: "Drop",
          danger: true,
        });
        if (ok) await run("Dropping stash", () => api.stashDrop(s.index));
      },
    },
  ];

  const pickStash = (s: StashEntry) => {
    setExtraCommit({
      id: s.id,
      shortId: s.id.slice(0, 7),
      parents: [],
      summary: s.message,
      authorName: "Stash",
      authorEmail: "",
      authorTime: s.time,
      commitTime: s.time,
    });
    setSelected(s.id);
    setDiffTarget(null);
  };

  // ---- Undo, drag and drop -------------------------------------------------------

  const undo = async () => {
    const last = undos.at(-1);
    if (!last) return;
    if (await run(`Undoing: ${last.label}`, () => api.undo(last.action))) setUndos((u) => u.slice(0, -1));
  };

  const onDrop = (payload: DragPayload, x: number, y: number) => {
    const at = { clientX: x, clientY: y, preventDefault() {} };
    if (payload.type === "commit") {
      openMenu(at, [
        { label: `Cherry-pick ${payload.id.slice(0, 7)} into ${currentLabel}`, onSelect: () => run("Cherry-picking", () => api.cherryPick(payload.id)) },
      ]);
    } else if (payload.name !== current) {
      openMenu(at, [
        { label: `Merge ${payload.name} into ${currentLabel}`, onSelect: () => merge(payload.name) },
        { label: `Rebase ${currentLabel} onto ${payload.name}…`, onSelect: () => rebase(payload.name) },
      ]);
    }
  };

  // ---- Keyboard ---------------------------------------------------------------------

  const shortcuts = useRef<(e: KeyboardEvent) => void>(() => {});
  shortcuts.current = (e: KeyboardEvent) => {
    if (!active || dialog || paletteOpen || document.querySelector(".modal-backdrop")) return;
    const mod = e.ctrlKey || e.metaKey;
    if (!mod) return;
    const k = e.key.toLowerCase();
    if (k === "k") setPaletteOpen(true);
    else if (k === "p" && e.shiftKey) doPush();
    else if (k === "l" && e.shiftKey) doPull();
    else if (k === "b" && !e.shiftKey) newBranch();
    else if (k === "f" && !e.shiftKey) searchRef.current?.focus();
    else if (k === "z" && !e.shiftKey && !isTyping(e.target)) undo();
    else return;
    e.preventDefault();
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => shortcuts.current(e);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // ---- Derived data -------------------------------------------------------------------

  const refsByCommit = useMemo(() => {
    const map = new Map<string, RefInfo[]>();
    for (const r of data?.refs ?? []) map.set(r.target, [...(map.get(r.target) ?? []), r]);
    return map;
  }, [data]);

  const forksByCommit = useMemo(() => {
    const map = new Map<string, string[]>();
    for (const l of lineage ?? [])
      if (l.forkPoint && l.ahead > 0) map.set(l.forkPoint, [...(map.get(l.forkPoint) ?? []), l.name]);
    return map;
  }, [lineage]);

  const authors = useMemo(() => {
    const seen = new Set<string>();
    for (const r of data?.rows.slice(0, 2000) ?? []) seen.add(`${r.commit.authorName} <${r.commit.authorEmail}>`);
    return [...seen];
  }, [data]);

  // Rebuilt every render so each command sees the latest state.
  const paletteCommands = ((): PaletteCommand[] => {
    if (!data) return appCommands;
    const cmds: PaletteCommand[] = [
      { id: "fetch", label: "Fetch", keywords: "remote update", run: doFetch },
      { id: "pull", label: "Pull", hint: "Ctrl+Shift+L", run: () => doPull(false) },
      { id: "pull-rebase", label: "Pull with rebase", run: () => doPull(true) },
      { id: "push", label: "Push", hint: "Ctrl+Shift+P", run: () => doPush(false) },
      { id: "force-push", label: "Force push (with lease)…", run: () => doPush(true) },
      { id: "branch", label: "New branch…", hint: "Ctrl+B", keywords: "create", run: () => newBranch() },
      { id: "stash", label: "Stash changes…", keywords: "save", run: stash },
      ...(data.stashes.length
        ? [{ id: "pop", label: "Pop latest stash", run: () => run("Popping stash", () => api.stashApply(0, true)) }]
        : []),
      ...(undos.length ? [{ id: "undo", label: `Undo: ${undos.at(-1)!.label}`, hint: "Ctrl+Z", run: undo }] : []),
      { id: "search", label: "Search commits", hint: "Ctrl+F", keywords: "find filter", run: () => searchRef.current?.focus() },
      { id: "log", label: showLog ? "Hide Git log" : "Show Git log", keywords: "commands history", run: () => toggleLog() },
      { id: "tree", label: "Show branches as lineage tree", run: () => changeView("tree") },
      { id: "folders", label: "Show branches as folders", run: () => changeView("folders") },
      { id: "refresh", label: "Refresh", keywords: "reload", run: () => refresh() },
    ];
    for (const r of data.refs) {
      if (r.kind === "tag") {
        cmds.push({ id: `go-${r.name}`, label: `Go to tag ${r.shortName}`, run: () => goTo(r) });
        continue;
      }
      if (r.shortName !== current)
        cmds.push({ id: `co-${r.name}`, label: `Check out ${r.shortName}`, keywords: "switch branch", run: () => checkout(r) });
      cmds.push({ id: `go-${r.name}`, label: `Go to ${r.shortName}`, keywords: "show select", run: () => goTo(r) });
      if (r.kind === "localBranch" && current && r.shortName !== current)
        cmds.push({ id: `merge-${r.name}`, label: `Merge ${r.shortName} into ${current}`, run: () => merge(r.shortName) });
    }
    return [...cmds, ...appCommands];
  })();

  function goTo(ref: RefInfo) {
    setSelected(ref.target);
    setDiffTarget(null);
    setExtraCommit(null);
    setScrollRequest((n) => n + 1);
  }

  function toggleLog() {
    setShowLog((s) => {
      store("heritree.showGitLog", s ? "no" : "yes");
      return !s;
    });
  }

  function changeView(view: BranchView) {
    setBranchView(view);
    store("heritree.branchView", view);
  }

  if (!data || !info) {
    return <div className="loading" hidden={!active}>{error ? <p className="error pad">{error}</p> : <p className="empty pad">Opening…</p>}</div>;
  }

  const { refs, rows, status, stashes } = data;
  const selectedCommit =
    selected === null ? null : (rows.find((r) => r.commit.id === selected)?.commit ?? (extraCommit?.id === selected ? extraCommit : null));
  const headLabel = info.head.detached ? `detached at ${info.head.id?.slice(0, 7)}` : (info.head.branch ?? "no branch");
  const upstream = info.upstream;
  const lastUndo = undos.at(-1);

  return (
    <div className={`app${showLog ? " with-log" : ""}`} hidden={!active}>
      <header className="toolbar">
        <button className="repo-switch" onClick={onSwitchRepo} title="Switch repository (Ctrl+P)">
          <strong>{info.name}</strong> <span className="chevron">▾</span>
        </button>
        <span className="branch-pill" title={upstream ? `Tracking ${upstream.name}` : "No upstream yet; Push sets it"}>
          ⎇ {headLabel}
          {upstream && (upstream.ahead > 0 || upstream.behind > 0) && (
            <span className="ahead-behind">
              {upstream.ahead > 0 && ` ↑${upstream.ahead}`}
              {upstream.behind > 0 && ` ↓${upstream.behind}`}
            </span>
          )}
        </span>
        <div className="toolbar-group">
          <button onClick={doFetch} disabled={!!busy} title="Download new commits from all remotes">
            Fetch
          </button>
          <span className="split-button">
            <button onClick={() => doPull()} disabled={!!busy} title={`Pull ${pullRebase ? "with rebase" : "with merge"} (Ctrl+Shift+L)`}>
              Pull{upstream?.behind ? ` ${upstream.behind}` : ""}
            </button>
            <button
              className="split-arrow"
              aria-label="Pull options"
              disabled={!!busy}
              onClick={(e) =>
                openMenu(e, [
                  {
                    label: `${pullRebase ? "" : "✓ "}Merge when pulling`,
                    onSelect: () => (setPullRebase(false), store("heritree.pullRebase", "no")),
                  },
                  {
                    label: `${pullRebase ? "✓ " : ""}Rebase when pulling`,
                    onSelect: () => (setPullRebase(true), store("heritree.pullRebase", "yes")),
                  },
                ])
              }
            >
              ▾
            </button>
          </span>
          <span className="split-button">
            <button onClick={() => doPush()} disabled={!!busy || !current} title="Push (Ctrl+Shift+P)">
              Push{upstream?.ahead ? ` ${upstream.ahead}` : ""}
            </button>
            <button
              className="split-arrow"
              aria-label="Push options"
              disabled={!!busy || !current}
              onClick={(e) => openMenu(e, [{ label: "Force push (with lease)…", danger: true, onSelect: () => doPush(true) }])}
            >
              ▾
            </button>
          </span>
        </div>
        <div className="toolbar-group">
          <button onClick={() => newBranch()} disabled={!!busy} title="New branch (Ctrl+B)">
            Branch
          </button>
          <button onClick={stash} disabled={!!busy || status.length === 0} title="Stash uncommitted changes">
            Stash
          </button>
          <button
            onClick={() => run("Popping stash", () => api.stashApply(0, true))}
            disabled={!!busy || stashes.length === 0}
            title="Apply and remove the latest stash"
          >
            Pop
          </button>
          <button onClick={undo} disabled={!!busy || !lastUndo} title={lastUndo ? `Undo: ${lastUndo.label} (Ctrl+Z)` : "Nothing to undo"}>
            Undo
          </button>
        </div>
        <div className="spacer" />
        <input
          ref={searchRef}
          className="search"
          type="search"
          placeholder="Search message, author, SHA (Ctrl+F)"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setDiffTarget(null);
          }}
          onKeyDown={(e) => e.key === "Escape" && setQuery("")}
        />
        <button onClick={() => setPaletteOpen(true)} title="Command palette (Ctrl+K)">
          ⌘ K
        </button>
        <button className={showLog ? "active" : undefined} onClick={toggleLog} title="Show the git commands Heritree ran">
          Git log
        </button>
      </header>

      <Sidebar
        refs={refs}
        currentBranch={current}
        lineage={lineage}
        stashes={stashes}
        view={branchView}
        onViewChange={changeView}
        onPick={goTo}
        onActivate={(r) => {
          if (r.kind === "localBranch" && r.shortName === current) return;
          checkout(r);
        }}
        onPickStash={pickStash}
        menuFor={refMenu}
        stashMenu={stashMenu}
        openMenu={openMenu}
        onDrop={onDrop}
      />

      {diffTarget ? (
        <DiffView
          api={api}
          workdir={info.workdir}
          target={diffTarget}
          version={version}
          run={run}
          ask={ask}
          onClose={() => setDiffTarget(null)}
          onShowCommit={(id) => {
            setDiffTarget(null);
            setSelected(id);
            setScrollRequest((n) => n + 1);
          }}
        />
      ) : (
        <CommitList
          rows={rows}
          refsByCommit={refsByCommit}
          headId={info.head.id}
          changeCount={status.length}
          selectedId={selected}
          onSelect={(id) => {
            setSelected(id);
            setExtraCommit(null);
          }}
          scrollRequest={scrollRequest}
          forksByCommit={forksByCommit}
          query={query}
          onContextMenu={commitMenu}
        />
      )}

      {selected === null ? (
        <WorkingTreePanel
          api={api}
          info={info}
          files={status}
          authors={authors}
          diffTarget={diffTarget}
          onOpenDiff={setDiffTarget}
          run={run}
          ask={ask}
          openMenu={openMenu}
        />
      ) : selectedCommit ? (
        <CommitPanel
          api={api}
          commit={selectedCommit}
          refs={refsByCommit.get(selectedCommit.id) ?? []}
          forks={forksByCommit.get(selectedCommit.id) ?? []}
          version={version}
          diffTarget={diffTarget}
          onOpenDiff={setDiffTarget}
          onSelectCommit={(id) => {
            setSelected(id);
            setScrollRequest((n) => n + 1);
          }}
        />
      ) : (
        <aside className="details">
          <p className="empty">This commit is older than the {LOG_LIMIT.toLocaleString()} loaded ones.</p>
        </aside>
      )}

      {showLog && <GitLogPanel version={version} onClose={toggleLog} />}

      <footer className="statusbar">
        <span>{info.workdir ?? info.gitDir}</span>
        {busy && <span className="busy">{busy}…</span>}
        <span>
          {rows.length.toLocaleString()} commits{rows.length === LOG_LIMIT ? " (newest only)" : ""}
        </span>
      </footer>

      {error && (
        <div className="toast" role="alert">
          <pre>{error}</pre>
          <button className="small" onClick={() => setError(null)} aria-label="Dismiss">
            ✕
          </button>
        </div>
      )}
      {menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />}
      {dialog && (
        <Dialog
          {...dialog}
          onDone={(answers) => {
            dialog.resolve(answers);
            setDialog(null);
          }}
        />
      )}
      {paletteOpen && <Palette commands={paletteCommands} onClose={() => setPaletteOpen(false)} />}
    </div>
  );
}
