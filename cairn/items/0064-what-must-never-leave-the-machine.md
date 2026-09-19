---
id: 64
title: What must never leave the machine?
type: spike
status: done
milestone: v0.5
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p0
effort: s
area: store
---

## Question

A transcript is not innocuous. It holds file paths, source code, and every shell command run — and commands carry secrets. This session's own transcript contains a `grep` for an API key pattern, and the key's prefix appears in the output. What is safe to sync, and what must be stripped or kept local?

## Timebox

Half a day, before any sync code is written.

## What we will do with the answer

It decides what the sync item is allowed to send. Getting this wrong uploads somebody's credentials, and it cannot be undone by a later fix.

## To settle

- Does the raw text sync, or only the condensations? A gist alone makes the feed readable on another machine and carries far less.
- If raw text syncs, what is redacted, and is redaction trustworthy enough to rely on? Assume it is not.
- Is the archive encrypted before it leaves, and who holds the key?
- What does the user see about what is being sent, before it is sent?

## Answer

**Condensations and session identity sync. Raw text does not, unless the user
says so in as many words.**

### What is actually in there

Looked at the real archive rather than reasoning about it. `events.raw` holds
every shell command the agent ran, in full. In this project's own transcript
that includes a `grep` for an API key pattern **and the matched prefix of the
key in the output**. It also holds file contents written via heredocs, absolute
paths, branch names, and the text of every message.

A condensation is a dozen words describing what happened. It can still be
sensitive — "found the key in ~/.config/namesync/env" is a gist — but it cannot
contain a credential the model was never asked to repeat, and it is orders of
magnitude less surface.

### The default

| what | syncs | why |
|---|---|---|
| `gists` | yes | the feed is readable on another machine, and this is the part that cost money |
| `sessions`, `scan` | yes | without them a gist has nothing to attach to |
| `events.raw` | **no** | every command you have run, verbatim |
| `events` metadata and gist | yes | time, author, kind, the condensed line |

On the second machine the conversation reads normally and pressing enter on a
message says the original is not here. That is a smaller loss than it sounds:
the transcript is still on the machine that produced it.

`sync_raw = true` turns the rest on for somebody who wants a complete mirror and
has decided their archive is not sensitive. It is a sentence in a config file,
not a flag nobody reads.

### Encryption

Not attempted here, and not claimed. Turso encrypts in transit; at rest the
database is readable by whoever holds the account. That is the reason raw text
is off by default rather than a reason to believe redaction would be safe —
scrubbing secrets out of arbitrary shell commands is not a problem anybody has
solved, and pretending otherwise would be worse than not syncing them.

### What the user sees

`moan config` prints what syncs and what stays, every time, without being
asked. Nothing is uploaded until `remote_url` is set, and setting it is a
deliberate act.

State the default explicitly, and make the default the conservative one.
