<p align="center">
  <img src="app/src-tauri/icons/128x128@2x.png" width="96" alt="Heritree logo">
</p>

<h1 align="center">Heritree</h1>

<p align="center">
  A free, open source Git client for Windows, Linux and macOS that shows where every branch came from.
</p>

---

> **Status: MVP alpha.** Heritree covers everyday Git work and shows a first branch lineage tree. Expect rough
> edges, and keep another client around for now. See the [roadmap](#roadmap).

## What it does today

- **Repositories:** open, clone, create; several repositories in tabs that stay loaded in the background; quick
  switcher (Ctrl+P, Ctrl+Tab, Ctrl+1…9); add every repository found in a folder at once.
- **Changes:** stage and unstage whole files, hunks or single lines; discard (with Undo); commit, amend,
  co-authors, commit templates.
- **Diffs:** inline or side by side, with syntax highlighting; before and after for images; file history and blame.
- **Live:** refreshes by itself when files change on disk or another tool (terminal, IDE) runs git.
- **History:** commit graph with search (message, author, SHA); commit details with changed files; which branches
  contain a commit.
- **Branches and tags:** create, check out, rename, delete, merge, rebase, cherry-pick, revert, reset; drag a
  branch or commit onto the current branch to merge or cherry-pick; create, push and delete tags.
- **Remotes and stash:** fetch, pull (merge or rebase), push (force only with lease); stash, apply, pop, drop.
- **Pull requests:** create one from any branch on GitHub or GitLab, signed in with the credentials git already
  uses; it goes into the branch it was created from by default, and is pushed first if needed. Bitbucket and Azure
  DevOps (or no stored credentials) open the site's new pull request page, filled in.
- **Lineage:** branches shown as a tree under the branch they came from, with how sure Heritree is, ahead/behind
  counts, and a marker on the commit where each branch started.
- **Safety:** Undo (Ctrl+Z) for commits, discards, resets, merges, branch and tag changes; a backup before every
  hard reset; a Git log panel listing every command Heritree ran.
- **Keyboard:** Ctrl+K command palette, Ctrl+Enter commit, Ctrl+Shift+P push, Ctrl+Shift+L pull, Ctrl+B new
  branch, Ctrl+F search, J/K to move through commits.

## Why another Git client?

- **Branch lineage.** Git does not record which branch a branch was created from. Heritree works it out and remembers it,
  so you can see that `hotfix/2.4.1` came from `release/2.4`, which came from `main`, and which fixes are missing where.
- **Everything free.** No paid tier and no account. Licensed under the GPL, so it stays that way.
- **Fast.** A Rust core on [gitoxide](https://github.com/GitoxideLabs/gitoxide) and a small [Tauri](https://tauri.app)
  shell instead of Electron.
- **Never hides Git.** Every action will show the `git` command it runs, and writes go through your own `git`, so hooks,
  credentials and config behave exactly as on the command line.

## Roadmap

| Phase | Goal |
| --- | --- |
| 0. Foundations (done) | Workspace, Rust core reading refs, log and status, first graph, CI on all three platforms |
| 1. MVP alpha (now) | Stage, commit, branch, merge, cherry-pick, revert, stash, push/pull, first lineage tree |
| 2. Public beta | Interactive rebase, conflict editor, GitHub/GitLab integration, installers |
| 3. v1.0 | Full lineage with release compare and backport, worktrees, submodules, LFS, signing |

## Project layout

```text
crates/heritree-core   Git engine: reading, operations with undo, diffs, graph layout, lineage
crates/heritree-cli    `heritree` command, the same engine without the UI
app/                   Desktop app: React + TypeScript UI
app/src-tauri          Tauri shell exposing heritree-core to the UI
```

## Building from source

You need:

- [Rust](https://rustup.rs) (stable)
- [Node.js](https://nodejs.org) 20 or newer
- `git` on your `PATH`
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS
  (WebView2 on Windows, which Windows 10 and 11 already have; `libwebkit2gtk-4.1-dev` and friends on Linux)

Run the desktop app in development mode:

```sh
cd app
npm install
npm run tauri dev
```

Build an installer for your platform:

```sh
cd app
npm run tauri build
```

Try the engine from the command line:

```sh
cargo run -p heritree-cli -- log -n 30          # graph of the current repository
cargo run -p heritree-cli -- -C ../other status  # another repository
cargo run -p heritree-cli -- log --json          # exactly what the app receives
cargo run -p heritree-cli -- lineage             # which branch each branch came from
```

Run the checks CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The Tauri crate embeds the built UI, so run `npm run build` in `app/` once before building the whole workspace.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) first.

## License

Heritree is free software, released under the [GNU General Public License v3.0 or later](LICENSE).
