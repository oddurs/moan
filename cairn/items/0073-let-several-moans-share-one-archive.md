---
id: 73
title: Let several moans share one archive
type: bug
status: done
milestone: v0.3
created: 2026-09-14
updated: 2026-09-14
priority: p0
effort: m
area: store
---

## What happened

A second moan started against the same archive died immediately with `database is locked`.

Three separate causes, each hiding the next:

- No busy timeout, so any contention was fatal rather than a wait.
- `pragma journal_mode = wal` needs exclusive access and does not honour a busy timeout. Instances starting together all tried to set it and all but one failed. It lives in the file, so only the first needs to.
- `append` used rusqlite's default deferred transaction, which opens as a reader and upgrades on first write. SQLite refuses that upgrade with a busy error *immediately* — it cannot wait without risking deadlock — so the timeout never applied.

Two instances on one session also both paid to condense the same messages.

## Acceptance criteria

- [x] A second and third instance open and run against a live archive
- [x] Writes from several instances are all visible to a reader
- [x] Write transactions take the lock up front so the busy timeout applies
- [x] A claim table stops two instances buying the same condensation; measured at 35 total across two instances, the same as one alone
- [x] A claim from an instance that died goes stale and is taken over
- [x] Each instance sweeps for condensations the others produced, without a keypress
