---
id: 91
title: Say conversation, project and message
type: docs
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
created: 2026-09-15
updated: 2026-09-15
priority: p0
effort: m
area: cli
---

## Problem

`moan config` reports "821 events · 47 condensations". `moan export` writes "20 messages" but `moan sessions` lists sessions. `event`, `gist` and `session` are the parser's words and they have leaked into everything the reader sees.

## Proposal

Fix the vocabulary at the surface only. A **project** holds **conversations**; a conversation is made of **messages**; what the model writes is a **summary**. The code keeps `Event`, `Session` and `gist`, which are good names for what they are.

## Acceptance criteria

- [x] Nothing printed, rendered or documented says session, event or gist
- [x] `moan sessions` becomes `moan conversations`, with the old name still working
- [x] The help and the README use one vocabulary throughout
- [x] A test greps the user-facing strings for the old words
