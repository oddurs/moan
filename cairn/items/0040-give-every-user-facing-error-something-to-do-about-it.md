---
id: 40
title: Give every user-facing error something to do about it
type: chore
status: done
milestone: v1.0
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: cli
---

## Problem

Errors currently range from good to a bare provider string. A stranger hitting a bad model name, an unreadable config, or a missing key should not have to read the source.

## Proposal

Audit every path that can fail in front of a user and make each name the thing that failed and the next action.

## Acceptance criteria

- [x] A bad provider or model names what was configured and where to change it
- [x] A missing key says which variable or file it looked in
- [x] An unparseable config reports the line
- [x] No user-facing path can panic
