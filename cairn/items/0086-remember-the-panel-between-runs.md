---
id: 86
title: Remember the panel between runs
type: chore
status: dropped
milestone: v0.6
depends_on:
- 84
created: 2026-09-15
updated: 2026-09-15
priority: p2
effort: s
area: ui
---

## Problem

Somebody who works with it open should not open it every morning.

## Proposal

Keep whether it is open, how wide, and which mode, alongside the other interface settings.

## Acceptance criteria

- [ ] Open or closed survives a restart
- [ ] A width the user set survives a restart
- [ ] A width that no longer fits the terminal is clamped rather than breaking the layout

## 2026-09-15

Superseded by 0094 in v0.7, which remembers the conversation, the message and the panel together. They are one thing — where you were — and splitting them across two milestones would mean two half-restorations.
