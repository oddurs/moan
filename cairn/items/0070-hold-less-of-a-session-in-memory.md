---
id: 70
title: Hold less of a session in memory
type: chore
status: backlog
milestone: later
created: 2026-09-14
updated: 2026-09-14
priority: p3
effort: l
area: store
---

## Problem

`load` reads every event of every active session into memory, raw text and all — 18MB for a five-session merged view. It is fine on a laptop and would not be at ten times the size.

## Proposal

Keep the raw text out of memory until something needs it: expanding a message, filtering, or condensing. That needs the content hash stored on the row, since it is currently derived from the raw text and is the cache key.

## Acceptance criteria

- [ ] The content hash is a column, not something recomputed from text held in memory
- [ ] Raw text is fetched when a message is expanded, filtered over, or condensed
- [ ] Memory for a large merged project is measured before and after
