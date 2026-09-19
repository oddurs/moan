---
id: 90
title: One surface for switching, or does the rail earn its row?
type: spike
status: done
milestone: v0.7
assignee: Oddur Sigurdsson
created: 2026-09-15
updated: 2026-09-15
priority: p0
effort: s
area: ui
---

## Question

The panel reaches every conversation on the machine, grouped by project, with live and waiting marks and unread counts. The rail reaches one project's, as chips, in a single row. The panel is a superset of what the rail can *show*. Is it a superset of what the rail is *for*?

## Timebox

Half a day, on a real day's work rather than on a screenshot.

## What we will do with the answer

It is the difference between deleting the rail and keeping two switchers for ever. Two surfaces for one job drift — the modal picker had to be deleted for exactly this, having been overtaken by the panel a day after it was built.

## The case each way

**Delete it.** Two ways to do one thing is the thing this milestone exists to stop. The panel already knows more, and `tab` already opens it.

**Keep it.** It costs one row and is *always* visible; the panel costs thirty columns and is a keystroke away. On a narrow terminal the panel covers the feed entirely, so switching there means losing sight of what you are reading — which is precisely when a one-row strip is worth having.

## Answer

**Delete it.**

The case for keeping it rested on passive display: a row that is always there,
against a panel that costs a keystroke. But switching never needed the rail —
`tab` and `1`–`9` move between a project's conversations whether the panel is
open or shut. What the rail uniquely provided was *seeing* siblings without
asking.

That is worth a few characters, not a row. Where a project has more than one
conversation and any of them is live, the header says so. A reader who wants
more opens the panel, which knows more: every project, not just this one, with
what is waiting and what is unread.

The narrow-terminal argument was the strongest one and it does not survive
either. Below ninety-two columns the panel covers the feed, so switching there
means losing sight of what you are reading — but `tab` and the number keys work
without opening anything, which is exactly the case the rail was supposed to
cover.

So: one surface that shows everything, one set of keys that move without
showing anything, and a few characters in the header for the glance. Three
things become two, and neither of them is a second navigation system.

If the answer is "keep it", say what makes it not a second navigation system — the honest version is that it becomes the narrow-terminal form of the panel rather than a surface of its own, and it should then be built and described that way.
