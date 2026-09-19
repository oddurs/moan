---
id: 57
title: Mark fresh messages and show Claude composing
type: feature
status: done
milestone: v0.3
created: 2026-09-14
updated: 2026-09-14
priority: p1
effort: m
area: ui
---

## Problem

Coming back to a feed you left running, nothing said which part was new. And after posting there was no sign the assistant had your message until its reply landed whole, which on a long turn is minutes of apparently nothing.

## Acceptance criteria

- [x] A message carries a left-edge accent that fades over three stages to three minutes
- [x] The feed redraws while anything is fading or composing, and stops when neither is
- [x] A composing row appears when nothing has been said since the last prompt, tool calls included
- [x] It gives up on a session that is no longer being written to
