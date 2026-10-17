// Mirrors the serde types in crates/heritree-core/src/model.rs and graph.rs.

export interface HeadInfo {
  branch: string | null;
  id: string | null;
  detached: boolean;
  unborn: boolean;
}

export interface RepoInfo {
  name: string;
  gitDir: string;
  workdir: string | null;
  head: HeadInfo;
}

export type RefKind = "localBranch" | "remoteBranch" | "tag";

export interface RefInfo {
  name: string;
  shortName: string;
  kind: RefKind;
  target: string;
}

export interface CommitInfo {
  id: string;
  shortId: string;
  parents: string[];
  summary: string;
  authorName: string;
  authorEmail: string;
  authorTime: number;
  commitTime: number;
}

export type EdgeKind = "in" | "out" | "through";

export interface Edge {
  from: number;
  to: number;
  kind: EdgeKind;
  color: number;
}

export interface GraphRow {
  lane: number;
  color: number;
  edges: Edge[];
  width: number;
}

export interface LogRow {
  commit: CommitInfo;
  graph: GraphRow;
}

export type ChangeKind =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "copied"
  | "typeChanged"
  | "untracked"
  | "conflicted";

export interface FileStatus {
  path: string;
  originalPath: string | null;
  staged: ChangeKind | null;
  unstaged: ChangeKind | null;
}
