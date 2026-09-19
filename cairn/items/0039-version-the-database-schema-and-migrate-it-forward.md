---
id: 39
title: Version the database schema and migrate it forward
type: chore
status: done
milestone: v1.0
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: store
---

## Problem

The store has no version. Any change to the events table against an existing database is undefined behaviour, and users have real archives in there by now.

## Proposal

Stamp a schema version, migrate forward on open, and refuse to open a database from a newer moan rather than misreading it.

## Acceptance criteria

- [x] An older database opens and migrates without losing events or cached gists
- [x] A newer database is refused with a message saying to upgrade
- [x] Migration is covered by a test that starts from a real older schema

## 2026-09-14

Partly done: the sessions table now carries a parser version and migrate() adds columns to an older database. Still open: a version on the database as a whole, and refusing one written by a newer moan.

## 2026-09-14

The engine change in v0.5 lands before this. Version the archive once, on Turso, rather than building a migration for SQLite and then migrating engines.

## 2026-09-14

The schema now carries a version in pragma user_version, refuses an archive from a newer moan, and migrates older ones. What is left for v1.0 is the engine-level versioning on Turso.

## 2026-09-15

Done during the storage pass: pragma user_version stamps the format, an archive from a newer moan is refused with a message saying what to do, and migrate() brings older ones forward. Tested.
