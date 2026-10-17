// Typed wrappers around the Tauri commands in src-tauri/src/lib.rs. Every
// call that works on a repository takes its path first.

import { invoke } from "@tauri-apps/api/core";
import type {
  BlameLine,
  BranchLineage,
  CommandRecord,
  CommitDetails,
  CommitInfo,
  FileDiff,
  FileStatus,
  LineRef,
  LogRow,
  RefInfo,
  RepoInfo,
  FileVersion,
  ResetMode,
  StashEntry,
  Undo,
  UndoAction,
} from "./types";

export const openRepository = (path: string) => invoke<RepoInfo>("open_repository", { path });
export const cloneRepository = (url: string, parent: string, name: string) =>
  invoke<string>("clone_repository", { url, parent, name });
export const initRepository = (path: string) => invoke<void>("init_repository", { path });
export const commandLog = () => invoke<CommandRecord[]>("command_log");
export const findRepositories = (root: string) => invoke<string[]>("find_repositories", { root });
/** Starts sending "repo-changed" events for `repo`; see src-tauri/src/watch.rs. */
export const watchRepository = (repo: string) => invoke<void>("watch_repository", { repo });
export const unwatchRepository = (repo: string) => invoke<void>("unwatch_repository", { repo });

/** All commands for one repository. */
export function repoApi(repo: string) {
  const call = <T>(command: string, args: Record<string, unknown> = {}) => invoke<T>(command, { repo, ...args });
  return {
    info: () => call<RepoInfo>("repo_info"),
    refs: () => call<RefInfo[]>("list_refs"),
    log: (limit: number) => call<LogRow[]>("load_log", { limit }),
    status: () => call<FileStatus[]>("working_tree_status"),
    fileDiff: (path: string, oldPath: string | null, staged: boolean, untracked: boolean) =>
      call<FileDiff>("file_diff", { path, oldPath, staged, untracked }),
    commitDetails: (id: string) => call<CommitDetails>("commit_details", { id }),
    commitFileDiff: (id: string, path: string, oldPath: string | null) =>
      call<FileDiff>("commit_file_diff", { id, path, oldPath }),
    fileHistory: (path: string, limit: number) => call<CommitInfo[]>("file_history", { path, limit }),
    blame: (path: string) => call<BlameLine[]>("blame", { path }),
    branchesContaining: (id: string) => call<string[]>("branches_containing", { id }),
    lineage: () => call<BranchLineage[]>("lineage"),
    stashes: () => call<StashEntry[]>("stashes"),
    remotes: () => call<string[]>("remotes"),
    commitTemplate: () => call<string | null>("commit_template"),
    /** Raw bytes; empty when the file does not exist at that version. */
    fileBytes: (version: FileVersion) => call<ArrayBuffer>("file_bytes", { version }),

    stage: (paths: string[]) => call<void>("stage", { paths }),
    unstage: (paths: string[]) => call<void>("unstage", { paths }),
    discard: (paths: string[]) => call<Undo>("discard", { paths }),
    stageLines: (path: string, picked: LineRef[], untracked: boolean) =>
      call<void>("stage_lines", { path, picked, untracked }),
    unstageLines: (path: string, picked: LineRef[]) => call<void>("unstage_lines", { path, picked }),
    discardLines: (path: string, picked: LineRef[]) => call<Undo>("discard_lines", { path, picked }),
    resolveConflict: (path: string, side: "ours" | "theirs") => call<void>("resolve_conflict", { path, side }),
    commit: (message: string, amend: boolean) => call<Undo>("commit", { message, amend }),

    cherryPick: (id: string) => call<Undo>("cherry_pick", { id }),
    revert: (id: string) => call<Undo>("revert", { id }),
    reset: (id: string, mode: ResetMode) => call<Undo>("reset", { id, mode }),
    abortOperation: () => call<void>("abort_operation"),
    continueOperation: () => call<void>("continue_operation"),
    undo: (action: UndoAction) => call<void>("undo", { action }),

    createBranch: (name: string, start: string | null, checkout: boolean) =>
      call<Undo>("create_branch", { name, start, checkout }),
    checkoutBranch: (name: string) => call<Undo>("checkout_branch", { name }),
    checkoutRemoteBranch: (name: string) => call<Undo>("checkout_remote_branch", { name }),
    checkoutCommit: (id: string) => call<Undo>("checkout_commit", { id }),
    renameBranch: (from: string, to: string) => call<Undo>("rename_branch", { from, to }),
    deleteBranch: (name: string, force: boolean) => call<Undo>("delete_branch", { name, force }),
    merge: (name: string) => call<Undo>("merge", { name }),
    rebase: (onto: string) => call<Undo>("rebase", { onto }),
    createTag: (name: string, target: string, message: string | null) =>
      call<Undo>("create_tag", { name, target, message }),
    deleteTag: (name: string) => call<Undo>("delete_tag", { name }),
    pushTag: (name: string) => call<void>("push_tag", { name }),

    fetch: () => call<void>("fetch"),
    pull: (rebase: boolean) => call<Undo>("pull", { rebase }),
    push: (force: boolean) => call<void>("push", { force }),
    stashPush: (message: string | null, includeUntracked: boolean) =>
      call<Undo>("stash_push", { message, includeUntracked }),
    stashApply: (index: number, pop: boolean) => call<void>("stash_apply", { index, pop }),
    stashDrop: (index: number) => call<Undo>("stash_drop", { index }),
  };
}

export type RepoApi = ReturnType<typeof repoApi>;
