---
id: 33
title: Read opencode's SQLite store incrementally
type: feature
status: done
milestone: v0.4
assignee: Oddur Sigurdsson
depends_on:
- 19
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: l
area: source
---

## Problem

opencode keeps its transcript in `~/.local/share/opencode/opencode.db`, not in files. Polling it by re-reading everything does not scale and will not stream.

## Proposal

Implement the source against whatever cursor the v0.2 spike settled on — a rowid or timestamp watermark per session — reading only rows newer than the last poll.

## Acceptance criteria

- [x] A poll reads only new rows
- [x] Opening a large opencode history is fast enough to feel instant
- [x] The database is opened read-only and never written to
- [x] WAL mode and a concurrent writer are handled
