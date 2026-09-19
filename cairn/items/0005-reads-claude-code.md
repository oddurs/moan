---
id: 5
key: v0.1
title: reads Claude Code
type: milestone
status: done
created: 2026-09-14
updated: 2026-09-14
priority: p2
due: 2026-09-14
---

## Ships

Point it at a Claude Code session and read what happened as a compressed feed — one tight line per event, newest at the bottom, expandable to the original text.

## Done when

- [x] Tails `~/.claude/projects/*/*.jsonl` live and survives a half-written line
- [x] Tool calls condense by rule; only long prose costs a model call
- [x] Any provider: OpenAI and Anthropic wire formats, ten presets, local servers keyless
- [x] Events and condensations persist; a gist is paid for once
- [x] Feed with expand, filter, session picker, follow-the-tail
- [x] `sessions`, `export`, `doctor`, `config` subcommands
- [x] README a stranger can start from

## Explicitly not in this milestone

- Any agent other than Claude Code
- Searching across sessions
- Packaging beyond `cargo install --path .`
