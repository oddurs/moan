# Why the archive moved to Turso

*Decided 2026-09-15. Supersedes nothing; the previous engine is still available.*

## What changed

moan kept its archive in SQLite through `rusqlite`. It now keeps it in Turso by
default, with SQLite still selectable through `store.engine`.

## What it bought

**No C toolchain.** `rusqlite` with `bundled` compiles SQLite's C. The `turso`
crate is pure Rust. That matters for the Linux binaries and the cross-compiling
that v1.0 promises, and it removes the single reason moan needed a C compiler.

**A path to syncing.** The reason the milestone existed. An archive that only
exists on one machine is half a tool when you work on two.

**Nothing to migrate.** Turso reads the file SQLite wrote, in place. Verified
against a live archive: both engines report the same sessions, events and
condensations from the same file, and the whole interface runs on either.
Changing back is one line of config, not a migration.

## What it cost

**It cannot give disk space back.** Turso 0.8.0-pre.11 has no `VACUUM`, no
`auto_vacuum`, and `incremental_vacuum` is a silent no-op — measured: 740KB of
test data stayed 740KB after deleting every row, with 211 pages sitting free.

The archive therefore plateaus rather than shrinks. Retention still works,
because `Store::size` counts pages *in use* rather than pages allocated;
counting allocated pages would have made pruning chase a number that never
falls and delete for ever. `moan config` reports both, so the difference is
never a surprise.

**It is pre-release.** This is the component holding durable data. That is why
both engines stayed, why the store's SQL is written once above them, and why
the key store tests run against both — a port that passed on one engine only
would not have been a port.

**Sync cannot be selective.** `push` and `pull` replicate a whole database;
there is no way to hold one column back. So what syncs is a second, smaller
archive built from the first, and `sync.raw` decides whether the original text
goes into it. It does not, by default: `events.raw` holds every shell command
the agent ran, and this project's own transcript contains a `grep` for an API
key pattern along with the matched prefix in its output. Turso can encrypt what
it stores, but it sends the key to the server in a header — which protects the
data from third parties, not from the service.

**It cannot be opened by two processes.** Turso takes an exclusive lock on the
file, so a second moan started against a live archive dies immediately. The
builder offers `experimental_multiprocess_wal`, which does not yet work: with it
on, three instances produced one clean failure, one "shared WAL coordination
file is smaller than the coordination header", and one panic.

That breaks a feature that works on SQLite and is tested there — several moans
reading one archive, with a claim table so two of them never pay to condense the
same message.

## Which is why SQLite is still the default

Turso is implemented, tested against the same suite, and one config line away.
It is not the default, because today it would trade two things moan already does
for one it cannot do yet:

| | SQLite | Turso |
|---|---|---|
| several instances, one archive | yes | no — exclusive lock |
| gives disk back when pruned | yes | no — plateaus |
| no C toolchain | no | yes |
| syncs between machines | no | the reason it exists |

The engine work was not wasted. When multiprocess access and `VACUUM` land, the
default moves with a one-word change, and the suite already proves the port.

## What would take us back

- A release where `VACUUM` still is not there, and somebody's archive has
  plateaued somewhere they cannot afford.
- A correctness bug in the engine that costs somebody their history. The cost of
  finding out is one config line, which is the point of keeping both.
- Sync arriving in a form that cannot be scoped, making the second archive
  permanent rather than a workaround.
