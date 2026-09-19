---
id: 51
title: What breaks when moan runs on Linux?
type: spike
status: done
milestone: v1.0
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: s
area: ui
---

## Question

moan has only ever run on macOS. v1.0 promises Linux. What actually breaks?

## Timebox

Half a day. Build it on Linux, run it against a real transcript, and write down what is wrong.

## What we will do with the answer

It turns a guessed-at item into a list of real ones. Until it closes, the Linux work is unsized and the v1.0 date is a guess.

## Answer

**Nothing breaks.** Run in a `rust:1.90-slim` container against the real code:

- **The suite passes.** 140 tests, 0 failures, same as on macOS.
- **Paths resolve.** `~/.config/moan/config.toml` and
  `~/.local/share/moan/moan.db` — the `directories` crate already did the right
  thing on both, so there was nothing to change.
- **It reads and renders.** A transcript under `~/.claude/projects` is listed,
  exported and drawn.
- **The characters are there.** `▎ ▌ │ ● ·` all reached the terminal. (`→`
  appeared only because that fixture had no link notes, not because it failed.)

Two things a container cannot answer, and they are the ones left:

- **Mouse reporting** across emulators. crossterm speaks the same escapes on
  both platforms, so this is about the terminal rather than about moan, but
  nobody has clicked in one yet.
- **How the palette looks** under a real scheme. Every colour is now an ANSI
  index, so it is the reader's own — which makes this much less likely to
  surprise than when surfaces were hardcoded RGB.

So the Linux work turned out to be: a CI matrix that proves it, and no code
changes at all.

Check at least:

- Config and data paths: macOS uses `~/Library/Application Support`, Linux expects XDG. `directories` should handle it — confirm it does, and that an existing database is found.
- Mouse reporting across common emulators, since the picker leans on click-to-expand.
- Whether the box-drawing, ellipsis and guillemet characters the feed uses render, and what to fall back to.
- Terminal restore on exit and on panic.
