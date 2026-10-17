// Typed wrappers around the Tauri commands in src-tauri/src/lib.rs.

import { invoke } from "@tauri-apps/api/core";
import type { FileStatus, LogRow, RefInfo, RepoInfo } from "./types";

export const openRepository = (path: string) => invoke<RepoInfo>("open_repository", { path });
export const repoInfo = () => invoke<RepoInfo>("repo_info");
export const listRefs = () => invoke<RefInfo[]>("list_refs");
export const loadLog = (limit: number) => invoke<LogRow[]>("load_log", { limit });
export const workingTreeStatus = () => invoke<FileStatus[]>("working_tree_status");
