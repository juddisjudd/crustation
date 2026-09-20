# Agent instructions

The instructions for this repository live in [CLAUDE.md](CLAUDE.md). Read that file first; it is the single copy, so nothing here can drift out of step with it.

The two that catch people out:

- Every push to `main` publishes the container image, so `main` is a release.
- CI runs on Linux while development happens on Windows. Keep tests free of platform assumptions.
