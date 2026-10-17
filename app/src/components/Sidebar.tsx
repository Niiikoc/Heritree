import type { RefInfo, RefKind } from "../types";

const SECTIONS: { kind: RefKind; title: string }[] = [
  { kind: "localBranch", title: "Branches" },
  { kind: "remoteBranch", title: "Remotes" },
  { kind: "tag", title: "Tags" },
];

interface Props {
  refs: RefInfo[];
  currentBranch: string | null;
  onPick: (ref: RefInfo) => void;
}

export function Sidebar({ refs, currentBranch, onPick }: Props) {
  return (
    <nav className="sidebar" aria-label="References">
      {SECTIONS.map(({ kind, title }) => {
        const items = refs.filter((r) => r.kind === kind);
        return (
          <section key={kind}>
            <h2>
              {title} <span className="count">{items.length}</span>
            </h2>
            {items.length === 0 && <p className="empty">None</p>}
            <ul>
              {items.map((r) => {
                const current = kind === "localBranch" && r.shortName === currentBranch;
                return (
                  <li key={r.name}>
                    <button
                      className={current ? "current" : undefined}
                      onClick={() => onPick(r)}
                      title={`${r.name}\n${r.target.slice(0, 7)}`}
                    >
                      {current && <span className="current-dot" aria-label="current branch" />}
                      {r.shortName}
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
    </nav>
  );
}
