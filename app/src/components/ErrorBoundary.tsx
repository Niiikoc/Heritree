import { Component, Fragment, type ReactNode } from "react";

/** UI choices a repository view remembers; a bad one can break every start. */
const VIEW_SETTINGS = ["heritree.showGitLog", "heritree.branchView", "heritree.diffMode"];

interface Props {
  /** Repository folder, shown in the message. */
  path: string;
  /** Whether this tab is the one on screen. */
  active: boolean;
  children: ReactNode;
}

interface State {
  error: Error | null;
  /** Bumped on retry. */
  attempt: number;
}

/**
 * Keeps a crash in one repository tab from blanking the whole window: the tab
 * shows what went wrong and how to recover, and the other tabs keep working.
 * React still needs a class for this.
 */
export class RepoErrorBoundary extends Component<Props, State> {
  state: State = { error: null, attempt: 0 };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack?: string | null }) {
    console.error(`Repository view for ${this.props.path} crashed:`, error, info.componentStack);
  }

  retry = () => this.setState((s) => ({ error: null, attempt: s.attempt + 1 }));

  resetViewSettings = () => {
    for (const key of VIEW_SETTINGS) {
      try {
        localStorage.removeItem(key);
      } catch {
        // Storage unavailable: nothing was saved, so nothing to reset.
      }
    }
    this.retry();
  };

  render() {
    const { error, attempt } = this.state;
    // A new key on retry remounts the view, so it starts over from scratch.
    if (!error) return <Fragment key={attempt}>{this.props.children}</Fragment>;
    const details = `${error.name}: ${error.message}\n\n${error.stack ?? ""}`;
    return (
      <main className="app-error" hidden={!this.props.active} role="alert">
        <h2>Something went wrong in this repository</h2>
        <p className="muted">{this.props.path}</p>
        <p>
          Your repository is untouched; only the view stopped. Try again, or reset the view settings if it keeps
          happening. Other tabs still work.
        </p>
        <div className="app-error-actions">
          <button className="primary" onClick={this.retry}>
            Try again
          </button>
          <button onClick={this.resetViewSettings} title="Git log panel, branch view and diff mode go back to their defaults">
            Reset view settings
          </button>
        </div>
        <details>
          <summary>
            Error details
            <button
              className="small"
              onClick={(e) => {
                e.preventDefault(); // copy without toggling the details open or shut
                navigator.clipboard.writeText(details);
              }}
            >
              Copy
            </button>
          </summary>
          <pre>{details}</pre>
        </details>
      </main>
    );
  }
}
