---
id: 37
title: Run the suite on macOS and Linux in CI
type: chore
status: done
milestone: v1.0
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: dist
---

## Problem

v1.0 promises Linux. Nothing has ever compiled there, let alone run.

## Proposal

Add a CI matrix over macOS and Linux calling the same `scripts/task check` the hooks call.

## Acceptance criteria

- [x] Both platforms build and pass the full suite on every push
- [x] A failure on either blocks the merge
- [x] The matrix runs the same verb as the git hooks, so they cannot drift

## 2026-09-15

Done alongside 0025. The matrix runs ./scripts/task check, which is the same verb the git hook runs, so they cannot drift.
