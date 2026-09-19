---
id: 23
title: Degrade instead of panicking on a bad transcript
type: bug
status: done
milestone: v0.2
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p1
effort: s
area: source
---

## What happens

A transcript that is rotated, truncated, replaced mid-read, or not valid UTF-8 can take the whole TUI down, because parse errors propagate to the poll loop.

## What should happen

The feed keeps what it has, the status bar says what failed, and the next poll retries. Nothing a file on disk does should end the process.

## Reproduction

1. Open a live session
2. `truncate -s 100` the transcript underneath it
3. Watch the next poll
