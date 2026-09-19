---
id: 61
title: Port the schema and queries to Turso
type: feature
status: done
milestone: v0.5
depends_on:
- 39
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: l
area: store
---

## Problem

moan's archive has to run on the new engine before any of the rest of this is worth anything.

## Proposal

Implement the trait against `turso`, following whatever the async spike settled on. A probe has already run every statement the store uses against 0.8.0-pre.11 and they all pass, so this is porting rather than redesign.

## Acceptance criteria

- [x] Every existing store test passes against the Turso implementation, unchanged
- [x] `pragma journal_mode = wal` is issued as a query, since it returns a row
- [x] No C compiler is needed to build moan
- [x] Opening a large archive is no slower than it is today, measured

## 2026-09-15

Turso is implemented and passes the suite, but it is not the default. Two things it cannot do yet, both of which SQLite does and moan relies on: several instances cannot open one archive (exclusive lock; experimental_multiprocess_wal panics), and pruning cannot give disk space back (no VACUUM). Recorded in docs/0001-turso.md with the comparison.
