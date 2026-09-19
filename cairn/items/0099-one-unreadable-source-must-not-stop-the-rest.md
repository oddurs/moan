---
id: 99
title: One unreadable source must not stop the rest
type: bug
status: done
milestone: v0.4
created: 2026-09-16
updated: 2026-09-16
priority: p0
effort: s
area: source
---

## What happened

`moan` exited 101 when opencode's database could not be opened — busy, missing, or unreadable. `OpencodeSource::list` propagated the error through `refresh_sessions` to main, so one source being momentarily unavailable stopped the whole program listing anything.

It also made `ui::render` tests fail intermittently, because one of them read the live filesystem.

## Acceptance criteria

- [x] A source that cannot be read contributes nothing and the others still list
- [x] The status says which source could not be read, without treating it as a failure
- [x] Verified by making the database unreadable: the other two agents still list, exit 0
- [x] The tests no longer read live transcripts, so their result does not depend on what else is running
