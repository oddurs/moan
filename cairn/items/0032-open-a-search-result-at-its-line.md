---
id: 32
title: Open a search result at its line
type: feature
status: done
milestone: v0.3
assignee: Oddur Sigurdsson
depends_on:
- 31
created: 2026-09-14
updated: 2026-09-15
priority: p1
effort: m
area: ui
---

## Problem

A search that tells you a match exists but not how to get to it makes you do the work twice.

## Proposal

Selecting a result opens that session with the matching event selected and in view.

## Acceptance criteria

- [x] Enter on a result opens its session at that event
- [x] The event is selected, not merely scrolled past
- [x] Going back returns to the results

## 2026-09-15

Done as part of the panel: choosing a result opens its conversation and selects the message, and the results stay up.
