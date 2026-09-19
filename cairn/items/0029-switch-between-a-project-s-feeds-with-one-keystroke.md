---
id: 29
title: Switch between a project's feeds with one keystroke
type: feature
status: done
milestone: v0.3
depends_on:
- 27
created: 2026-09-14
updated: 2026-09-14
priority: p0
effort: m
area: ui
---

## Problem

Watching three agents on one project means opening the picker, finding the session, and opening it, three times over.

## Proposal

Cycle through the open project's sessions with a single key in either direction, keeping each feed's scroll position.

## Acceptance criteria

- [x] One key moves to the next session in the project, another to the previous
- [x] Returning to a feed restores where you were reading
- [x] The header says which of the project's sessions is open
