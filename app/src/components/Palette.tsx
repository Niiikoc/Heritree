import { useMemo, useState } from "react";

export interface PaletteCommand {
  id: string;
  label: string;
  /** Searchable extra words, e.g. "checkout switch". */
  keywords?: string;
  hint?: string;
  run: () => void;
}

interface Props {
  commands: PaletteCommand[];
  onClose: () => void;
}

/** Ctrl+K: type a few letters of any action or branch and press Enter. */
export function Palette({ commands, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);

  const results = useMemo(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return commands
      .filter((c) => {
        const text = `${c.label} ${c.keywords ?? ""}`.toLowerCase();
        return words.every((w) => text.includes(w));
      })
      .slice(0, 50);
  }, [commands, query]);

  const pick = (c: PaletteCommand | undefined) => {
    if (!c) return;
    onClose();
    c.run();
  };

  return (
    <div className="modal-backdrop palette-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="palette" role="dialog" aria-label="Command palette">
        <input
          autoFocus
          value={query}
          placeholder="Type a command or a branch name…"
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
          {results.map((c, i) => (
            <li key={c.id}>
              <button className={i === index ? "active" : undefined} onMouseEnter={() => setIndex(i)} onClick={() => pick(c)}>
                <span>{c.label}</span>
                {c.hint && <span className="menu-hint">{c.hint}</span>}
              </button>
            </li>
          ))}
          {results.length === 0 && <li className="empty pad">No matching command</li>}
        </ul>
      </div>
    </div>
  );
}
