---
id: 94
title: Put me back where I was
type: feature
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
created: 2026-09-15
updated: 2026-09-15
priority: p0
effort: m
area: ui
---

## Problem

Closing and reopening lands you at the newest message of whichever conversation sorted first, with the panel shut and the tools hidden again. Every session starts by navigating back to where the last one ended.

## Proposal

Remember the conversation, the message you were on, and whether the panel was open — beside the archive, not in the config, because it is a position rather than a preference.

## Acceptance criteria

- [x] Reopening restores the conversation and the message
- [x] The panel is as it was left, open or shut
- [x] A conversation that has gone since opens the nearest one rather than failing
- [x] `moan -s` still overrides all of it
