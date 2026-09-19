---
id: 43
title: Guard the feed's frame time against regression
type: chore
status: done
milestone: v1.0
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p2
effort: m
area: ui
---

## Problem

The feed redraws a 5,800-event session in well under a millisecond today. Merged feeds and cross-session search are exactly the changes that would quietly lose that.

## Proposal

Add a benchmark over a large synthetic session with a threshold CI enforces.

## Acceptance criteria

- [x] A benchmark covers a feed of tens of thousands of events
- [x] CI fails on a large regression
- [x] The number is recorded so it can be argued with

## 2026-09-15

Done during the pass over v0.1, and it caught a real regression: drawing had gone linear in the length of the conversation, 178ms a frame at fifty thousand messages. The test asserts the shape rather than a wall-clock number.
