---
id: 20
title: List Codex sessions alongside Claude Code's
type: feature
status: done
milestone: v0.2
assignee: Oddur Sigurdsson
depends_on:
- 19
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: source
---

## Problem

Codex writes rollouts to `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`. moan cannot see them.

## Proposal

Add a `CodexSource` that enumerates rollout files and reads each session's real `cwd` from its `session_meta` record, the same way the Claude Code source does.

## Acceptance criteria

- [x] `moan sessions` lists Codex sessions with `codex` in the source column
- [x] The title is the session's real working directory, not a path guessed from the filename
- [x] An empty or truncated rollout file is skipped, not fatal
