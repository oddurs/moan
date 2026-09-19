---
id: 63
title: Work with no account, no network, no config
type: feature
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
depends_on:
- 61
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: store
---

## Problem

moan reads what is already on your disk. Needing a login to look at it would be absurd, and a tool that degrades when the network does is not a local tool.

## Proposal

A local Turso file is the default and needs nothing configured. Sync is something you turn on.

## Acceptance criteria

- [x] A fresh install with no config and no network opens a feed
- [x] No Turso account is needed to use any part of moan
- [x] Sync being unreachable is a status line, never a failure to start
- [x] Disabling sync afterwards leaves a working local archive
