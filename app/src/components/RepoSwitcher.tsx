import { useMemo, useState } from "react";

interface Props {
  /** Open tabs, in order. */
  tabs: string[];
  active: number;
  /** Every repository Heritree knows about, most recent first. */
  known: string[];
  onPick: (path: string) => void;
  onAddFolder: () => void;
  onClose: () => void;
}

export const folderName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

/** Ctrl+P: jump to any open or recent repository by typing part of its name. */
export function RepoSwitcher({ tabs, active, known, onPick, onAddFolder, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);

  const results = useMemo(() => {
    const all = [...tabs, ...known.filter((p) => !tabs.includes(p))];
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return all
      .filter((p) => words.every((w) => p.toLowerCase().includes(w)))
      // Name matches before path-only matches.
      .sort((a, b) => {
        const q = words[0] ?? "";
        return Number(!folderName(a).toLowerCase().includes(q)) - Number(!folderName(b).toLowerCase().includes(q));
      })
      .slice(0, 50);
  }, [tabs, known, query]);

  const pick = (path: string | undefined) => {
    if (!path) return;
    onClose();
    onPick(path);
  };

  return (
    <div className="modal-backdrop palette-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="palette" role="dialog" aria-label="Switch repository">
        <input
          autoFocus
          value={query}
          placeholder="Switch to repository…"
          spellCheck={false}
          onChange={(e) => {
            setQuery(e.target.value);
            setIndex(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape") onClose();
            else if (e.key === "ArrowDown") {
              e.preventDefault();
              setIndex((i) => Math.min(results.length - 1, i + 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setIndex((i) => Math.max(0, i - 1));
            } else if (e.key === "Enter") pick(results[index]);
          }}
        />
        <ul>
          {results.map((path, i) => {
            const tab = tabs.indexOf(path);
            return (
              <li key={path}>
                <button className={i === index ? "active" : undefined} onMouseEnter={() => setIndex(i)} onClick={() => pick(path)}>
                  <span className="switcher-entry">
                    <strong>{folderName(path)}</strong>
                    <span className="muted">{path}</span>
                  </span>
                  <span className="menu-hint">
                    {tab === active ? "current" : tab >= 0 ? (tab < 9 ? `open · Ctrl+${tab + 1}` : "open") : ""}
                  </span>
                </button>
              </li>
            );
          })}
          {results.length === 0 && <li className="empty pad">No repository matches. Add a folder of repositories below.</li>}
        </ul>
        <div className="switcher-footer">
          <button
            className="small"
            onClick={() => {
              onClose();
              onAddFolder();
            }}
          >
            Add repositories from a folder…
          </button>
          <span className="muted">Ctrl+Tab cycles open tabs</span>
        </div>
      </div>
    </div>
  );
}
