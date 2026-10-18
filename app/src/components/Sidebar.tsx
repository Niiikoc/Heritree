import { type DragEvent, useMemo, useState } from "react";
import type { BranchLineage, LineageSource, OpenPullRequest, RefInfo, StashEntry } from "../types";
import type { MenuItem, OpenMenu } from "../ui";
import { timeAgo } from "../format";

export type BranchView = "tree" | "folders";

/** What can be dragged onto a branch. */
export type DragPayload = { type: "ref"; name: string; kind: RefInfo["kind"] } | { type: "commit"; id: string };
export const DRAG_TYPE = "application/x-heritree";

export function readDrag(e: DragEvent): DragPayload | null {
  try {
    return JSON.parse(e.dataTransfer.getData(DRAG_TYPE));
  } catch {
    return null;
  }
}

const SOURCE_TEXT: Record<LineageSource, string> = {
  recorded: "recorded by Heritree when the branch was created",
  reflog: "read from the branch's reflog",
  upstream: "taken from the branch's upstream",
  convention: "guessed from naming conventions",
};

const SOURCE_MARK: Record<LineageSource, string> = {
  recorded: "●",
  reflog: "◐",
  upstream: "↥",
  convention: "○",
};

interface Props {
  refs: RefInfo[];
  currentBranch: string | null;
  lineage: BranchLineage[] | null;
  stashes: StashEntry[];
  view: BranchView;
  onViewChange: (view: BranchView) => void;
  /** Select the commit a ref points at. */
  onPick: (ref: RefInfo) => void;
  /** Double-click: check out. */
  onActivate: (ref: RefInfo) => void;
  onPickStash: (stash: StashEntry) => void;
  menuFor: (ref: RefInfo) => MenuItem[];
  stashMenu: (stash: StashEntry) => MenuItem[];
  openMenu: OpenMenu;
  /** Something was dropped on the current branch. */
  onDrop: (payload: DragPayload, x: number, y: number) => void;
  /** Open pull requests by the local branch they come from. */
  pulls: Map<string, OpenPullRequest[]>;
  onOpenPull: (pull: OpenPullRequest) => void;
}

export function Sidebar(props: Props) {
  const { refs, lineage, stashes, view, onViewChange, onPickStash, stashMenu, openMenu } = props;
  const locals = refs.filter((r) => r.kind === "localBranch");
  const remotes = refs.filter((r) => r.kind === "remoteBranch");
  const tags = refs.filter((r) => r.kind === "tag");

  const remoteGroups = useMemo(() => groupBy(remotes, (r) => r.shortName.split("/")[0]), [remotes]);

  return (
    <nav className="sidebar" aria-label="References">
      <section>
        <h2>
          Branches <span className="count">{locals.length}</span>
          <span className="segmented tiny">
            <button
              className={view === "tree" ? "active" : undefined}
              onClick={() => onViewChange("tree")}
              title="Show which branch each branch came from"
            >
              Lineage
            </button>
            <button className={view === "folders" ? "active" : undefined} onClick={() => onViewChange("folders")}>
              Folders
            </button>
          </span>
        </h2>
        {locals.length === 0 && <p className="empty">None yet</p>}
        {view === "tree" ? (
          <LineageTree {...props} locals={locals} />
        ) : (
          <FolderList {...props} items={locals} strip="" />
        )}
      </section>

      <section>
        <h2>
          Remotes <span className="count">{remotes.length}</span>
        </h2>
        {remotes.length === 0 && <p className="empty">None</p>}
        {[...remoteGroups].map(([remote, items]) => (
          <Folder key={remote} name={remote} depth={0} defaultOpen>
            <FolderList {...props} items={items} strip={`${remote}/`} depth={1} />
          </Folder>
        ))}
      </section>

      <section>
        <h2>
          Tags <span className="count">{tags.length}</span>
        </h2>
        {tags.length === 0 && <p className="empty">None</p>}
        <ul>
          {tags.map((r) => (
            <RefItem key={r.name} {...props} item={r} label={r.shortName} depth={0} current={false} />
          ))}
        </ul>
      </section>

      <section>
        <h2>
          Stashes <span className="count">{stashes.length}</span>
        </h2>
        {stashes.length === 0 && <p className="empty">None</p>}
        <ul>
          {stashes.map((s) => (
            <li key={s.id}>
              <button
                onClick={() => onPickStash(s)}
                onContextMenu={(e) => openMenu(e, stashMenu(s))}
                title={`${s.message}\n${timeAgo(s.time)}`}
              >
                <span className="stash-icon">≡</span>
                <span className="ref-label">{s.message.replace(/^On [^:]+: /, "").replace(/^WIP on [^:]+: /, "WIP: ")}</span>
              </button>
            </li>
          ))}
        </ul>
      </section>
      {view === "tree" && lineage && (
        <p className="legend" title="How Heritree knows where a branch came from">
          ● recorded · ◐ reflog · ↥ upstream · ○ guessed
        </p>
      )}
    </nav>
  );
}

