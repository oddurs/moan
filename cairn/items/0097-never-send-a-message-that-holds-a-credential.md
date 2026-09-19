---
id: 97
title: Never send a message that holds a credential
type: feature
status: done
milestone: v0.3
created: 2026-09-16
updated: 2026-09-16
priority: p0
effort: m
area: condense
---

## Problem

Tool calls are summarised by rule and short messages stand as they are, but prose long enough to want summarising is sent to a model — and a transcript collects keys nobody meant to write down. There was nothing between a key echoed by a shell into a reply and an HTTP request.

## Proposal

Look before sending. A message holding something key-shaped is summarised from its own opening instead, marked, and counted.

Refuse rather than redact: scrubbing a secret out of arbitrary text is not a problem anybody has solved, and a redaction that misses looks safe.

## Acceptance criteria

- [x] A message holding a credential is never sent, and is marked `⊘` on its row
- [x] The header counts what was kept back and the status says what was found
- [x] Known shapes: Anthropic, OpenAI, OpenRouter, GitHub, GitLab, Slack, AWS, Google, DigitalOcean, Shopify, and PEM private keys
- [x] A name like `token` or `password` followed by something long and random
- [x] Writing *about* keys is not a key: `$ANTHROPIC_API_KEY`, `your-key-here`, a grep for the shape — all pass
- [x] The decision survives: a summary arriving from another moan sharing the archive cannot overwrite it
- [x] Verified end to end on a transcript with a real key in it: kept back, while the harmless message beside it was summarised
