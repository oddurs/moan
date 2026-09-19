---
id: 98
title: Make a hundred conversations browsable
type: feature
status: done
milestone: v0.6
created: 2026-09-16
updated: 2026-09-16
priority: p0
effort: m
area: ui
---

## Problem

The list held 101 conversations across 66 projects and could only be moved through with arrow keys. It also wasted a blank row between every project — sixty rows of nothing — and said nothing about when a conversation last moved or which agent wrote it.

## Acceptance criteria

- [x] Typing narrows the list by project, branch or agent, and says how much of what is left
- [x] `esc` puts it back
- [x] No blank row between projects: the heading already separates them
- [x] Each row says how long ago it moved
- [x] The agent is named only where it differs from the common one
- [x] The query is visible in both lists, so a list never quietly loses things
