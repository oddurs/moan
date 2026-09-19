---
id: 25
title: Install the scripts/task seam, git hooks and CI
type: chore
status: done
milestone: v0.2
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p1
effort: m
area: dist
---

## Problem

Everything is verified by hand today. With a second source landing, `cargo fmt && cargo clippy -D warnings && cargo test` has to be one command that CI and the hooks both call.

## Proposal

Add `scripts/task` with `fmt fmt:check lint test build check`, wire git hooks to it, and run `check` in CI on macOS.

## Acceptance criteria

- [x] `./scripts/task check` runs format, clippy with warnings denied, and the full suite
- [x] A commit that fails `check` is rejected by a hook
- [x] CI runs the same verb, so the two cannot drift

## 2026-09-15

Done during the pass over v0.1: scripts/task with fmt, fmt:check, lint, test, build, check and bench; .githooks/pre-commit and commit-msg; a CI matrix on macOS and Linux calling the same verb; and a pinned toolchain.
