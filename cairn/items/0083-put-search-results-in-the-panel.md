---
id: 83
title: Put search results in the panel
type: feature
status: done
milestone: v0.6
assignee: Oddur Sigurdsson
depends_on:
- 79
created: 2026-09-15
updated: 2026-09-15
priority: p0
effort: l
area: ui
---

## Problem

Searching today filters the feed, which means losing the conversation to look for something in it. And it only searches the open session.

## Proposal

A second panel mode. Results across every stored session, grouped by project, with the matching line. Choosing one moves the feed to it and leaves the results up, so a search is something you work through rather than something you redo.

## Acceptance criteria

- [x] Search runs across every stored session
- [x] Results show the session and the matching line
- [x] Choosing a result opens that session with the message selected
- [x] The results stay while you read, so the next one is one keypress away
- [x] Searching a large archive stays interactive
