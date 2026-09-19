---
id: 85
title: Retire the modal picker
type: chore
status: done
milestone: v0.6
assignee: Oddur Sigurdsson
depends_on:
- 80
- 83
created: 2026-09-15
updated: 2026-09-15
priority: p1
effort: s
area: ui
---

## Problem

Two ways to browse sessions is one too many, and they will drift.

## Proposal

Delete the picker once the panel does everything it did, and move `s` to the panel.

## Acceptance criteria

- [x] The picker is gone from the code, not merely unreachable
- [x] Everything it could do, the panel can
- [x] The help and the README describe one way to browse