function RefItem({
  item,
  label,
  depth,
  current,
  extra,
  title,
  onPick,
  onActivate,
  menuFor,
  openMenu,
  onDrop,
  pulls,
  onOpenPull,
}: Props & { item: RefInfo; label: string; depth: number; current: boolean; extra?: React.ReactNode; title?: string }) {
  const [over, setOver] = useState(false);
  const accepts = current; // drops only ever change the checked-out branch
  const prs = item.kind === "localBranch" ? (pulls.get(item.shortName) ?? []) : [];
  const prText = prs.map((p) => `\nPull request #${p.number} into ${p.base}${p.draft ? " (draft)" : ""}: ${p.title}`).join("");
  return (
    <li>
      <button
        className={`${current ? "current" : ""}${over ? " drop-over" : ""}`}
        style={{ paddingLeft: 20 + depth * 14 }}
        onClick={() => onPick(item)}
        onDoubleClick={() => onActivate(item)}
        onContextMenu={(e) => openMenu(e, menuFor(item))}
        title={(title ?? `${item.name}\n${item.target.slice(0, 7)}\nDouble-click to check out`) + prText}
        draggable={item.kind !== "tag"}
        onDragStart={(e) => {
          e.dataTransfer.setData(DRAG_TYPE, JSON.stringify({ type: "ref", name: item.shortName, kind: item.kind }));
          e.dataTransfer.effectAllowed = "copy";
        }}
        onDragOver={(e) => {
          if (!accepts || !e.dataTransfer.types.includes(DRAG_TYPE)) return;
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          setOver(false);
          const payload = readDrag(e);
          if (accepts && payload) onDrop(payload, e.clientX, e.clientY);
        }}
      >
        {current && <span className="current-dot" aria-label="current branch" />}
        <span className="ref-label">{label}</span>
        {prs.map((p) => (
          <span
            key={p.number}
            className={`pr-badge${p.draft ? " draft" : ""}`}
            role="link"
            title={`Open pull request #${p.number} into ${p.base}`}
            onClick={(e) => {
              e.stopPropagation();
              onOpenPull(p);
            }}
            onDoubleClick={(e) => e.stopPropagation()}
          >
            #{p.number}
          </span>
        ))}
        {extra}
      </button>
    </li>
  );
}

function Folder({ name, depth, defaultOpen, children }: { name: string; depth: number; defaultOpen?: boolean; children: React.ReactNode }) {
  const [open, setOpen] = useState(!!defaultOpen);
  return (
    <>
      <button className="folder" style={{ paddingLeft: 8 + depth * 14 }} onClick={() => setOpen(!open)} aria-expanded={open}>
        <span className="chevron">{open ? "▾" : "▸"}</span>
        {name}
      </button>
      {open && children}
    </>
  );
}

