---
id: 89
key: v0.7
title: one way in
type: milestone
status: backlog
created: 2026-09-15
updated: 2026-09-15
priority: p2
due: 2027-02-08
---

## Ships

One way in. moan grew three ways to move between conversations, two vocabularies for the same things, and chrome on every edge. This takes it back to one of each.

## What is actually wrong

**Three navigation systems.** A rail along the top for one project's sessions, a panel down the side for every session on the machine, and a merged view that interleaves them. The panel is a superset of the rail: everything the rail can reach, the panel can, grouped and with more to say about it. Two surfaces for one job will drift, and the modal picker already had to be deleted for the same reason.

**The parser's words leaked into the product.** `moan config` says "821 events · 47 condensations". Nobody has an event. They have a conversation, in a project, made of messages. `session`, `event` and `gist` are words from the code and belong there.

**Chrome on every edge.** A header, a rail, a hairline, a scrollbar, a footer that never changes, and a divider. The conversation is the interface; most of the rest can go, and what stays should appear when it means something.

**It forgets.** Closing and reopening puts you at the newest message of whatever session sorted first, with the panel shut. It should put you back where you were.

**It narrates its own machinery.** `◌ condensing 44` tells you about a worker pool. That the feed is still improving is worth knowing; the number of outstanding HTTP requests is not.

## Done when

- [ ] One surface for moving between conversations, not three
- [ ] Nothing user-facing says session, event or gist
- [ ] Chrome appears when it carries information and not otherwise
- [ ] Reopening puts you where you were
- [ ] Nothing on screen describes the implementation

## Explicitly not in this milestone

- Throwing away the panel. A sidebar that splits on a wide terminal and covers a narrow one is the right shape; it is the rail that is redundant.
- New features of any kind. This milestone only removes.
- Renaming anything in the code. `Event` and `Session` are good names for what they are.
