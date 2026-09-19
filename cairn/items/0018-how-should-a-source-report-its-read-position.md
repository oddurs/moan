---
id: 18
title: How should a source report its read position?
type: spike
status: done
milestone: v0.2
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: s
area: source
---

## Question

`Source::parse` takes a byte offset and returns a new one. That is exactly right for an append-only file and meaningless for opencode, whose transcript is a SQLite database. What replaces it?

## Timebox

One day. Read Codex's rollout files and opencode's `session` / `message` / `part` tables, and write down the smallest cursor that serves all three.

## What we will do with the answer

It decides the `Source` trait signature, which every source implementation depends on. Getting it wrong means rewriting three sources instead of one.

## Answer

**An opaque string each source interprets.**

The two alternatives both lose.

A *typed enum* of `Bytes(u64)` and `Row { table, id }` puts every source's
private business in one shared type. Adding a fourth agent means editing an enum
three other sources already match on, and the store has to know the difference
between them to write one down.

A *uniform watermark* of `(timestamp, id)` assumes every transcript is ordered
by time and that its records carry ids. Claude Code's is a file: its position is
a byte offset, and re-deriving one from a timestamp would mean re-reading the
file to find it.

An opaque string costs one thing — the store cannot reason about it — and moan
never needs to. It writes it down and gives it back.

| source | what it puts in the cursor |
|---|---|
| Claude Code | the byte offset, `"4096"` |
| Codex | the same; rollouts are append-only files too |
| opencode | the last row it read, `"41928"` |

### The three cases

- **A torn read.** Claude Code's parser already only advances past the last
  newline, so the cursor never names a position inside a half-written record.
  That property belongs to the source and stays there.
- **A rotated file.** The store compares the recorded size to the file's. This
  does not change: the size is tracked beside the cursor, not inside it.
- **A source that shrinks.** A source that cannot make sense of a cursor it is
  handed returns the start, and the session is read again. Cheap, and correct
  by construction — a re-read is a no-op, because `(session, uid)` is unique.

### What it costs

`sessions.offset` becomes `sessions.cursor`, a text column. An archive written
before this reads its integer offset and carries it over, which is the whole
migration.

Candidates to weigh:

- an opaque `Cursor(String)` each source interprets however it likes
- a typed enum of `Bytes(u64)` and `Row { table, id }`
- a watermark `(timestamp, id)` pair, uniform across sources

Say explicitly how each handles a torn read, a rotated file, and a source that shrinks.
