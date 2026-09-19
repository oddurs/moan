---
id: 75
title: Decide how much history to keep
type: feature
status: done
milestone: v0.3
created: 2026-09-14
updated: 2026-09-14
priority: p1
effort: m
area: store
---

## Problem

The archive grew without limit and nothing decided what lean meant.

Measured on a real machine: 445 sessions, 1,052 MB of transcript, of which the newest twenty were already 321 MB. A session count is a poor proxy for disk because sessions differ by two orders of magnitude.

## Proposal

Make size the primary control, default 256 MB, with age and count available and off. Apply it at startup, cheaply.

Exploit an asymmetry: condensations cost money and are tiny; raw events are bulky and, while the transcript still exists, free to read again.

## Acceptance criteria

- [x] `[retain] max_mb / days / sessions`, defaulting to 256 MB
- [x] Applied at startup, doing nothing unless over budget
- [x] `moan prune` with `--max-mb`, `--days`, `--sessions`
- [x] Condensations are never pruned: verified after dropping every session
- [x] A session whose transcript is gone outlives a newer one that can be read again
- [x] One keep-order shared by the size, age and count rules
