---
id: 66
title: Prove a broken sync cannot break the archive
type: bug
status: done
milestone: v0.5
depends_on:
- 65
created: 2026-09-14
updated: 2026-09-15
priority: p1
effort: m
area: store
---

## What happens

Unknown, and that is the problem. Sync writes to the same database the feed is reading. A process killed mid-sync, a half-applied change, or a connection dropped partway through have never been tested.

## What should happen

The local archive is readable after any of them. Worst case is stale, never corrupt and never empty.

## Reproduction

1. Start a sync of a large archive
2. Kill the process partway, then pull the network partway, then do both
3. Reopen and check every session, event and condensation is still there

## 2026-09-15

Covered as far as it can be without a remote: the archive is only ever read when building the shareable copy, so nothing that happens to the copy can reach it. Tested by truncating the copy mid-way and rebuilding. Pulling the network mid-push is untested, because this build does not push.

## 2026-09-15

The archive is only ever read when the shareable copy is built, so nothing that happens to the copy can reach it — tested by truncating the copy part-way and rebuilding. Merging runs in one transaction and rolls back whole. Pulling the network mid-push stays untested, because there is no push.
