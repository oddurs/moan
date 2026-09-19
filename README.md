# moan

A compressed, live transcript of your coding-agent sessions.

Claude Code writes everything it does to a JSONL file. Most of it is machinery:
token reminders, file snapshots, mode flips, hundred-line heredocs, tool results
nobody reads. `moan` tails that file and gives you back the conversation — you,
Claude, you, Claude — like a Slack channel you can't type into.

```
 moan   ~/Code/moan  fix/0041-attach                  ● just now  ·  haiku-4.5  ·  26
──────────────────────────────────────────────────────────────────────────────────────
09:41 ▌ you     make the parser handle a half-written tail
09:43 ▌ claude  offsets now stop at the last newline
                ! a torn line was silently dropped before this
                · sidechain traffic no longer doubles the feed
                → src/source/claude.rs
09:47 ▌ you     the bash stuff should be hidden by default
09:51 ▌ claude  tool calls hidden; the feed reads as a conversation
 ↑↓ move   ⏎ expand   / search   o tools   t think   s sessions   ? help   q quit
```

A message carries up to three notes under its headline, and they are sorted so
the one that matters is never below the ones that don't:

| | |
|---|---|
| `!` | something went wrong, was skipped, or you should not miss it |
| `·` | a detail worth keeping |
| `→` | a file, path, or url you may want to go to |

One turn of an agent's work is a dozen text blocks with tool calls between them.
moan fuses each turn into a single message: a headline, and up to three bullets
when there was more to it than one line can hold. Your own messages are never
paraphrased — they appear as you typed them.

Press <kbd>⏎</kbd> on any message to read the original underneath it, and
<kbd>o</kbd> to bring the tool calls back.

Messages are written in markdown, so they are shown that way: headings, lists,
quotes and emphasis become shape rather than punctuation, and fenced code is
left exactly as it was typed. Anything unrecognised stays as written — which is
the right answer for a transcript.

Links are printed in full rather than hidden behind their text, because a url
you cannot see is one you cannot follow or copy, and most terminals will make a
literal one clickable. <kbd>O</kbd> opens the first link in a message for the
terminals that will not — and only ever an `http` or `https` address, never a
path or a command from the transcript.

## Install

```sh
cargo install --path .
```

Or take a build, which needs no Rust toolchain:

```sh
curl -L https://github.com/oddurs/moan/releases/latest/download/moan-$(uname -s)-$(uname -m).tar.gz | tar xz
```

Every release carries binaries for macOS and Linux, on Intel and ARM, each with
a `.sha256` beside it.

## Use

```sh
moan                    # newest session for the current directory
moan --any              # newest session anywhere
moan -s 9b7a583         # a specific session id (prefix is enough)
moan conversations      # list what it can see
moan export 9b7a583     # the conversation as markdown
moan export 9b7a583 --tools --thinking   # with everything shown
moan doctor             # check the model answers
moan config             # paths, model, key, what is stored
moan prune --keep 50    # drop the oldest sessions and reclaim the space
```

## Keys

| | |
|---|---|
| <kbd>↑</kbd> <kbd>↓</kbd> <kbd>j</kbd> <kbd>k</kbd> | move |
| <kbd>⏎</kbd> <kbd>space</kbd> | expand / collapse the original text |
| <kbd>g</kbd> <kbd>G</kbd> | top / bottom |
| <kbd>f</kbd> | follow the live tail |
| <kbd>o</kbd> | tool calls on and off (off by default) |
| <kbd>t</kbd> | assistant reasoning on and off |
| <kbd>f</kbd> | follow the live tail |
| <kbd>/</kbd> | search this conversation |
| <kbd>O</kbd> | open the first link in a message |
| <kbd>r</kbd> <kbd>R</kbd> | re-condense this line / drop every cached condensation |
| <kbd>s</kbd> | every conversation, and search |
| <kbd>tab</kbd> | move between the list and the conversation |
| <kbd>1</kbd>–<kbd>9</kbd> | jump to one on this project |
| <kbd>a</kbd> | this project's conversations, interleaved |
| <kbd>?</kbd> <kbd>q</kbd> | help / quit |

Clicking a line selects it; clicking it again expands it.

## The interface

```toml
[ui]
show_tools    = false   # tool calls in the feed; o toggles it
show_thinking = false   # assistant reasoning; t toggles it
poll_ms       = 400     # how often to look for new messages
clock_24h     = true    # the clock in the gutter
theme         = "auto"  # auto | dark | light
```

