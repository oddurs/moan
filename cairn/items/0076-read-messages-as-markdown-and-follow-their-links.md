---
id: 76
title: Read messages as markdown, and follow their links
type: feature
status: done
milestone: v0.3
created: 2026-09-14
updated: 2026-09-14
priority: p1
effort: l
area: ui
---

## Problem

Assistant prose is markdown, and it was shown raw: asterisks and backticks in front of the words, headings as `##`, and links reduced to text with the destination thrown away.

## Acceptance criteria

- [x] Inline emphasis, code and links become styling in the feed and in an expanded message
- [x] Headings, lists, quotes and rules take their shape; fenced code is left exactly as typed
- [x] `snake_case` and arithmetic are not mistaken for emphasis
- [x] Anything unrecognised survives untouched
- [x] A link shows its destination as well as its words, and a url is never broken across lines while it fits
- [x] `O` opens the first link in a message, and only an http or https address
- [x] A message with no link says so rather than guessing

## Note

Neither ratatui nor crossterm can emit OSC 8 hyperlinks, so a rendered cell cannot itself be a link. Printing urls literally lets the terminal's own detection do it — which is what iTerm2, Kitty, WezTerm, Ghostty and Windows Terminal all offer — and `O` covers the rest.
