---
id: 26
title: Resolve a session's cwd to its repo root
type: feature
status: done
milestone: v0.3
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-14
priority: p0
effort: m
area: source
---

## Problem

Agents run in worktrees. `~/Code/moan` and `~/Code/.worktrees/moan/fix/0041` are the same project, and moan currently treats them as two unrelated things.

## Proposal

Resolve each session's recorded `cwd` to the common git directory, so every worktree of a repo reports the same project identity. Cache it: it never changes for a given session.

## Acceptance criteria

- [x] Sessions from a worktree group with sessions from the main checkout
- [x] The branch is kept and shown, because that is how you tell two worktrees apart
- [x] A cwd that is not in a git repo groups by its own path and does not error
- [x] A deleted worktree does not break the grouping of its siblings
