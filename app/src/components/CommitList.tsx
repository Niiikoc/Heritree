import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import type { CommitInfo, LogRow, RefInfo } from "../types";
import { fullDate, timeAgo } from "../format";
import { GraphCell, LANE_WIDTH, WorktreeCell } from "./GraphCell";
import { DRAG_TYPE } from "./Sidebar";

export const ROW_HEIGHT = 28;
const OVERSCAN = 10;

interface Props {
  rows: LogRow[];
  refsByCommit: Map<string, RefInfo[]>;
  headId: string | null;
  changeCount: number;
  /** `null` selects the working tree. */
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  /** Bump to scroll `selectedId` into view. */
  scrollRequest: number;
  /** Branches created at each commit, from lineage. */
  forksByCommit: Map<string, string[]>;
  /** Search text; when set, only matching commits are listed (without the graph). */
  query: string;
  onContextMenu: (commit: CommitInfo, e: React.MouseEvent) => void;
}

function matches(commit: CommitInfo, query: string): boolean {
  const q = query.toLowerCase();
  return (
    commit.summary.toLowerCase().includes(q) ||
    commit.authorName.toLowerCase().includes(q) ||
    commit.authorEmail.toLowerCase().includes(q) ||
    commit.id.startsWith(q)
  );
}

/**
 * The commit list with its graph column. Only the visible rows are rendered,
 * so a repository with 100k commits scrolls as smoothly as one with 10.
 */
export function CommitList({
  rows: allRows,
  refsByCommit,
  headId,
  changeCount,
  selectedId,
  onSelect,
  scrollRequest,
  forksByCommit,
  query,
  onContextMenu,
}: Props) {
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(600);

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const observer = new ResizeObserver(() => setViewport(el.clientHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const filtering = query.trim().length > 0;
  const rows = useMemo(
    () => (filtering ? allRows.filter((r) => matches(r.commit, query.trim())) : allRows),
    [allRows, query, filtering],
  );
  // Graph column width: widest row, at least two lanes. Hidden while searching.
  const lanes = useMemo(() => (filtering ? 0 : Math.max(2, ...rows.map((r) => r.graph.width))), [rows, filtering]);
  // The "uncommitted changes" row sits just above HEAD, on its branch's lane;
  // at the top when HEAD is not loaded (unborn branch) or while searching.
  const headIndex = filtering ? -1 : rows.findIndex((r) => r.commit.id === headId);
  const wipAt = changeCount === 0 ? -1 : Math.max(0, headIndex);
  const total = rows.length + (wipAt >= 0 ? 1 : 0);
  /** Position on screen of the commit at `index` in `rows`. */
  const displayOf = (index: number) => (wipAt >= 0 && index >= wipAt ? index + 1 : index);
  /** Commit at a screen position, or `null` for the uncommitted changes row. */
  const commitAt = (position: number) =>
    position === wipAt ? null : rows[wipAt >= 0 && position > wipAt ? position - 1 : position];

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const index = selectedId === null ? wipAt : displayOf(rows.findIndex((r) => r.commit.id === selectedId));
    if (index < 0) return;
    const top = index * ROW_HEIGHT;
    if (top < el.scrollTop || top + ROW_HEIGHT > el.scrollTop + el.clientHeight) {
      el.scrollTop = top - el.clientHeight / 3;
    }
    // Only scroll when asked to (clicking a branch), not on every selection.
  }, [scrollRequest]);

  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(total, Math.ceil((scrollTop + viewport) / ROW_HEIGHT) + OVERSCAN);

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "j" && e.key !== "k") return;
    e.preventDefault();
    const down = e.key === "ArrowDown" || e.key === "j";
    const current = selectedId === null ? wipAt : displayOf(rows.findIndex((r) => r.commit.id === selectedId));
    const next = Math.min(total - 1, Math.max(0, current + (down ? 1 : -1)));
    if (next === current || total === 0) return;
    onSelect(commitAt(next)?.commit.id ?? null);
  };

  const items = [];
  for (let i = first; i < last; i++) {
    const style = { top: i * ROW_HEIGHT, height: ROW_HEIGHT };
    const row = commitAt(i);
    if (!row) {
      const head = headIndex >= 0 ? rows[headIndex] : null;
      items.push(
        <div
          key="worktree"
          className={`row worktree-row${selectedId === null ? " selected" : ""}`}
          style={style}
          onClick={() => onSelect(null)}
        >
          <div className="col-graph" style={{ width: lanes * LANE_WIDTH }}>
            {head && <WorktreeCell head={head.graph} height={ROW_HEIGHT} lanes={lanes} />}
          </div>
          <div className="col-summary">
            <span className="worktree-label">Uncommitted changes ({changeCount})</span>
          </div>
        </div>,
      );
      continue;
    }
    const { commit, graph } = row;
    const refs = refsByCommit.get(commit.id) ?? [];
    const forks = forksByCommit.get(commit.id) ?? [];
    items.push(
      <div
        key={commit.id}
        className={`row${selectedId === commit.id ? " selected" : ""}${commit.id === headId ? " head" : ""}`}
        style={style}
        onClick={() => onSelect(commit.id)}
        onContextMenu={(e) => {
          onSelect(commit.id);
          onContextMenu(commit, e);
        }}
        draggable
        onDragStart={(e) => {
          e.dataTransfer.setData(DRAG_TYPE, JSON.stringify({ type: "commit", id: commit.id }));
          e.dataTransfer.effectAllowed = "copy";
        }}
      >
        <div className="col-graph" style={{ width: lanes * LANE_WIDTH }}>
          {!filtering && (
            <GraphCell
              row={graph}
              height={ROW_HEIGHT}
              lanes={lanes}
              isMerge={commit.parents.length > 1}
              isHead={commit.id === headId}
            />
          )}
        </div>
        <div className="col-summary">
          {refs.map((r) => (
            <span key={r.name} className={`ref-badge ref-${r.kind}`} title={r.name}>
              {r.shortName}
            </span>
          ))}
          {forks.map((f) => (
            <span key={f} className="ref-badge fork-badge" title={`${f} was created from here`}>
              ⑂ {f}
            </span>
          ))}
          <span className="summary-text">{commit.summary}</span>
        </div>
        <div className="col-author" title={commit.authorEmail}>
          {commit.authorName}
        </div>
        <div className="col-date" title={fullDate(commit.authorTime)}>
          {timeAgo(commit.authorTime)}
        </div>
        <div className="col-sha">{commit.shortId}</div>
      </div>,
    );
  }

  return (
    <div className="commit-list">
      <div className="row header-row">
        <div className="col-graph" style={{ width: lanes * LANE_WIDTH }}>
          Graph
        </div>
        <div className="col-summary">Message</div>
        <div className="col-author">Author</div>
        <div className="col-date">Date</div>
        <div className="col-sha">Commit</div>
      </div>
      <div
        ref={scroller}
        className="commit-scroller"
        tabIndex={0}
        onKeyDown={onKeyDown}
        onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      >
        <div style={{ height: total * ROW_HEIGHT, position: "relative" }}>{items}</div>
        {filtering && rows.length === 0 && <p className="empty pad">No loaded commit matches “{query.trim()}”.</p>}
      </div>
    </div>
  );
}
