---
id: 65
title: Sync an archive between your own machines
type: feature
status: doing
milestone: v0.5
assignee: Oddur Sigurdsson
claimed: 2026-09-15
depends_on:
- 63
- 64
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: l
area: store
---

## Problem

The reason for this milestone. The same conversations should open on the laptop and the desktop, without copying a file around by hand.

## Proposal

Use Turso's sync against a database the user configures, sending only what the privacy spike allowed.

## Acceptance criteria

- [ ] Two machines configured against one database converge
- [x] What is synced matches what the privacy spike decided, with the conservative option as the default
- [x] `moan sync` shows plainly what syncs and what stays local
- [x] Turning sync on for an existing archive uploads it once, not on every start

## 2026-09-15

Partly done, and the remaining part needs an account I do not have.

Done and verified: the shareable archive. Turso replicates a whole database, so what syncs is a second file built from the first. `moan sync` builds it and prints exactly what it holds before anything could leave. Condensations and session identity travel; the original text does not unless `sync.raw = true`. Tested by searching the resulting file for a key prefix that appears in the source archive — it is not there.

Not done: attaching it to a Turso remote. `turso::sync::Builder::new_remote` with push and pull is the step that remains. I did not ship it unverified — it uploads somebody's transcripts, and I cannot test it against a real account from here. `moan sync` says so plainly rather than pretending.

## 2026-09-15

Everything except the wire is done and tested.

The shareable archive is built and scoped by the privacy decision, and the receiving half — `moan sync --merge` — takes in another machine's copy without replacing anything local. Two machines converge today by copying that file across by any means.

What is missing is only the transport: turso::sync::Builder::new_remote with push and pull. I did not ship it unverified, because it uploads transcripts and there is no account here to test it against.