`auto` reads `COLORFGBG` and assumes dark when the terminal does not say.
Every colour comes from your terminal's own sixteen, so moan sits in whatever
scheme you already use rather than beside it.

## Knowing what just happened

A message that has just arrived is marked at the left edge, and the mark fades
as it stops being news — bright for twenty seconds, then two dimmer stages out
to three minutes, then nothing. Glancing back at a feed you left running, the
new part is the part that is lit.

While Claude has your message and has not answered yet, its reply is shown
being composed:

```
▎ 01:44 ▌ you     now make it feel natural in the message ui
▎ 01:44 ▌ claude  · ● ·
```

That holds for as long as it is working, including while it is running tools it
has not told you about yet. It stops on its own if a session was abandoned —
nothing is being written, so nothing is working.

## More than one agent

Agents run in worktrees, and a worktree's `.git` points back at the checkout it
was cut from — so moan groups every session of a project together, however many
directories they are spread across.

`tab` and `1`–`9` move between a project's conversations without opening
anything, and `a` interleaves them into one feed in time order. There, a caption
marks each change of source — only where it changes, so a long run from one
conversation reads uninterrupted:

```
              ── main·536f ──────────────────────────────────────────
23:31 ▌ you     we'll build a desktop after 1.0
23:40 ▌ claude  noted, the desktop shell lands after 1.0
              ── feat/platform-activation ───────────────────────────
23:43 ▌ you     create an astro site for the docs
```

The header says when a project has other conversations and whether any of them
are working, which is all the glance needs. Everything else lives in the list.

## The list

<kbd>s</kbd> opens a list beside the conversation holding every conversation on
the machine, grouped by project. It is not modal: you can watch one project
while reading another. It is the only way in — there is no second switcher and
no modal picker, because two surfaces for one job drift apart.

```
  sessions   search           │  01:38 ▌ you     write release post
                              │  01:41 ▌ claude  release 0.2.0 is ready
 ~/Code/unifont  5            │                  ! 42 features, 38 fixes
▎● main·cc6b                  │                  · browser work is 2 of 4
   feat/platform-activation · │  02:38 ▌ you     make cli output better
   main·0c65              · 3 │  02:51 ▌ claude  browser and cli differ
                              │
 ● ~/Code/yoghurt         ! 2 │
 ◆ ~/Code/trafford            │
```

`●` is working · `◆` has stopped and is waiting for you · `· 3` arrived since
you last read it · `! 2` of those want attention. A project with one session is
named by the project, because `~/Code/yoghurt` says more than `main` does.

<kbd>tab</kbd> moves the keyboard between the two panes; the divider lights on
whichever side has it.

With a hundred conversations, arrow keys are not navigation, so <kbd>/</kbd>
narrows the list as you type — by project, by branch, or by agent — and says how
much of what is left:

```
 ▸ uni▏
   6 of 101

 ~/Code/unifont  5
▎● main·cc6b           2m  ·
   feat/platform-act…  8d  ·
```

<kbd>esc</kbd> puts it back. Each row carries how long ago it moved and what has
arrived since you read it; the agent is named only where it is the odd one out,
because a letter on every row when most of them say the same thing is noise.

In the **search** list <kbd>/</kbd> queries every conversation in the archive
rather than narrowing this list, and <kbd>⏎</kbd> on a result jumps the feed to
that message while leaving the results up — so working through them is one
keypress each.

Below about ninety columns the panel covers the feed instead of sitting beside
it: thirty columns out of eighty leaves nothing worth reading.

## The model

Any model, anywhere. Edit `~/Library/Application Support/moan/config.toml`
(`~/.config/moan/config.toml` on Linux) — `moan config` prints the path.

```toml
[model]
provider = "openrouter"
model = "google/gemini-2.5-flash-lite"
```

The default is picked for being cheap and quick rather than clever — condensing
one line of a transcript is not hard work. Measured over six real messages
through OpenRouter:

| model | median | per 1,000 messages |
|---|---|---|
| `google/gemini-2.5-flash-lite` | 0.80s | $0.13 |
| `amazon/nova-lite-v1` | 1.25s | $0.09 |
| `inclusionai/ling-3.0-flash` | 1.40s | $0.03 |
| `anthropic/claude-haiku-4.5` | 2.05s | $1.56 |

