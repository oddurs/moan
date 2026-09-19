---
id: 9
key: v1.0
title: the promise
type: milestone
status: backlog
created: 2026-09-14
updated: 2026-09-15
priority: p2
due: 2027-03-15
---

## Ships

A terminal tool a stranger installs with one command on macOS or Linux, points at any of three coding agents, and reads. No new surface — this milestone is about being able to stand behind what is already there.

## Done when

- [ ] Installs and runs on macOS and Linux; CI proves both on every push
- [ ] Claude Code, Codex, and opencode all read correctly
- [ ] A stored database from an older version opens, or migrates, but never corrupts
- [ ] Every error a user can cause names what to do about it
- [ ] `--help`, the README, and the config reference agree with the code
- [ ] Tagged release with prebuilt binaries

## Explicitly not in this milestone

- Writing back into sessions: no replying, no steering, no input box. It stays read-only.
- Any non-terminal UI: no web view, no hosted or shared feed. Markdown export is the escape hatch.
- Cost and token analytics: no spend dashboards, no per-model cost tables.
