# Crustation

A game server control panel. One Rust binary supervises the game servers and serves a Svelte interface.

Read before changing anything:

- `docs/API.md` is the contract between the panel and the interface. Change both sides together.
- `docs/ROADMAP.md` says what is done and what comes next, in order.
- `docs/ARCHITECTURE.md` says how the pieces fit.

## Checks that must pass

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd web && pnpm check && pnpm lint && pnpm build
```

`pnpm check` must report 0 errors, not just 0 failures.

## Worth knowing

- Every push to `main` publishes `ghcr.io/juddisjudd/crustation:latest`. That is how the Unraid box gets updates, so `main` is a release.
- CI runs on Linux, development happens on Windows. Keep tests free of platform assumptions: `PathBuf::join` uses a different separator on each.
- `web/docs/CONVENTIONS.md` came from Crafty Controller. Its design and Svelte sections apply here. Its backend sections describe a Python app and do not.
- Comments earn their place. Explain a constraint the code cannot show, in one line, or write none.
- Commit messages: imperative, sentence case, no prefixes. The body explains why. No `Co-Authored-By` and no AI attribution.
