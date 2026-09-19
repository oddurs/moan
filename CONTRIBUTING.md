# Contributing

Thanks for looking. This is a small program with strong opinions; the fastest
way to get a change in is to match the ones already here.

## Before you start

The backlog is in `cairn/items/`, rendered to [ROADMAP.md](ROADMAP.md). If you
are picking something up, say so on the issue first — several items look
independent and are not.

## The one command

```sh
./scripts/task check
```

`fmt:check`, `lint` with warnings denied, and the full suite. CI runs exactly
this verb on macOS and Linux, and so do the git hooks, so the three cannot
drift apart. Install the hooks once:

```sh
git config core.hooksPath .githooks
```

Never `--no-verify`, and never `|| true` to make something pass. If a check is
wrong, fix the check.

## What a change looks like

- **One unit of work, one branch, one pull request.** Branch `<type>/<slug>`
  where type is one of `feat fix chore docs perf refactor test`. Where the
  work has a cairn item, the slug starts with its id: `fix/0041-attach-to-a-terminal`.
- **Conventional Commits.** Imperative, subject ≤ 72 characters, no trailing
  full stop. The body explains *why*; the diff already says what.
- **A bug fix arrives with the test that would have caught it.** Not a test
  that passes now — the one that would have failed before.
- **Comments say why.** The code already says what.

## House style

Beyond `rustfmt` and `clippy`, which are not up for discussion:

- `unwrap` and `expect` belong in tests and in `main`. If an invariant makes
  one safe elsewhere, write the invariant as the `expect` message.
- Prefer the standard library. A dependency earns its place or does not get
  added.
- Handle an error where you can act on it; otherwise propagate. Never swallow
  one.
- No dead scaffolding: no TODO stubs, no commented-out code, no abstraction
  for a second caller that does not exist.

## Things that will get a change sent back

- **A message that could hold a credential being sent to a model.** See
  `src/secrets.rs`. The rule is refusal, not redaction: a redaction that
  misses looks safe. New detectors are welcome; loosening one needs a reason.
- **Spending someone's money by surprise.** Condensation is bought once per
  content hash, only for what the reader is looking at, and only after the
  session settles. A change that widens any of those three needs to say so in
  the pull request.
- **A schema change without a migration.** `migrate()` in `src/store.rs` runs
  on every open; an archive written by an older build must still open.
- **Bumping `GIST_VERSION` or `PARSER_VERSION` casually.** Each one invalidates
  every cached summary and re-buys them at the user's expense. Say why in the
  commit body.

## Pull requests

State the problem, the approach, and what a reviewer should look at
sceptically. Say where it is weak rather than letting a reviewer find it.
