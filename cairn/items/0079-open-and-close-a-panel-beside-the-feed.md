---
id: 79
title: Open and close a panel beside the feed
type: feature
status: done
milestone: v0.6
assignee: Oddur Sigurdsson
depends_on:
- 78
created: 2026-09-15
updated: 2026-09-15
priority: p0
effort: m
area: ui
---

## Problem

The feed owns the whole width. A panel needs somewhere to be, without the feed reflowing every time it opens.

## Proposal

Split the body horizontally, panel on the left. Below a threshold width it overlays instead, the way the picker does now, because a 30-column panel on an 80-column terminal leaves nothing to read.

## Acceptance criteria

- [x] A key opens and closes it
- [x] The feed keeps its scroll position and selection across a toggle
- [x] Under about 90 columns it overlays rather than splitting
- [x] Which pane has focus is visible without having to press anything
