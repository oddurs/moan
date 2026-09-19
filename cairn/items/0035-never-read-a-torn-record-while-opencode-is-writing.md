---
id: 35
title: Never read a torn record while opencode is writing
type: bug
status: done
milestone: v0.4
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: source
---

## What happens

opencode writes to its database while moan reads it. A message can be observed with only some of its parts present, and a feed line built from it would be wrong and then stay wrong, because the cursor has already moved past it.

## What should happen

A message is only turned into events once it is complete, or it is revised in place when the rest of it arrives. Nothing half-read is ever committed to the store as final.

## Reproduction

1. Start an opencode session and keep it working
2. Open the same session in moan
3. Watch a long assistant message land

## 2026-09-15

The cursor is a time rather than a position, and a tool still running is not read at all until its row is written again with a result. Tested by starting one, reading, finishing it and reading again. The watermark is inclusive so a part written in the same millisecond is never lost; re-reading costs nothing because the archive will not insert a row twice.
