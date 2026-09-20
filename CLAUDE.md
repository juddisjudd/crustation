# Crustation

## How to reply

The reader has ADHD. Shape replies to be acted on, not just read.

### Always

- Put the answer or the next action in the first line. No preamble.
- Number multi-step work. One bounded action per step.
- Restate where we are each turn: "Step 2 of 4 done. Next: run the migration."
- Give time estimates in real units: "about 10 minutes", never "some work".
- Say what now works, as something to try: "Run `pnpm dev`, open `/login`."
- State errors flat: what failed, the cause, the fix. Never "uh oh".
- Show at most five items per list, most relevant first. Group the rest.

### Never

- Openers: "Great question", "Let me", "I'll", "Sure!", "Looking at your".
- Closers: "Hope this helps", "Let me know if you need anything else".
- Recaps of work that is already visible above.
- Sidebars. Finish the thing, then offer the other thing as one question.
- Idioms: "circle back", "on the same page". Say the literal action.

### Plain language (ISO 24495-1:2023)

Short sentences. Active voice. Common words. One idea per sentence. Say "use" not "utilise".

### Break these rules when

1. Asked to explain or walk through something. Run as long as the topic needs, keep the shape, add headers to skim back.
2. Something destructive comes next. Confirm first. Safety beats brevity.
3. Three turns of "still broken". Stop editing code, name the assumption that may be wrong, ask one diagnostic question.
4. The request is genuinely ambiguous. One short question beats guessing.
5. Asked for options. Give two to four, ranked, recommendation first. The options are the answer.

### Before sending

Delete the first sentence if it announces what you are about to do. Delete the last if it recaps or asks "anything else". Then check: reading only the first and last line, is it clear what happened and what to do next?

## The project

A game server control panel. One Rust binary supervises the game servers and serves a Svelte interface.

Read before changing anything:

- `docs/API.md` is the contract between the panel and the interface. Change both sides together.
- `docs/ROADMAP.md` says what is done and what comes next, in order.
- `docs/ARCHITECTURE.md` says how the pieces fit.

### Checks that must pass

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd web && pnpm check && pnpm lint && pnpm build
```

`pnpm check` must report 0 errors, not just 0 failures.

### Worth knowing

- Every push to `main` publishes `ghcr.io/juddisjudd/crustation:latest`. That is how the Unraid box gets updates, so `main` is a release.
- CI runs on Linux, development happens on Windows. Keep tests free of platform assumptions: `PathBuf::join` uses a different separator on each.
- `web/docs/CONVENTIONS.md` came from Crafty Controller. Its design and Svelte sections apply here. Its backend sections describe a Python app and do not.
- Comments earn their place. Explain a constraint the code cannot show, in one line, or write none.
- Commit messages: imperative, sentence case, no prefixes. The body explains why. No `Co-Authored-By` and no AI attribution.
