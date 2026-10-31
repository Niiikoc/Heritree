import type { GraphRow } from "../types";

export const LANE_WIDTH = 16;
const DOT_RADIUS = 4;

/** Lane colours; the core hands out slots and we wrap around the palette. */
export function laneColor(slot: number): string {
  return `var(--lane-${slot % 8})`;
}

interface Props {
  row: GraphRow;
  height: number;
  /** Width of the graph column in lanes, so every row lines up. */
  lanes: number;
  isMerge: boolean;
  isHead: boolean;
}

/** Draws one row of the commit graph. See crates/heritree-core/src/graph.rs. */
export function GraphCell({ row, height, lanes, isMerge, isHead }: Props) {
  const x = (lane: number) => lane * LANE_WIDTH + LANE_WIDTH / 2;
  const mid = height / 2;

  // Straight down when the lane does not change, an S-curve when it does.
  const segment = (x1: number, y1: number, x2: number, y2: number) =>
    x1 === x2 ? `M${x1} ${y1}V${y2}` : `M${x1} ${y1}C${x1} ${(y1 + y2) / 2} ${x2} ${(y1 + y2) / 2} ${x2} ${y2}`;

  return (
    <svg className="graph-cell" width={lanes * LANE_WIDTH} height={height} aria-hidden="true">
      {row.edges.map((edge, i) => {
        const d =
          edge.kind === "in"
            ? segment(x(edge.from), 0, x(edge.to), mid)
            : edge.kind === "out"
              ? segment(x(edge.from), mid, x(edge.to), height)
              : segment(x(edge.from), 0, x(edge.to), height);
        return <path key={i} d={d} stroke={laneColor(edge.color)} strokeWidth={2} fill="none" />;
      })}
      <circle
        cx={x(row.lane)}
        cy={mid}
        r={isHead ? DOT_RADIUS + 1.5 : DOT_RADIUS}
        fill={isMerge ? "var(--bg)" : laneColor(row.color)}
        stroke={laneColor(row.color)}
        strokeWidth={isMerge || isHead ? 2 : 0}
      />
    </svg>
  );
}

interface WorktreeProps {
  /** The HEAD commit's row, which the uncommitted changes sit just above. */
  head: GraphRow;
  height: number;
  lanes: number;
}

/**
 * The "uncommitted changes" node: a hollow dot in HEAD's lane with a dashed
 * line down to HEAD. Lanes that enter HEAD's row from above pass straight by.
 */
export function WorktreeCell({ head, height, lanes }: WorktreeProps) {
  const x = (lane: number) => lane * LANE_WIDTH + LANE_WIDTH / 2;
  const mid = height / 2;
  const color = laneColor(head.color);

  return (
    <svg className="graph-cell" width={lanes * LANE_WIDTH} height={height} aria-hidden="true">
      {head.edges
        .filter((edge) => edge.kind !== "out")
        .map((edge, i) => (
          <path key={i} d={`M${x(edge.from)} 0V${height}`} stroke={laneColor(edge.color)} strokeWidth={2} fill="none" />
        ))}
      <path d={`M${x(head.lane)} ${mid}V${height}`} stroke={color} strokeWidth={2} strokeDasharray="3 3" fill="none" />
      <circle cx={x(head.lane)} cy={mid} r={DOT_RADIUS + 1} fill="var(--bg)" stroke={color} strokeWidth={2} strokeDasharray="2 2" />
    </svg>
  );
}
