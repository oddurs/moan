---
id: 31
title: Search every stored session, not just the open one
type: feature
status: done
milestone: v0.3
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: store
---

## Problem

`/` filters the open session. Everything else moan has archived is unreachable, which wastes the best thing about having a store.

## Proposal

Query the events table across all sessions, ranked by recency, over both the gist and the original text.

## Acceptance criteria

- [x] A search returns matches from every stored session
- [x] Results say which session and when
- [x] Searching a large archive stays interactive

## 2026-09-15

Done as part of the panel: search runs across every conversation in the archive, scoped to the kinds the feed is showing.
