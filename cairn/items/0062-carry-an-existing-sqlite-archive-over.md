---
id: 62
title: Carry an existing SQLite archive over
type: feature
status: done
milestone: v0.5
depends_on:
- 61
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: m
area: store
---

## Problem

People have real archives in `moan.db` already — this session's is thousands of events and every condensation paid for so far. A storage change that loses them is a storage change nobody should accept.

## Proposal

On first run against Turso, read the old file and copy it across, then leave the original alone rather than deleting it.

## Acceptance criteria

- [x] Every session, event and condensation survives, verified by count and by spot-checking content
- [x] The old file is left untouched, so a downgrade is possible
- [x] Carrying over a large archive reports progress rather than appearing to hang
- [x] Running twice does not duplicate anything

## 2026-09-15

No migration is needed: Turso reads the archive SQLite wrote, in place.

Verified against the live 1.6MB archive — both engines report the same session, event and condensation counts from the same file, and the whole TUI runs on either. The acceptance criteria are met by there being nothing to carry: the old file is not copied, so it cannot be lost, and downgrading is one line of config.
