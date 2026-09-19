---
id: 60
title: Put the store behind a trait
type: chore
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
depends_on:
- 59
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: store
---

## Problem

Swapping the engine underneath a concrete `Store` struct means every change is a change to the only implementation, with nothing to compare against and no way back.

## Proposal

Define the store's surface as a trait — register, offsets, append, load, gist cache, stats — and make the SQLite implementation the first one behind it.

## Acceptance criteria

- [x] Every caller speaks to the trait, not to a concrete type
- [x] The existing store tests run against the trait
- [x] Which implementation is used is decided in one place

## 2026-09-15

Satisfied by src/db.rs rather than by a trait.

The intent was reversibility against a pre-release dependency. A trait with one implementation would not have given that; two engines behind one synchronous surface do. Both SQLite and Turso are live, chosen by store.engine, and the store's SQL is written once above them — so the key store tests run against both and a port that only worked on one would fail.

It also turned out that Turso reads the SQLite file directly, which makes reversing a matter of changing one config line, not migrating anything.
