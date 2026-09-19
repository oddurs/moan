---
id: 55
title: Condense only what the reader is near
type: feature
status: done
milestone: v0.3
created: 2026-09-14
updated: 2026-09-14
priority: p0
effort: m
area: condense
---

## Problem

Opening a merged project queued a model call for every message in it — 541 at once on a five-session project, which is real money for lines nobody would scroll to.

## Acceptance criteria

- [x] Condensation reaches a window either side of the selection and no further
- [x] Moving the selection pulls more in
- [x] `moan export` widens the window to everything
