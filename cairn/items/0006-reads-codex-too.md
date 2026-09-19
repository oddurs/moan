---
id: 6
key: v0.2
title: reads Codex too
type: milestone
status: backlog
created: 2026-09-14
updated: 2026-09-14
priority: p2
due: 2026-09-28
---

## Ships

The same feed for Codex sessions. One session list spanning both agents; you pick a session without caring which tool wrote it.

## Done when

- [ ] Codex rollout files parse into the same `Event` model
- [ ] `moan sessions` lists Claude Code and Codex together, newest first
- [ ] The `Source` cursor is general enough for a store that is not an append-only file
- [ ] Codex parsing is covered by fixtures checked into the repo
- [ ] A rotated, truncated, or unreadable transcript degrades instead of panicking

## Explicitly not in this milestone

- opencode (its store is SQLite; the spike here only decides the shape)
- Grouping sessions by project
- Linux CI
