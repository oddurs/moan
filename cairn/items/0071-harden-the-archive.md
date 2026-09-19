---
id: 71
title: Harden the archive
type: chore
status: done
milestone: v0.3
created: 2026-09-14
updated: 2026-09-14
priority: p0
effort: l
area: store
---

## Problem

A whole-run pass over the store, driven by measurement rather than guesswork.

- `append` and `set_offset` were separate transactions. A crash between them stored events with the offset still behind, so the next run re-read the same lines and appended them again under fresh sequence numbers.
- `save_gist` located a row by `(session, uid)` with no index for it, walking every row of the session — 4,746 reads per write, dozens of times per open.
- `events_ts` was never queried by anything and cost 200KB.
- The write-ahead log was never checkpointed and had grown to 7.8MB, larger than the 7.7MB archive.
- The database carried no version, so one written by a newer moan would be misread rather than refused.
- Deleting `moan.db` and leaving its sidecars made every query fail with `disk I/O error`.
- Event ids were not unique by construction: blocks of one message share its uuid.

## Acceptance criteria

- [x] Events and the offset they were read to commit together
- [x] Re-reading a transcript is a no-op, enforced by a unique index on (session, uid)
- [x] Event ids are unique by construction, the first keeping its bare uuid so nothing stored has to move
- [x] `save_gist` is a direct index lookup, verified through the query planner in a test
- [x] The unused index is dropped on open
- [x] The log is checkpointed when moan exits; a clean quit leaves only the archive
- [x] `pragma user_version` is set, a newer archive is refused with a message saying what to do
- [x] Orphaned sidecars are cleared rather than becoming an unexplained I/O error
- [x] `moan prune --keep N` drops the oldest sessions and reclaims the space
