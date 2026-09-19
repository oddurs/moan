---
id: 80
title: List every session, grouped by project
type: feature
status: done
milestone: v0.6
assignee: Oddur Sigurdsson
depends_on:
- 79
created: 2026-09-15
updated: 2026-09-15
priority: p0
effort: m
area: ui
---

## Problem

The rail shows one project. The picker shows everything but covers the feed. Neither lets you watch another project while reading this one.

## Proposal

The panel's first mode: every project, most recently active first, its sessions beneath it, the open one marked.

## Acceptance criteria

- [x] Every session on the machine is reachable
- [x] The project you are in sorts first
- [x] Selecting a session opens it in the feed without closing the panel
- [x] A project with one session does not waste a line on a heading
