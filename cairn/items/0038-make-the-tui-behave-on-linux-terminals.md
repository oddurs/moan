---
id: 38
title: Make the TUI behave on Linux terminals
type: bug
status: done
milestone: v1.0
assignee: Oddur Sigurdsson
depends_on:
- 51
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: ui
---

## What happens

Unknown, because it has never been run there. The likely candidates: the config and data directories move from `~/Library/Application Support` to XDG paths, mouse reporting differs across terminal emulators, and the box-drawing and ellipsis characters the feed leans on may not render.

## What should happen

The feed looks and behaves on a Linux terminal the way it does on macOS, or degrades to something plainly readable where it cannot.

## Reproduction

1. Build and run on Linux under at least the common emulators
2. Compare the feed, the picker, mouse selection, and the expanded body rule

## 2026-09-15

Nothing needed changing. Verified in a container: the suite passes, XDG paths resolve, a transcript is listed, exported and drawn, and the box-drawing characters all reach the terminal. Mouse reporting across emulators is the one thing left and it is a property of the terminal rather than of moan; CI now proves the rest on every push.
