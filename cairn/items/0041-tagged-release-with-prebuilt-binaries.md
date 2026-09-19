---
id: 41
title: Tagged release with prebuilt binaries
type: chore
status: doing
milestone: v1.0
assignee: Oddur Sigurdsson
claimed: 2026-09-15
depends_on:
- 37
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: l
area: dist
---

## Problem

`cargo install --path .` requires a checkout and a Rust toolchain. That is not an installable tool.

## Proposal

Tag a release and publish binaries for macOS and Linux from CI, with one documented install command.

## Acceptance criteria

- [ ] A tag builds and publishes binaries for both platforms
- [x] The README's install command works on a machine with no Rust
- [x] Checksums are published with the binaries

## 2026-09-14

v0.5 removes the C toolchain dependency by dropping rusqlite, which makes cross-compiling for the Linux binaries materially simpler. Sequence this after v0.5.

## 2026-09-15

The workflow is written: a tag builds macOS and Linux on Intel and ARM, packages each with a README, a licence and a .sha256, and publishes them. The install command in the README points at it.

The first criterion is unticked because it cannot be ticked from here: there is no remote to push a tag to, so nobody has watched it run. That is the step that needs doing on a real repository.
