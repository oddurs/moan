---
id: 30
title: Interleave a project's sessions into one merged feed
type: feature
status: done
milestone: v0.3
depends_on:
- 29
created: 2026-09-14
updated: 2026-09-14
priority: p1
effort: l
area: ui
---

## Problem

To see what a project's agents did, in the order they did it, you have to read several feeds and merge them in your head.

## Proposal

Offer one feed for a whole project: every session's events in timestamp order, each line labelled with the session or branch it came from.

## Acceptance criteria

- [x] A merged feed shows every session of the project in time order
- [x] Each line names its origin without crowding out the gist
- [x] New events from any session appear live
- [x] Filtering and expansion work exactly as in a single feed
