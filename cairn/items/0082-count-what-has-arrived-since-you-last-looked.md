---
id: 82
title: Count what has arrived since you last looked
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
area: store
---

## Problem

A session you have not opened for an hour looks the same as one you read a minute ago.

## Proposal

Keep a read watermark per session — the last message you had on screen — and count what has arrived past it. Stored, so it survives a restart.

## Acceptance criteria

- [x] Each session shows how many messages have arrived since you last read it
- [x] Opening a session and reading to the end clears its count
- [x] The count survives closing and reopening moan
- [x] An alert among the unread is called out separately from ordinary messages
- [ ] The watermark costs one row per session, not one per message

## 2026-09-15

A session never opened has nothing stored to count, so the panel marks it unread with a dot rather than inventing a number. The watermark is one column on the session row, not a row per message.
