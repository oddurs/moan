---
id: 78
title: Where does the keyboard go when there are two panes?
type: spike
status: done
milestone: v0.6
assignee: Oddur Sigurdsson
created: 2026-09-15
updated: 2026-09-15
priority: p0
effort: s
area: ui
---

## Question

Every key today acts on the feed. With a panel open there are two things that can be moved through, and the keymap has no room left: `s`, `tab`, `1`-`9`, `j`/`k` and `/` all already mean something.

## Timebox

One day. Build the layout as a stub with no content and try the three models against a real session before writing any panel code.

## What we will do with the answer

It decides the keymap, and the keymap is the thing that cannot be changed later without annoying everybody who learned it.

## Candidates

- **Focus toggles.** `tab` moves between panel and feed; `j`/`k` act on whichever has it. Familiar from tmux and vim, and needs a visible marker of which pane is live. Costs `tab`, which cycles sessions today.
- **The panel is a list you drive from the feed.** No focus at all: dedicated keys move the panel selection while the feed keeps `j`/`k`. Nothing to learn, but it needs a second set of movement keys and runs out of letters.
- **The panel takes focus while it is open.** Opening it is how you say "I want to navigate"; `esc` gives focus back and closes. Simplest model, but you cannot scroll the feed while looking at the list, which is half the point.

## Answer

**Focus toggles.**

The other two fail on something specific rather than on taste.

*The panel takes focus while open* is the modal picker wearing a different shape. You cannot scroll the feed while looking at the list, and reading one project while watching another is the entire reason for the milestone.

*The panel as a driven list* needs a second movement vocabulary, and it breaks on one key in particular: `⏎`. In the feed it expands a message; in the panel it opens a session; in search results it jumps to a hit. Without focus there is nothing to disambiguate it, and every candidate for a second Enter is worse than the first.

Focus makes every key mean one thing in each pane, which is why tmux and vim both landed there.

### The keymap

`tab` cycles a project's sessions today. Once the panel exists, that is what the panel is *for*, so `tab` is not being taken away from anything — it is being pointed at the thing that replaced it. `1`–`9` stay as the quick jump for people who liked it.

| key | panel closed | panel open, feed focused | panel open, panel focused |
|---|---|---|---|
| `s` | open the panel | close it | close it |
| `tab` | next session in project | focus the panel | focus the feed |
| `shift-tab` | previous session | focus the panel | focus the feed |
| `j` `k` `↑` `↓` | move in the feed | move in the feed | move in the panel |
| `⏎` | expand a message | expand a message | open the session or result |
| `/` | search this session | search this session | switch the panel to search |
| `esc` | clear the filter | clear the filter | focus the feed, panel stays open |
| `1`–`9` | jump to a session | jump to a session | jump to a session |
| everything else | unchanged | unchanged | unchanged |

`tab` is the only key whose meaning depends on state, and it depends on one bit that is visible on screen.

### Showing which pane has it

Two signals, because colour alone is not enough:

- the divider is bright along the focused pane and dim along the other
- the focused pane's selection is a filled highlight; the unfocused pane's is an accent bar with no background, so a selection is still visible but plainly not live

Say which, write out the resulting keymap in full, and say what `tab` does in each of: panel open, panel closed, one session, several.
