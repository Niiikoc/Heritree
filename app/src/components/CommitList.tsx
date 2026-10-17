import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import type { LogRow, RefInfo } from "../types";
import { fullDate, timeAgo } from "../format";
import { GraphCell, LANE_WIDTH } from "./GraphCell";

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
}

/**
 * The commit list with its graph column. Only the visible rows are rendered,
 * so a repository with 100k commits scrolls as smoothly as one with 10.
 */
export function CommitList({ rows, refsByCommit, headId, changeCount, selectedId, onSelect, scrollRequest }: Props) {
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

  // Graph column width: widest row, at least two lanes.
  const lanes = useMemo(() => Math.max(2, ...rows.map((r) => r.graph.width)), [rows]);
  const offset = changeCount > 0 ? 1 : 0; // the "uncommitted changes" row
  const total = rows.length + offset;

  useEffect(() => {
    const el = scroller.current;
    if (!el || selectedId === null) return;
    const index = rows.findIndex((r) => r.commit.id === selectedId);
    if (index < 0) return;
    const top = (index + offset) * ROW_HEIGHT;
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
    const current = selectedId === null ? -1 : rows.findIndex((r) => r.commit.id === selectedId);
    const next = Math.min(rows.length - 1, Math.max(offset ? -1 : 0, current + (down ? 1 : -1)));
    onSelect(next < 0 ? null : rows[next].commit.id);
  };

  const items = [];
  for (let i = first; i < last; i++) {
    const style = { top: i * ROW_HEIGHT, height: ROW_HEIGHT };
    if (offset && i === 0) {
      items.push(
        <div
          key="worktree"
          className={`row worktree-row${selectedId === null ? " selected" : ""}`}
          style={style}
          onClick={() => onSelect(null)}
        >
          <div className="col-graph" style={{ width: lanes * LANE_WIDTH }} />
          <div className="col-summary">
            <span className="worktree-label">Uncommitted changes ({changeCount})</span>
          </div>
        </div>,
      );
      continue;
    }
    const { commit, graph } = rows[i - offset];
    const refs = refsByCommit.get(commit.id) ?? [];
    items.push(
      <div
        key={commit.id}
        className={`row${selectedId === commit.id ? " selected" : ""}`}
        style={style}
        onClick={() => onSelect(commit.id)}
      >
        <div className="col-graph" style={{ width: lanes * LANE_WIDTH }}>
          <GraphCell
            row={graph}
            height={ROW_HEIGHT}
            lanes={lanes}
            isMerge={commit.parents.length > 1}
            isHead={commit.id === headId}
          />
        </div>
        <div className="col-summary">
          {refs.map((r) => (
            <span key={r.name} className={`ref-badge ref-${r.kind}`} title={r.name}>
              {r.shortName}
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
      </div>
    </div>
  );
}
