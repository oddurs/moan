---
id: 28
title: Mark which of a project's sessions are live
type: feature
status: done
milestone: v0.3
assignee: Oddur Sigurdsson
depends_on:
- 27
created: 2026-09-14
updated: 2026-09-15
priority: p1
effort: s
area: ui
---

## Problem

A session written to ten seconds ago and one from March look identical in the picker.

## Proposal

Mark sessions whose transcript has grown recently as live, so parallel agents are visible at a glance.

## Acceptance criteria

- [x] A session written to within the last few minutes is marked live
- [x] The mark clears when the session goes quiet
- [x] It costs a stat, not a read

## 2026-09-14

The rail already marks live sessions with a filled dot, and so does the picker. What is left is the unread count per chip.

## 2026-09-15

Done as part of the panel: ● for working, ◆ for waiting on a reply, read from the end of each transcript and memoised against its timestamp.