Anything that follows the shape will do. Models that answer with markdown
headings or ignore the note marks look wrong in the feed, which is the thing to
check if you swap it.

Presets: `anthropic`, `openai`, `openrouter`, `groq`, `together`, `deepseek`,
`gateway`, `ollama`, `lmstudio`, `llamacpp`. Local servers need no key. For
anything else, say what wire format it speaks:

```toml
[model]
provider = "custom"
api      = "openai"          # or "anthropic"
base_url = "http://box.lan:8000/v1"
model    = "qwen2.5-14b-instruct"
```

Keys come from the environment (`api_key_env`), not the file — or from a file
you already keep them in:

```toml
api_key_file = "~/.config/something/env"   # export NAME=value, or just the key
```

The rest of the model settings, with their defaults:

| | |
|---|---|
| `max_tokens = 96` | a summary longer than this is a failed summary |
| `concurrency = 8` | how many to have in flight; each is a short request |
| `timeout_secs = 30` | before one is abandoned |
| `style = ""` | extra guidance appended to the built-in prompt |

Give it a house style if you want one:

```toml
style = "Prefer the file or symbol name over the verb. Never start with 'the'."
```

## What costs a model call

Almost nothing does, which is the point. Only what is on screen is ever paid
for: when a turn's prose is merged into one message, the blocks it was made of
are never condensed.

- **Tool calls** condense by rule. `Bash` shows the command with its heredoc
  payload replaced by a line count; `Edit` shows the file and `+n/−m`; `Read`
  shows the file; `TodoWrite` shows progress and the active item. A tool call is
  already structured — a model would add latency without adding meaning.
- **Your own messages** are shown as written. A summary of your own sentence in
  somebody else's voice is worse than the sentence.
- **Short prose** is already its own gist and passes through untouched.
- **Only the session you are looking at**, and within it only one screen
  either side of where you are reading. A merged project can hold thousands of
  messages; scrolling pulls more in, and opening one does not spend money on
  lines nobody will look at.
- **Only once per message.** Prose is grouped into turns over the whole
  transcript rather than over what is on screen, so hiding or showing the tool
  calls does not change what a message contains — and therefore does not buy it
  again.
- **A turn's prose** is merged and sent once. Under about 700 characters it
  comes back as one line; above that, as a headline and up to three notes.
  Only the first and last few thousand characters are sent.
- **Tool results** never get a line of their own. They fold into the call as a
  right-hand note (`28 passed`, `4 lines`, `Exit code 1`), and only surface
  alone when the call that produced them is already off the end of the buffer.

Dropped entirely: token reminders, file-history snapshots, mode and
permission-mode records, generated titles, injected meta-prompts, and subagent
sidechains — the parent's `Task` line already stands for those.

A condensation is keyed by a hash of its content, so the same paragraph is paid
for once across every session that contains it. The key carries a version, so
improving the prompt retires the old gists instead of letting them outlive it. With no key configured the feed
still works; long prose shows as a dimmed truncation instead.

## Keys stay here

A transcript collects things nobody meant to write down: a key echoed by a
shell, a token pasted into a prompt, a private key read by mistake. Most of a
conversation never leaves the machine — tool calls are summarised by rule, and
short messages stand as they are — but prose long enough to want summarising is
sent to a model, and that is the one way out.

So moan looks before it sends. A message that holds something key-shaped is
summarised from its own opening instead, marked `⊘`, and counted in the header:

```
 moan   /tmp/demo                    ● just now  ·  flash-lite  ·  ⊘ 1 kept back  ·  4
 00:55 ▌ claude  ⊘ I should not have echoed this. The key is sk-or-v1-82d7aa11bb…
 00:56 ▌ claude  the sentence was long enough to summarise
   note   a message holds an OpenRouter key — summarised here, not sent
```

It **refuses rather than redacts**. Scrubbing a secret out of arbitrary text is
not a problem anybody has solved, and a redaction that misses is worse than not
sending: it looks safe.

It knows the distinctive shapes — Anthropic, OpenAI, OpenRouter, GitHub, GitLab,
Slack, AWS, Google, and `BEGIN … PRIVATE KEY` — and a name like `token` or
`password` followed by something long and random enough to be one. Writing
*about* keys is not writing one: `$ANTHROPIC_API_KEY`, `your-key-here`, and a
grep for `sk-or-v1-[A-Za-z0-9]*` all pass through untouched, and there are tests
for each.

