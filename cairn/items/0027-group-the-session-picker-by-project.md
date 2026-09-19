---
id: 27
title: Group the session picker by project
type: feature
status: done
milestone: v0.3
depends_on:
- 26
created: 2026-09-14
updated: 2026-09-14
priority: p0
effort: m
area: ui
---

## Problem

The picker is a flat list of every session on the machine, newest first. With several agents running it is a wall.

## Proposal

Group by project, ordered by most recent activity, with each project's sessions nested under it and labelled by branch and agent.

## Acceptance criteria

- [x] Sessions are grouped under their project
- [ ] Collapsing a project hides its sessions
- [x] The project containing the current directory sorts first

## 2026-09-14

Collapsing is not implemented: the list shows every session under its project. Split out rather than faked — with a per-project count in the caption the list stays readable, and collapse is only worth it once somebody has more projects than fit.
