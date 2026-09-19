---
id: 72
title: Make condensation cheap enough not to think about
type: chore
status: done
milestone: v0.3
created: 2026-09-14
updated: 2026-09-14
priority: p0
effort: m
area: condense
---

## Problem

Three separate sources of spend, none of them necessary.

- The reading window was a fixed 60 rows either side, against a viewport of about 28. Four times more was condensed than could be seen.
- Hiding or showing tool calls changed how prose grouped into turns, which changed the content hash, which bought the whole session again on every toggle.
- The default model was `anthropic/claude-haiku-4.5` at $1.00/M in and $5.00/M out — the most expensive of every candidate measured, for work that is not hard.

## Acceptance criteria

- [x] Condensation reaches one screen either side of the reader, taken from the rendered height rather than a number picked in advance
- [x] Prose is grouped into turns over the whole transcript, so toggling tools costs nothing: measured at +4 rather than a full re-buy of 35
- [x] `moan export` asks for the whole conversation through a flag, not a saturating window that overflowed
- [x] Default model chosen by measurement over six real messages: 0.80s median and $0.13/1k against 2.05s and $1.56/1k
- [x] Only the session being viewed is ever read or condensed
