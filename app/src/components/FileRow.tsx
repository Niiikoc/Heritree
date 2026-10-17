import type { ReactNode } from "react";
import type { ChangeKind } from "../types";

export const CHANGE_LABEL: Record<ChangeKind, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  typeChanged: "T",
  untracked: "U",
  conflicted: "!",
};

interface Props {
  path: string;
  originalPath: string | null;
  kind: ChangeKind;
  active: boolean;
  onOpen: () => void;
  onContextMenu?: (e: React.MouseEvent) => void;
  /** Buttons shown on hover, e.g. Stage / Discard. */
  actions?: ReactNode;
}

/** One changed file: its change letter, folder and name. */
export function FileRow({ path, originalPath, kind, active, onOpen, onContextMenu, actions }: Props) {
  const slash = path.lastIndexOf("/");
  const name = path.slice(slash + 1);
  const folder = slash >= 0 ? path.slice(0, slash) : "";
  return (
    <li
      className={`file-row${active ? " active" : ""}`}
      title={originalPath ? `${originalPath} → ${path}` : path}
      onContextMenu={onContextMenu}
    >
      <button className="file-open" onClick={onOpen}>
        <span className={`change change-${kind}`} aria-label={kind}>
          {CHANGE_LABEL[kind]}
        </span>
        <span className="file-name">{name}</span>
        {folder && <span className="file-folder">{folder}</span>}
      </button>
      {actions && <span className="file-actions">{actions}</span>}
    </li>
  );
}
