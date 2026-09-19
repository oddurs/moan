---
id: 88
title: one way in
type: chore
status: done
milestone: v0.1
created: 2026-09-15
updated: 2026-09-15
priority: p0
effort: l
area: ui
---

## Problem

A pass over v0.1 as engineering rather than as features, driven by measurement and a stricter lint policy.

## What it found

- **Drawing was linear in the length of the conversation.** Every row was laid out to discover how tall it was, so a frame cost 36ms at ten thousand messages and 178ms at fifty thousand — five frames a second. Heights are knowable without laying a row out.
- **`App::scroll` was a `u16` holding an absolute line number.** Past 65,535 lines it wrapped, and the mouse handler adds a row to it to decide what was clicked, so clicks landed on the wrong message. Only the small windowed offset handed to ratatui has to fit a `u16`.
- **Integers crossed between storage and indices unchecked.** `i64` to `usize` and back in a dozen places, any of which turns a negative into a very large index.
- **No seam.** No `scripts/task`, no hooks, no CI, no pinned toolchain, and a lint policy that only applied when somebody remembered the flag.

## Acceptance criteria

- [x] Only the rows the viewport touches are laid out: 178ms → 8.1ms at fifty thousand, 0.52ms in release
- [x] A test asserts the *shape* rather than a wall-clock number, so it means the same on a busy machine
- [x] `App::scroll` is a `usize`; a test clicks correctly in a 70,000-line feed
- [x] Storage integers cross through clamping helpers rather than `as`
- [x] The scrollbar is integer arithmetic, not rounded floats
- [x] The lint policy lives in `Cargo.toml`, so `cargo clippy` alone is the whole check
- [x] `unsafe_code` is forbidden; the three cast classes that were clean are denied so they stay clean
- [x] `scripts/task {fmt|fmt:check|lint|test|build|check|bench}`, with hooks and CI calling the same verbs
- [x] Toolchain pinned; CI runs macOS and Linux, which v1.0 promises
