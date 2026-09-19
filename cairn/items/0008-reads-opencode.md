---
id: 8
key: v0.4
title: reads opencode
type: milestone
status: backlog
created: 2026-09-14
updated: 2026-09-14
priority: p2
due: 2026-11-09
---

## Ships

opencode sessions in the same feed as Claude Code and Codex. All three agents, one tool.

## Done when

- [ ] opencode's SQLite store is read incrementally, without re-scanning it each poll
- [ ] Its `session` / `message` / `part` rows map onto the same `Event` model
- [ ] A concurrent opencode write is never read as a torn record
- [ ] Covered by fixtures, same as the other two sources

## Explicitly not in this milestone

- Any fourth agent
- Reading opencode's own config for model settings
