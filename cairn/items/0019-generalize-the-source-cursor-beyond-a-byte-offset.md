---
id: 19
title: Generalize the Source cursor beyond a byte offset
type: chore
status: done
milestone: v0.2
assignee: Oddur Sigurdsson
depends_on:
- 18
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: source
---

## Problem

`parse(path, from_byte) -> (Vec<Event>, u64)` hardcodes the assumption that a transcript is an append-only file. opencode's is not.

## Proposal

Adopt whatever the spike concluded. Migrate the Claude Code source onto it with no behaviour change, proven by the existing tests staying green.

## Acceptance criteria

- [x] The trait no longer mentions bytes
- [x] `sessions.offset` in the store holds whatever the new cursor is, and old rows still open
- [x] Every existing test passes unchanged
