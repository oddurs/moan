---
id: 54
title: Re-read sessions when the parser changes
type: chore
status: done
milestone: v0.3
created: 2026-09-14
updated: 2026-09-14
priority: p0
effort: s
area: store
---

## Problem

Events are archived and resumed from a byte offset, so a fix to what counts as cruft never reached a session that had already been read. Task notifications kept showing as the human speaking long after the parser stopped emitting them.

## Acceptance criteria

- [x] `sessions.parser` records which parser wrote a session's rows
- [x] A bump throws the rows away and re-reads from the start
- [x] A database written before the column existed still opens
