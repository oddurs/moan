//! The normalized event model every source flattens into.
//!
//! A transcript is a tree of nested JSON; the feed is a flat list of lines.
//! Everything downstream — storage, condensation, rendering — speaks `Event`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
    Tool,
    System,
}

impl Role {
    pub fn glyph(self) -> &'static str {
        match self {
            Role::User => "▌",
            Role::Assistant => "▌",
            Role::Tool => "▏",
            Role::System => "▏",
        }
    }
}

/// What the event *is*, which decides how it condenses and whether it is cruft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    /// Something the human typed.
    Prompt,
    /// Assistant prose addressed to the human.
    Say,
    /// Assistant reasoning. Hidden by default.
    Think,
    /// A tool invocation. Condensed by rule, never by model.
    Tool { name: String },
    /// A tool failure worth surfacing on its own line.
    Fail,
    /// Session boundaries, compaction notices, model switches.
    Note,
}

impl Kind {
    /// Prose kinds are the only ones that pay for a model call.
    pub fn needs_model(&self) -> bool {
        matches!(self, Kind::Prompt | Kind::Say | Kind::Think)
    }

    pub fn label(&self) -> &str {
        match self {
            Kind::Prompt => "you",
            Kind::Say => "claude",
            Kind::Think => "think",
            Kind::Tool { name } => name,
            Kind::Fail => "fail",
            Kind::Note => "·",
        }
    }
}

/// Where a gist came from, so the UI can show what is still settling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GistSource {
    /// Short enough to stand as-is.
    Verbatim,
    /// Produced by a deterministic rule.
    Rule,
    /// Produced by the configured model.
    Model,
    /// Wants the model but has not been sent: out past the reading window, or
    /// still arriving. Shows an opening, and no promise that more is coming.
    Pending,
    /// Sent, and being condensed right now.
    Working,
    /// The model was asked and did not answer. Retryable, and worth saying so
    /// on the row rather than only in a status line that scrolls away.
    Failed,
    /// Holds something that looks like a credential, so it was never sent.
    /// Summarised here instead, from its own opening.
    Withheld,
}

#[derive(Debug, Clone)]
pub struct Event {
    /// Stable id from the source transcript.
    pub uid: String,
    pub session: String,
    pub ts: DateTime<Utc>,
    pub role: Role,
    pub kind: Kind,
    /// Full original text, kept for expansion.
    pub raw: String,
    /// The one-line condensation shown in the feed.
    pub gist: String,
    pub gist_src: GistSource,
    /// Result text attached to a tool call, folded in from its tool_result.
    pub outcome: Option<String>,
    pub is_error: bool,
    /// Output tokens attributed to this event, for the session tally.
    pub tokens: u32,
}

/// What a line under a headline is telling you. Ordered by how much it wants
/// to be seen: an alert that scrolls past below three details is wasted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Note {
    /// Something went wrong, was skipped, or the reader must not miss it.
    Alert,
    /// A detail worth keeping.
    Info,
    /// A file, path, url, or command the reader may want to go to.
    Link,
}

impl Note {
    pub fn mark(self) -> char {
        match self {
            Note::Alert => '!',
            Note::Info => '·',
            Note::Link => '→',
        }
    }

    pub fn of(c: char) -> Option<Self> {
        match c {
            '!' | '⚠' => Some(Note::Alert),
            '·' | '-' | '*' | '•' => Some(Note::Info),
            '→' | '>' => Some(Note::Link),
            _ => None,
        }
    }
}

/// Split a gist line into its mark and its text. An unmarked line is info.
pub fn note_of(line: &str) -> (Note, &str) {
    let t = line.trim();
    let mut c = t.chars();
    match c.next().and_then(Note::of) {
        Some(k) => (k, c.as_str().trim()),
        None => (Note::Info, t),
    }
}

/// Bump when the condensation prompt changes in a way that should show.
/// Cached gists are keyed on it, so old ones stop being reused instead of
/// quietly outliving the prompt that produced them.
const GIST_VERSION: u8 = 2;

/// Above this much original text, a one-line gist loses too much and the
/// condensation earns a headline plus a few bullets instead.
pub const COMPLEX: usize = 700;

