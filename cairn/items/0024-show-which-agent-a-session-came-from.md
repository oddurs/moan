---
id: 24
title: Show which agent a session came from
type: feature
status: done
milestone: v0.2
depends_on:
- 20
created: 2026-09-14
updated: 2026-09-15
priority: p2
effort: s
area: ui
---

## Problem

With two sources, a session list that does not say which tool wrote a session makes the reader guess.

## Proposal

Mark the source in the picker and in the header, so a Codex feed is never mistaken for a Claude Code one.

## Acceptance criteria

- [x] The picker shows the source per session
- [x] The header names the source of the open session