## Storage

The archive lives in one file next to the config, holding every parsed event and
every condensation. Claude Code rotates and deletes transcripts; this outlives
them. Reopening a session is a read from the archive plus a parse of whatever
the transcript has grown by — **eleven milliseconds to the first frame** on a
machine with four hundred and forty-five sessions, because what each transcript
says about itself is cached against its timestamp rather than read again.

```toml
[store]
engine = "turso"   # or "sqlite"
```

Turso is SQLite-compatible and pure Rust, so moan needs no C compiler, and it
can keep the same archive on more than one machine. It reads the file SQLite
wrote, in place — changing engine is one line, not a migration. It also cannot
yet return freed pages to the filesystem, so the archive plateaus rather than
shrinking; `moan config` reports space used and space on disk separately, and
retention counts pages in use so pruning stops when it should. The reasoning is
in [docs/0001-turso.md](docs/0001-turso.md).

Several moans can share one archive. They see each other's writes, and a claim
table stops two of them paying to condense the same message.

### How much it keeps

```toml
[retain]
max_mb   = 256   # ceiling for the archive; 0 keeps everything
days     = 0     # drop sessions older than this; 0 ignores age
sessions = 0     # keep at most this many; 0 for no limit
```

Size is the control that matters, because sessions differ wildly. On one real
machine 445 sessions came to 1,052 MB of transcript — but the newest *twenty*
were already 321 MB of it, so a session count says almost nothing about disk.

The policy runs at startup and only does work when the archive is over, so the
usual case costs a single page read. `moan prune` runs it on demand.

Two things are never thrown away. **Condensations** stay whatever else goes:
they are what cost money, and at tens of kilobytes against hundreds of megabytes
of text they are not what is taking up room. And a session whose transcript is
**still on disk** is given up before an older one whose transcript has been
rotated away — the first can be read again for nothing, the second is the only
copy left.

### Keeping it on two machines

```toml
[sync]
remote_url = ""                        # empty: nothing ever leaves
auth_token_env = "TURSO_AUTH_TOKEN"
raw = false                            # send the original text too
```

`moan sync` builds a second, smaller archive — the one that is allowed to leave
— and prints exactly what it holds before anything could be sent. Turso
replicates a whole database and cannot hold a column back, which is why the
shareable copy is a separate file rather than a filtered view.

The original text stays behind by default. `events.raw` holds every shell
command the agent ran: this project's own transcript contains a `grep` for an
API key pattern **and the matched prefix of the key in its output**. Turso can
encrypt what it stores, but it sends the key to the server in a header, so that
protects the archive from third parties rather than from the service. Turning it
on is a sentence in a config file, not a flag nobody reads.

The other half works without a remote at all. Copy the shareable archive to the
other machine however you like, and:

```sh
moan sync --merge /path/to/moan-shared.db
```

Whatever arrives is merged rather than replacing anything: conversations and
messages are keyed on their own ids, summaries on a hash of what they summarise,
so two machines that condensed the same message agree by construction. Nothing
local is overwritten — in particular, a message whose original text was left
behind never replaces the copy that still has it.

Pushing to a Turso remote is not implemented. `moan sync` builds the copy and
says so.

## Other agents

Codex and opencode are next. A source implements two methods — list sessions,
parse from a byte offset — and everything downstream is shared.

## Working on it

```sh
./scripts/task check     # fmt, lint, test, build — what CI runs
./scripts/task bench     # numbers to read rather than thresholds to pass
```

CI and the git hooks call the same verbs, so they cannot drift from what you
run by hand. `git config core.hooksPath .githooks` arms them. The lint policy
is in `Cargo.toml` rather than in flags somebody has to remember, so
`cargo clippy` on its own is the whole check: `unsafe_code` is forbidden, and
the cast classes that were clean are denied so they stay clean.

The backlog lives in `cairn/items/`, rendered to [ROADMAP.md](ROADMAP.md).
[CONTRIBUTING.md](CONTRIBUTING.md) has the house rules;
[SECURITY.md](SECURITY.md) covers what leaves the machine and what to do if
something escapes that should not have.

Rust 1.90 or newer. The toolchain is pinned in `rust-toolchain.toml` so a build
is the same everywhere, and CI proves the floor separately from the pin.

## Licence

MIT
