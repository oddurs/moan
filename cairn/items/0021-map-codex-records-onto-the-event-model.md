---
id: 21
title: Map Codex records onto the Event model
type: feature
status: done
milestone: v0.2
depends_on:
- 20
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: l
area: source
---

## Problem

Codex records are `session_meta`, `turn_context`, `event_msg` and `response_item` — a different vocabulary from Claude Code's, describing the same story.

## Proposal

Map them onto the existing `Event`: prompts, assistant prose, tool calls with rule-condensed gists, and results folded into the call that produced them. Drop the bookkeeping.

## Acceptance criteria

- [x] A Codex session renders as a feed indistinguishable in shape from a Claude Code one
- [x] Tool calls condense by rule, with no model call
- [x] Reasoning maps to `Kind::Think` and stays hidden by default
- [x] Records the mapping does not recognise are dropped, never rendered as noise
