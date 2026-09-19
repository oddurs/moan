---
id: 93
title: Let the conversation be the interface
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
created: 2026-09-15
updated: 2026-09-15
priority: p1
effort: m
area: ui
---

## Problem

A header, a rail, a hairline, a scrollbar, a divider, and a footer that says the same nine things whatever is happening. The chrome is on every edge and most of it is not carrying anything.

## Proposal

Keep what changes, drop what does not. The hairline separates two things that are already separated by being different. The key bar is a beginner's crutch that never leaves; `?` is the place for that. The scrollbar means something only when there is more than a screen.

## Acceptance criteria

- [x] The hairline is gone
- [x] The footer carries status when there is status, and otherwise nothing
- [x] The scrollbar appears only when the conversation is taller than the pane
- [x] A first run still tells somebody how to find the keys
