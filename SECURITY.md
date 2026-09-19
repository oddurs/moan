# Security

## What this program touches

moan reads coding-agent transcripts from your machine, keeps a condensed copy
in a local SQLite archive, and sends *some* message text to whichever model you
configure. That is three places where a mistake costs you something real, so
each one has a rule.

**Transcripts never leave except to the model you chose.** There is no
telemetry, no crash reporting, and no default remote. `moan export` builds a
separate archive and tells you exactly what is in it before you move it.

**Credentials are refused, not redacted.** `src/secrets.rs` scans every message
before it can reach a model. A message that looks like it holds a key is
summarised locally and marked withheld in the feed — it is never sent. The rule
is refusal because a redaction that misses looks safe.

**Your API key is referenced, never copied.** `model.api_key_file` points at a
file you already have; moan reads it at call time and does not write it into
its own config or its archive.

## Reporting a vulnerability

Email <oddurs@gmail.com> with "moan security" in the subject, or use GitHub's
[private vulnerability reporting](https://github.com/oddurs/moan/security/advisories/new).
Please do not open a public issue first.

Include what you did, what happened, and what you expected. If it is a secret
that escaped, say which detector should have caught it — a failing case for
`src/secrets.rs` is the most useful thing you can send.

Expect an acknowledgement within a week. This is one person's side project, not
a vendor with an on-call rota; I will tell you honestly when a fix will land.

## Supported versions

The newest release. There is no backport branch.
