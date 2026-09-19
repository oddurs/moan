# Changelog

Notable changes, newest first. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Reads Claude Code, Codex and opencode sessions, live, with the newest message
  at the bottom.
- Rule-based condensation for tool calls, which costs nothing, and model-based
  condensation for prose, bought once per content hash and cached forever.
- A panel that browses every conversation on the machine, grouped by project,
  with search across everything already read.
- Notes under a message — alert, detail, link — lifted out of the summary so
  what matters survives being read at a glance.
- `moan export` builds a shareable archive and says exactly what is in it;
  `moan sync --merge` takes one in from another machine.
- `moan config` reports what condensation has cost, counted by the provider
  rather than estimated, and the conversation list marks the ones a model has
  been paid to summarise.
- Messages that look like they hold a credential are summarised locally and
  never sent.

[Unreleased]: https://github.com/oddurs/moan/commits/main
