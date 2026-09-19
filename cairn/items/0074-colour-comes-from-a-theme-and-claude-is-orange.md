---
id: 74
title: Colour comes from a theme, and Claude is orange
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

Colour was half ANSI and half hardcoded RGB. ANSI inherits whatever palette the reader has chosen; RGB does not. On a light terminal the selection was near-black and the chrome was invisible, and nothing said which was which.

## Acceptance criteria

- [x] Roles use ANSI so they sit with the rest of the terminal; surfaces are defined per palette
- [x] Claude is orange — 256-colour rather than truecolor, so it survives a terminal without 24-bit
- [x] Dark and light palettes, chosen from COLORFGBG, overridable with `ui.theme`
- [x] A test fails if any colour in the renderer bypasses the theme
- [x] Both palettes are rendered in tests
- [x] Fixed along the way: `show_tools` still defaulted to true in code — only the config file had been changed, so a fresh install showed tool calls
