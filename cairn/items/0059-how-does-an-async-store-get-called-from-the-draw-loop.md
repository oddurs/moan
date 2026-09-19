---
id: 59
title: How does an async store get called from the draw loop?
type: spike
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: s
area: store
---

## Question

`rusqlite` is synchronous and the store is called straight from `App`. The `turso` crate is async: `Builder::new_local(path).build().await`, `conn.execute(sql, params).await`. What is the least invasive way to bridge that?

## Timebox

One day. Build all three against the real crate and time them; do not reason about it on paper.

## What we will do with the answer

It decides the shape of every store call site, and whether key handling has to become async. Choosing wrong means rewriting the UI twice.

## Candidates

- **Async all the way.** `App::poll`, `rebuild`, `sync_view` and the key handler become `async fn`. The loop is already `tokio`, so this is honest — but it spreads `.await` through code that is really just arithmetic.
- **A blocking shim.** Keep the store's signatures synchronous, driving the futures on a current-thread runtime inside. Smallest diff; risks blocking the draw loop on a slow call.
- **A store thread.** The store owns its own task and takes requests over a channel. Keeps the UI sync and cannot stall it; adds a hop to every read, and `sync_view` does one read per pending row.

## Answer

**A blocking shim.** The store keeps synchronous signatures and drives the
futures itself; nothing above it changes.

Measured over five hundred reads of the shape `sync_view` actually performs —
one keyed lookup against a table of five thousand rows:

| | microseconds per read |
|---|---|
| rusqlite, as it was | 3.9 |
| turso, awaited directly | 6.5 |
| turso, `block_in_place` | **6.1** |
| turso, store thread | 13.8 |

Blocking in place is not slower than awaiting — the same work on the same
runtime, with the worker released while it happens — and it keeps `.await` out
of code that is really just arithmetic. A store thread costs a round trip per
read and more than doubles it, which matters because a rebuild does one lookup
per pending row.

Turso being 1.6× rusqlite per read is nothing at these volumes: sixty lookups
on a rebuild is 0.4ms against 0.23ms.

`block_in_place` is only legal inside a runtime and `block_on` only outside
one, so the adapter holds whichever applies — borrowed inside moan, owned in a
test, which is what lets the store tests stay synchronous and run against both
engines.

Say which, and give the measured cost of a `cached_gist` lookup under each — that call happens once per pending row on every rebuild, so it is the one that matters.
