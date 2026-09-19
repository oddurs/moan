---
id: 96
title: Make the empty states teach
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
created: 2026-09-15
updated: 2026-09-15
priority: p2
effort: s
area: ui
---

## Problem

A first run with no transcripts exits with an error about `~/.claude/projects`. An empty panel and an empty search say nothing useful.

## Proposal

Each empty state says what would fill it and what to do next.

## Acceptance criteria

- [x] No transcripts explains what moan reads and where it looks
- [x] A search with no matches says what was searched
- [x] A conversation still being read says so rather than looking finished

## 2026-09-15

Overlaps nothing in v0.6: the panel's own empty states are covered here rather than there, because they are the same problem as the first-run one.
