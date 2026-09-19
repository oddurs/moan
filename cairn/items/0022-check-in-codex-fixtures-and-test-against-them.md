---
id: 22
title: Check in Codex fixtures and test against them
type: chore
status: done
milestone: v0.2
assignee: Oddur Sigurdsson
depends_on:
- 21
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: source
---

## Problem

The Claude Code parser is trusted because tests pin its behaviour. Codex has no such net, and its format will move.

## Proposal

Reduce two real rollouts to small redacted fixtures in the repo and assert the same properties the Claude Code tests assert.

## Acceptance criteria

- [x] Fixtures contain no paths, keys, or prose from real work
- [x] Tests cover cruft filtering, result folding, and a half-written tail
- [x] A format change breaks a test rather than silently emptying the feed

## 2026-09-15

The fixtures are inline consts in the test module rather than separate files — redacted by construction, since they were written rather than copied. Alongside them, real_rollouts_still_parse runs the parser over whatever is on the machine and asserts the invariants that matter, so a format change fails loudly instead of producing an empty feed. The same guard now covers Claude Code.
