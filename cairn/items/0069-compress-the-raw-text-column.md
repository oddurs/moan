---
id: 69
title: Compress the raw text column
type: feature
status: dropped
milestone: v0.5
created: 2026-09-14
updated: 2026-09-15
priority: p1
effort: m
area: store
---

## Problem

Measured on a five-session project: the archive is 19MB, and 18MB of it is the `events.raw` column. The rest — indexes, sessions, every condensation ever paid for — is under 1.4MB. Across a hundred sessions this is hundreds of megabytes of mostly-English text held uncompressed.

## Proposal

Compress `raw` on write and decompress on read. Use a pure-Rust compressor, not a C one: v0.5 exists partly to drop the C toolchain, and reintroducing it for this would undo that.

Deliberately not done during the v0.3 storage pass, because this milestone rewrites the storage layer onto Turso and building it twice is waste.

## Acceptance criteria

- [ ] `raw` is stored compressed, with the codec recorded so it can change later
- [ ] An archive written uncompressed still reads
- [ ] Measured before and after on a real multi-session archive
- [ ] Opening a large session is no slower, measured
- [ ] No C compiler is introduced