impl Event {
    /// Whether this is substantial enough to want bullets under its headline.
    pub fn is_complex(&self) -> bool {
        self.kind == Kind::Say && self.raw.chars().count() > COMPLEX
    }

    /// Content hash — the cache key for a condensation.
    ///
    /// Keyed on kind and text, not on uid: the same paragraph in two sessions
    /// condenses once.
    pub fn content_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.kind.label().as_bytes());
        h.update([GIST_VERSION, self.is_complex() as u8]);
        h.update(self.raw.as_bytes());
        format!("{:x}", h.finalize())[..16].to_string()
    }
}

/// Strip the markdown out of prose so a placeholder reads as a sentence.
///
/// What is shown before a condensation lands is the message's own opening, and
/// an opening full of backticks and asterisks looks like source rather than
/// something somebody said.
pub fn preview(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // `code` and **emphasis** carry nothing at a glance.
            '`' | '*' | '_' | '#' | '>' => {}
            // [text](url) keeps the text and drops the destination.
            '[' => {}
            ']' => {
                if chars.peek() == Some(&'(') {
                    for d in chars.by_ref() {
                        if d == ')' {
                            break;
                        }
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Collapse any text to a single spaceless-run line, for one-line display.
pub fn flatten(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut sp = false;
    for c in s.chars() {
        if c.is_whitespace() {
            sp = true;
        } else {
            if sp && !out.is_empty() {
                out.push(' ');
            }
            sp = false;
            out.push(c);
        }
    }
    out
}

/// Truncate to `n` characters on a word boundary where one is close by.
pub fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let head: String = s.chars().take(n.saturating_sub(1)).collect();
    match head.rfind(' ') {
        Some(i) if i > n * 2 / 3 => format!("{}…", &head[..i]),
        _ => format!("{head}…"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gist cache is keyed on this string. If it changes, every summary
    /// ever bought stops being found and is bought again — silently, and at
    /// the reader's expense.
    ///
    /// So the value is written down rather than computed by the test. A hash
    /// library upgrade, a reordered `update`, or a renamed `Kind` label all
    /// change it, and all of them should be a decision somebody made on
    /// purpose. `GIST_VERSION` is the deliberate way to invalidate the cache;
    /// this test is here to make sure nothing else does it by accident.
    #[test]
    fn the_cache_key_does_not_move_on_its_own() {
        let e = Event {
            uid: "u".into(),
            session: "s".into(),
            ts: chrono::Utc::now(),
            role: Role::Assistant,
            kind: Kind::Say,
            raw: "the quick brown fox jumps over the lazy dog".into(),
            gist: String::new(),
            gist_src: GistSource::Pending,
            outcome: None,
            is_error: false,
            tokens: 0,
        };
        assert_eq!(
            e.content_hash(),
            "b95b31801103360e",
            "the cache key moved: every summary already bought would be bought \
             again. If this was deliberate, bump GIST_VERSION and update this."
        );
        // The timestamp, the session and the uid are deliberately not in it:
        // the same paragraph in two conversations is condensed once.
        let elsewhere = Event {
            uid: "other".into(),
            session: "other".into(),
            ts: chrono::Utc::now(),
            ..e.clone()
        };
        assert_eq!(e.content_hash(), elsewhere.content_hash());
    }

    #[test]
    fn a_preview_reads_as_prose_not_as_source() {
        let raw =
            "**Merged** — `aba9cb86` on main, and the [Pages deploy](https://x.dev/p) is running.";
        assert_eq!(
            preview(raw),
            "Merged — aba9cb86 on main, and the Pages deploy is running."
        );
    }

    #[test]
    fn a_preview_leaves_ordinary_prose_alone() {
        let raw = "the parser now stops at the last newline, so a torn line is re-read";
        assert_eq!(preview(raw), raw);
    }

    #[test]
    fn a_preview_drops_heading_marks() {
        assert_eq!(preview("## What changed"), " What changed");
    }

    #[test]
    fn a_preview_keeps_an_unpaired_bracket_from_eating_the_line() {
        // A stray `]` with no `(` after it must not swallow the rest.
        assert_eq!(
            preview("array[0] holds the offset"),
            "array0 holds the offset"
        );
    }
}
