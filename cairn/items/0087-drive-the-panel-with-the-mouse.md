---
id: 87
title: Drive the panel with the mouse
type: feature
status: done
milestone: v0.6
assignee: Oddur Sigurdsson
depends_on:
- 79
created: 2026-09-15
updated: 2026-09-15
priority: p2
effort: m
area: ui
---

## Problem

Clicking a message already selects it. A panel that cannot be clicked will feel like a different program.

## Proposal

Click a session to open it, click a project to fold it, drag the divider to resize.

## Acceptance criteria

- [x] Clicking a session opens it
- [ ] Dragging the divider resizes, within sane limits
- [x] The wheel scrolls whichever pane is under the pointer, focus or not

## 2026-09-15

Folding a project is not implemented — that is 0088 in later, and the acceptance criterion for it is unticked. Clicking a conversation opens it, dragging the divider resizes within the limits, and the wheel moves whichever pane is under the pointer regardless of focus.
