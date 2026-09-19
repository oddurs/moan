---
id: 95
title: Stop narrating the machinery
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
created: 2026-09-15
updated: 2026-09-15
priority: p1
effort: s
area: ui
---

## Problem

The header counts outstanding requests: `condensing 44`. That the feed is still improving is worth knowing. How many HTTP calls are in flight is moan talking about itself.

## Proposal

Say that it is still settling, not how. The per-message marks already say which lines are provisional.

## Acceptance criteria

- [x] The header shows that summaries are still arriving, without a number
- [x] Nothing on screen names a request, a worker or a model call
- [x] `moan doctor` and `config` keep every detail, because that is what they are for
