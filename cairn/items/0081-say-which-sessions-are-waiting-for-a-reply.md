---
id: 81
title: Say which sessions are waiting for a reply
type: feature
status: done
milestone: v0.6
assignee: Oddur Sigurdsson
depends_on:
- 80
created: 2026-09-15
updated: 2026-09-15
priority: p0
effort: m
area: ui
---

## Problem

The reason to watch several agents is to know when one needs you, and nothing says so. A live dot says an agent is working; it does not say one has stopped and is waiting.

## Proposal

Mark a session where the assistant has finished and nothing has been said since. That is the inverse of the composing indicator, which already knows when it is still working.

## Acceptance criteria

- [x] A session whose last word was the assistant's, and which has gone quiet, is marked as waiting
- [x] A session still being written to is marked live instead
- [x] An abandoned session is marked as neither
- [x] The marks are distinguishable without colour
