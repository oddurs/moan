---
id: 34
title: Map opencode session, message and part rows onto the Event model
type: feature
status: done
milestone: v0.4
assignee: Oddur Sigurdsson
depends_on:
- 33
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: l
area: source
---

## Problem

opencode splits a message into `part` rows of differing kinds. That structure has to become the same flat feed as the other two agents.

## Proposal

Assemble parts into messages, map tool parts to rule-condensed tool events, fold results into their calls, and drop the bookkeeping.

## Acceptance criteria

- [x] An opencode session renders as a feed indistinguishable in shape from the others
- [x] Tool calls condense by rule
- [x] A message whose parts arrive across polls is not rendered twice
