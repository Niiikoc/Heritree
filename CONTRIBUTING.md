# Contributing to Heritree

Thanks for helping. Heritree is young, so the best contributions right now are bug reports with a
repository that reproduces them, and small focused pull requests.

## Before you start

- For anything bigger than a small fix, open an issue first so we can agree on the approach.
- Issues labelled `good first issue` are a good place to begin.

## Development setup

See [Building from source](README.md#building-from-source). In short:

```sh
cd app && npm install && npm run tauri dev
```

## Pull requests

- Keep each pull request to one change.
- Run the checks before pushing:

  ```sh
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cd app && npm run build
  ```

- Add tests for engine changes in `crates/heritree-core`. Integration tests build real repositories with
  the `git` CLI (see `crates/heritree-core/tests/repository.rs`).
- Never lose user data: anything that rewrites history or discards changes must be undoable.

## Developer Certificate of Origin

Heritree uses the [Developer Certificate of Origin](https://developercertificate.org/) instead of a
contributor license agreement. Sign off every commit to confirm you wrote it or have the right to submit it
under the project's license:

```sh
git commit -s -m "Describe the change"
```

This adds a `Signed-off-by: Your Name <you@example.com>` line to the commit message.

## License

By contributing, you agree that your contributions are licensed under the
[GNU General Public License v3.0 or later](LICENSE).
