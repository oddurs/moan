---
id: 68
title: Stop paying for a turn that is still arriving
type: feature
status: dropped
milestone: v0.3
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: condense
---

## Problem

A turn lands as several text blocks. Because the blocks are fused into one message, every block changed the message's content hash and bought a fresh condensation — ten calls for one message, of which only the last answer was right. Opening a five-session merged project cost roughly 600 calls.

## Acceptance criteria

- [x] The newest message is left alone until it has gone 20s untouched
- [x] Anything older is condensed immediately, since it cannot still be growing
- [x] A settled turn is picked up on the next tick, once per lull rather than every tick
- [x] The reading window narrowed from 120 either side to 60
- [x] Measured: opening a 5-session merged project fell from ~600 condensations to 61

## 2026-09-15

Dropped. It was filed when the archive was unbounded; retention now caps it at 256MB by default, so the problem it solved is already solved.

Building it anyway would cost something real: search runs LIKE over events.raw, and a compressed column cannot be searched. Trading the ability to find a command you once ran, for space that is already bounded, is a bad trade. If the cap is ever raised far enough that 256MB of text is the constraint, this comes back — with a searchable projection alongside it.
