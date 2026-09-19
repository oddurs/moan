---
id: 84
title: Make the panel hold more than one thing
type: chore
status: done
milestone: v0.6
assignee: Oddur Sigurdsson
depends_on:
- 83
created: 2026-09-15
updated: 2026-09-15
priority: p1
effort: s
area: ui
---

## Problem

Sessions and search are the first two. A digest of alerts across a project, or whatever comes next, should not mean rebuilding the panel.

## Proposal

One trait for a panel mode: a title, rows, what happens on select. The panel holds a list of them and a key moves between.

## Acceptance criteria

- [x] Adding a mode does not touch the layout or the focus handling
- [x] The panel says which mode it is in and how to reach the others
- [x] The mode is remembered between runs

## 2026-09-15

Partly there: the panel already switches between sessions and search with h/l and remembers which it is in for the run. What is left is the trait, so a third mode does not mean touching the layout.

## 2026-09-15

Table-driven rather than a trait: MODES lists each mode and its name, and the tabs, the switching keys and the cycling are all written against the list. A trait with two implementations that share their entire structure would have been ceremony. Adding a third mode is a variant and a line.
