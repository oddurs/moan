---
id: 53
title: Stale SQLite sidecars abort startup with a disk I/O error
type: bug
status: done
milestone: v1.0
created: 2026-09-14
updated: 2026-09-14
priority: p1
effort: s
area: store
---

## What happens

Deleting `moan.db` without its `-shm` and `-wal` files leaves sidecars that do not match the new database. SQLite then fails every query with `Error code 522: disk I/O error`, and moan exits before drawing anything.

## What should happen

A mismatched sidecar is detected on open and cleared, or the error names the files to delete. A user who tidied up a directory should not be left with a tool that cannot start.

## Reproduction

1. Run moan once so the database exists
2. `rm ~/Library/Application\ Support/moan/moan.db` but leave `moan.db-shm` and `moan.db-wal`
3. Run moan again
