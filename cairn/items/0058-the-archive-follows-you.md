---
id: 58
key: v0.5
title: the archive follows you
type: milestone
status: backlog
created: 2026-09-14
updated: 2026-09-15
priority: p2
due: 2026-12-14
---

## Ships

The archive stops being a file on one machine. moan keeps its conversations in Turso — SQLite-compatible, pure Rust, no C compiler — and can optionally sync them, so the same history opens on your laptop and your desktop.

## Why replace SQLite

- **Sync.** The reason. An archive that only exists on one machine is half a tool when you work on two.
- **No C toolchain.** `rusqlite` bundles and compiles SQLite's C. The `turso` crate is pure Rust, which makes cross-compiling and the Linux and release work in v1.0 materially easier.
- **Same SQL.** A probe ran every statement moan's store uses against `turso 0.8.0-pre.11`: schema, indexes, `insert or replace`, `on conflict do update`, `alter table add column`, parameterised reads, transactions. All passed. Only `pragma journal_mode = wal` differs — it returns a row, so it needs `query` rather than `execute`.

## Done when

- [ ] The store runs on Turso, with every existing store test passing unchanged
- [ ] An existing SQLite archive is carried over without losing an event or a condensation
- [ ] moan works with no account, no network, and no Turso config at all
- [ ] Syncing is opt-in, and what it sends is a decision the user made on purpose
- [ ] Pulling the network mid-sync leaves the local archive readable

## Explicitly not in this milestone

- Syncing anything other than moan's own archive. Transcripts stay where the agent wrote them.
- Sharing an archive with another person. This is one user, several machines.
- Turso as a requirement. A local file stays the default and the fallback.

## Standing risk

`turso` is `0.8.0-pre.11` and `libsql` is `0.10.0-pre.4`. Both are pre-release, and this is the component holding durable data. The trait in the first item exists so the engine can be changed back or forward without touching anything else.
