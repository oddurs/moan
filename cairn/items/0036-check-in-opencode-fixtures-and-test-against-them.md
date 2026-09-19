---
id: 36
title: Check in opencode fixtures and test against them
type: chore
status: done
milestone: v0.4
assignee: Oddur Sigurdsson
depends_on:
- 34
created: 2026-09-14
updated: 2026-09-15
priority: p1
effort: m
area: source
---

## Problem

The other two sources are pinned by fixtures. opencode's schema is the most likely of the three to move, because it is a database with migrations.

## Proposal

Build a small redacted database in a fixture and assert the same properties the other sources assert.

## Acceptance criteria

- [x] Fixtures contain no real prose, paths, or keys
- [x] Tests cover incremental reads, torn messages, and cruft filtering
- [x] A schema migration in opencode breaks a test rather than emptying the feed

## 2026-09-15

Fixtures are a store built in the test rather than a copy of anybody's, so they carry no real prose. Alongside them, a_real_store_still_reads runs over whatever opencode has written on this machine and asserts the invariants that matter.