/** Branches grouped by their first path segment: `feature/`, `release/` … */
function FolderList(props: Props & { items: RefInfo[]; strip: string; depth?: number }) {
  const { items, strip, currentBranch, depth = 0 } = props;
  const groups = groupBy(items, (r) => {
    const rest = r.shortName.slice(strip.length);
    return rest.includes("/") ? rest.split("/")[0] : "";
  });
  const isCurrent = (r: RefInfo) => r.kind === "localBranch" && r.shortName === currentBranch;
  return (
    <ul>
      {(groups.get("") ?? []).map((r) => (
        <RefItem key={r.name} {...props} item={r} label={r.shortName.slice(strip.length)} depth={depth} current={isCurrent(r)} />
      ))}
      {[...groups]
        .filter(([folder]) => folder)
        .map(([folder, members]) => (
          <li key={folder}>
            <Folder name={`${folder}/`} depth={depth} defaultOpen={members.some(isCurrent)}>
              <ul>
                {members.map((r) => (
                  <RefItem
                    key={r.name}
                    {...props}
                    item={r}
                    label={r.shortName.slice(strip.length + folder.length + 1)}
                    depth={depth + 1}
                    current={isCurrent(r)}
                  />
                ))}
              </ul>
            </Folder>
          </li>
        ))}
    </ul>
  );
}

/** Branches nested under the branch they were created from. */
function LineageTree(props: Props & { locals: RefInfo[] }) {
  const { locals, lineage, currentBranch } = props;
  if (!lineage) return <p className="empty">Working out lineage…</p>;

  const byName = new Map(lineage.map((l) => [l.name, l]));
  const children = groupBy(lineage, (l) => (l.parent && byName.has(l.parent) ? l.parent : ""));
  const refFor = new Map(locals.map((r) => [r.shortName, r]));

  const render = (entry: BranchLineage, depth: number): React.ReactNode => {
    const ref = refFor.get(entry.name);
    if (!ref) return null;
    const kids = children.get(entry.name) ?? [];
    const how = entry.source ? SOURCE_TEXT[entry.source] : "";
    const title = entry.parent
      ? `${entry.name}\nCreated from ${entry.parent}${entry.forkPoint ? ` at ${entry.forkPoint.slice(0, 7)}` : ""}${
          entry.forkTime ? `, ${timeAgo(entry.forkTime)}` : ""
        }\n${entry.ahead} ahead, ${entry.behind} behind ${entry.parent} (${how})`
      : `${entry.name}\nA root branch\nDouble-click to check out`;
    return (
      <li key={entry.name} className="tree-node">
        <ul>
          <RefItem
            {...props}
            item={ref}
            label={entry.name}
            depth={depth}
            current={entry.name === currentBranch}
            title={title}
            extra={
              <>
                {entry.source && <span className={`lineage-mark source-${entry.source}`}>{SOURCE_MARK[entry.source]}</span>}
                {entry.parent && (entry.ahead > 0 || entry.behind > 0) && (
                  <span className="ahead-behind">
                    {entry.ahead > 0 && `↑${entry.ahead}`}
                    {entry.behind > 0 && ` ↓${entry.behind}`}
                  </span>
                )}
              </>
            }
          />
        </ul>
        {kids.length > 0 && <ul className="tree-children">{kids.map((k) => render(k, depth + 1))}</ul>}
      </li>
    );
  };

  const roots = children.get("") ?? [];
  // Branches whose parent is a remote branch hang under that remote name.
  const external = groupBy(
    roots.filter((r) => r.parent),
    (r) => r.parent!,
  );
  return (
    <ul className="lineage-tree">
      {roots.filter((r) => !r.parent).map((r) => render(r, 0))}
      {[...external].map(([parent, members]) => (
        <li key={parent}>
          <span className="external-parent" title={`${parent} is not a local branch`}>
            {parent}
          </span>
          <ul>{members.map((m) => render(m, 1))}</ul>
        </li>
      ))}
    </ul>
  );
}

function groupBy<T>(items: T[], key: (item: T) => string): Map<string, T[]> {
  const map = new Map<string, T[]>();
  for (const item of items) {
    const k = key(item);
    map.set(k, [...(map.get(k) ?? []), item]);
  }
  return map;
}
