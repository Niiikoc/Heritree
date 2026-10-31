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
  state: RepoState | null;
  upstream: BranchUpstream | null;
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

export type RepoState = "merging" | "rebasing" | "cherryPicking" | "reverting";

export interface BranchUpstream {
  name: string;
  ahead: number;
  behind: number;
}

export interface StashEntry {
  index: number;
  id: string;
  message: string;
  time: number;
}

export interface ChangedFile {
  path: string;
  originalPath: string | null;
  kind: ChangeKind;
}

export interface CommitDetails {
  id: string;
  message: string;
  committerName: string;
  committerEmail: string;
  commitTime: number;
  files: ChangedFile[];
}

export type LineKind = "context" | "added" | "removed";

export interface DiffLine {
  kind: LineKind;
  oldLine: number | null;
  newLine: number | null;
  text: string;
  noNewline: boolean;
}

export interface Hunk {
  header: string;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  oldPath: string | null;
  isNew: boolean;
  isDeleted: boolean;
  binary: boolean;
  hunks: Hunk[];
}

export interface LineRef {
  hunk: number;
  line: number;
}

export interface BlameLine {
  line: number;
  commit: string;
  authorName: string;
  authorTime: number;
  summary: string;
  text: string;
}

export type LineageSource = "recorded" | "reflog" | "upstream" | "convention";

export interface BranchLineage {
  name: string;
  parent: string | null;
  source: LineageSource | null;
  forkPoint: string | null;
  forkTime: number | null;
  ahead: number;
  behind: number;
}

export interface CommandRecord {
  dir: string;
  command: string;
  ok: boolean;
  error: string | null;
  time: number;
  durationMs: number;
}

export type ResetMode = "soft" | "mixed" | "hard";

/** Opaque to the UI: handed back to `undo` as-is. */
export interface UndoAction {
  kind: string;
  [key: string]: unknown;
}

export interface Undo {
  label: string;
  action: UndoAction;
}

export type HostKind = "github" | "gitlab" | "bitbucket" | "azuredevops" | "other";

export interface PullRequestDraft {
  branch: string;
  remote: string;
  /** `upstream` when the branch lives in a fork. */
  targetRemote: string;
  host: HostKind;
  webUrl: string;
  bases: string[];
  base: string | null;
  title: string;
  body: string;
  onRemote: boolean;
  unpushed: number;
  /** False when the browser finishes the pull request. */
  viaApi: boolean;
  /** Already open from this branch; their bases are left out of `bases`. */
  open: OpenPullRequest[];
}

export interface OpenPullRequest {
  number: number;
  title: string;
  url: string;
  branch: string;
  base: string;
  draft: boolean;
}

export interface NewPullRequest {
  branch: string;
  base: string;
  title: string;
  body: string;
  draft: boolean;
}

export interface PullRequest {
  url: string;
  number: number | null;
  outcome: "created" | "alreadyOpen" | "openInBrowser";
}

export type FileVersion =
  | { kind: "worktree"; path: string }
  | { kind: "index"; path: string }
  | { kind: "commit"; rev: string; path: string };
