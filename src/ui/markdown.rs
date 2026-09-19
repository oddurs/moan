//! Just enough markdown to read a message the way it was written.
//!
//! Assistant prose is markdown, and showing it raw puts asterisks and
//! backticks in front of the words. This turns the common inline marks into
//! styling and the block marks into shape. It is deliberately not a markdown
//! implementation: anything it does not recognise is left exactly as typed,
//! which is the right answer for a transcript.

/// A run of text with one meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Bold(String),
    Italic(String),
    Code(String),
    /// `[text](url)` — what to show, and where it goes.
    Link {
        text: String,
        url: String,
    },
    /// A url written out in full.
    Url(String),
}

impl Piece {
    pub fn text(&self) -> &str {
        match self {
            Piece::Text(s) | Piece::Bold(s) | Piece::Italic(s) | Piece::Code(s) | Piece::Url(s) => {
                s
            }
            Piece::Link { text, .. } => text,
        }
    }

    /// Where this points, if anywhere.
    pub fn url(&self) -> Option<&str> {
        match self {
            Piece::Link { url, .. } => Some(url),
            Piece::Url(u) => Some(u),
            _ => None,
        }
    }
}

/// The shape of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading(usize, String),
    /// A list item, with how deep it is nested.
    Bullet(usize, String),
    Quote(String),
    /// A fenced block, kept exactly as written.
    Code(Vec<String>),
    Rule,
    Para(String),
    Blank,
}

/// Split text into blocks, leaving fenced code untouched.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let t = line.trim_end();
        let lead = t.len() - t.trim_start().len();
        let body = t.trim_start();

        if body.starts_with("```") || body.starts_with("~~~") {
            let fence = &body[..3];
            let mut code = Vec::new();
            for l in lines.by_ref() {
                if l.trim_start().starts_with(fence) {
                    break;
                }
                code.push(l.to_string());
            }
            out.push(Block::Code(code));
            continue;
        }
        if body.is_empty() {
            out.push(Block::Blank);
        } else if body.starts_with('#') {
            let depth = body.chars().take_while(|&c| c == '#').count();
            out.push(Block::Heading(depth, body[depth..].trim().to_string()));
        } else if body.len() >= 3 && body.chars().all(|c| c == '-' || c == '*' || c == '_') {
            out.push(Block::Rule);
        } else if let Some(rest) = bullet(body) {
            out.push(Block::Bullet(lead / 2, rest.to_string()));
        } else if let Some(rest) = body.strip_prefix("> ") {
            out.push(Block::Quote(rest.to_string()));
        } else {
            out.push(Block::Para(t.to_string()));
        }
    }
    out
}

fn bullet(s: &str) -> Option<&str> {
    for m in ["- ", "* ", "+ "] {
        if let Some(r) = s.strip_prefix(m) {
            return Some(r);
        }
    }
    // `1. ` and friends.
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && s[digits..].starts_with(". ") {
        return Some(&s[digits + 2..]);
    }
    None
}

/// Split a line into styled runs.
pub fn inline(s: &str) -> Vec<Piece> {
    let b = s.as_bytes();
    let mut out: Vec<Piece> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;

    let flush = |plain: &mut String, out: &mut Vec<Piece>| {
        if !plain.is_empty() {
            out.push(Piece::Text(std::mem::take(plain)));
        }
    };

    while i < b.len() {
        // A url written out, so the terminal can make it clickable.
        if b[i..].starts_with(b"http://") || b[i..].starts_with(b"https://") {
            let end = b[i..]
                .iter()
                .position(|c| c.is_ascii_whitespace() || *c == b')' || *c == b'>')
                .map_or(b.len(), |p| i + p);
            // Trailing punctuation belongs to the sentence, not the url.
            let mut url = &s[i..end];
            while url.ends_with(['.', ',', ';', ':', '!', '?']) {
                url = &url[..url.len() - 1];
            }
            if url.len() > 8 {
                flush(&mut plain, &mut out);
                out.push(Piece::Url(url.to_string()));
                i += url.len();
                continue;
            }
        }
        // `[text](url)`
        if b[i] == b'[' {
            if let Some((text, url, len)) = link_at(s, i) {
                flush(&mut plain, &mut out);
                out.push(Piece::Link { text, url });
                i += len;
                continue;
            }
        }
        if let Some((piece, len)) = fenced(s, i) {
            flush(&mut plain, &mut out);
            out.push(piece);
            i += len;
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        plain.push(ch);
        i += ch.len_utf8();
    }
    flush(&mut plain, &mut out);
    out
}

/// `**bold**`, `*italic*`, `_italic_`, `` `code` `` at this offset.
fn fenced(s: &str, i: usize) -> Option<(Piece, usize)> {
    let b = s.as_bytes();
    let (mark, len, make): (&str, usize, fn(String) -> Piece) = if b[i..].starts_with(b"**") {
        ("**", 2, Piece::Bold)
    } else if b[i..].starts_with(b"__") {
        ("__", 2, Piece::Bold)
    } else if b[i] == b'`' {
        ("`", 1, Piece::Code)
    } else if b[i] == b'*' {
        ("*", 1, Piece::Italic)
    } else if b[i] == b'_' {
        ("_", 1, Piece::Italic)
    } else {
        return None;
    };
    // `_` only opens a word, so snake_case identifiers survive intact.
    if mark == "_" && i > 0 && !b[i - 1].is_ascii_whitespace() {
        return None;
    }
    let rest = &s[i + len..];
    let end = rest.find(mark)?;
    if end == 0 {
        return None;
    }
    let inner = &rest[..end];
    // An opener followed by a space is arithmetic or a footnote, not emphasis.
    if mark != "`" && inner.starts_with(char::is_whitespace) {
        return None;
    }
    Some((make(inner.to_string()), len + end + len))
}

fn link_at(s: &str, i: usize) -> Option<(String, String, usize)> {
    let rest = &s[i + 1..];
    let close = rest.find(']')?;
    if !rest[close + 1..].starts_with('(') {
        return None;
    }
    let after = &rest[close + 2..];
    let end = after.find(')')?;
    let text = rest[..close].to_string();
    let url = after[..end].to_string();
    if url.is_empty() {
        return None;
    }
    Some((text, url, 1 + close + 2 + end + 1))
}

/// Every place this text points, in the order they appear.
pub fn links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for b in blocks(text) {
        let line = match b {
            Block::Para(s) | Block::Heading(_, s) | Block::Bullet(_, s) | Block::Quote(s) => s,
            _ => continue,
        };
        for p in inline(&line) {
            if let Some(u) = p.url() {
                if !out.iter().any(|x| x == u) {
                    out.push(u.to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasis_becomes_style_not_punctuation() {
        assert_eq!(
            inline("**Merged** — see `parse_jsonl` now"),
            vec![
                Piece::Bold("Merged".into()),
                Piece::Text(" — see ".into()),
                Piece::Code("parse_jsonl".into()),
                Piece::Text(" now".into()),
            ]
        );
    }

    #[test]
    fn an_identifier_is_not_emphasis() {
        // snake_case would otherwise turn into italics from the first
        // underscore to the next one, several words later.
        assert_eq!(
            inline("call head_meta then repo_root"),
            vec![Piece::Text("call head_meta then repo_root".into())]
        );
        // Nor is arithmetic.
        assert_eq!(inline("2 * 3 * 4"), vec![Piece::Text("2 * 3 * 4".into())]);
    }

    #[test]
    fn links_keep_both_halves() {
        let p = inline("see [the PR](https://github.com/o/p/pull/7) for why");
        assert_eq!(
            p[1],
            Piece::Link {
                text: "the PR".into(),
                url: "https://github.com/o/p/pull/7".into()
            }
        );
    }

    #[test]
    fn a_bare_url_is_recognised_and_keeps_its_sentence_out_of_it() {
        let p = inline("shipped at https://example.com/a/b, then stopped");
        assert_eq!(p[1], Piece::Url("https://example.com/a/b".into()));
        assert_eq!(p[2], Piece::Text(", then stopped".into()));
    }

    #[test]
    fn fenced_code_is_left_exactly_as_written() {
        let b = blocks("before\n```sh\n**not bold** _nor this_\n```\nafter");
        assert_eq!(
            b[1],
            Block::Code(vec!["**not bold** _nor this_".to_string()])
        );
    }

    #[test]
    fn blocks_take_their_shape() {
        let b = blocks("## Heading\n\n- one\n- two\n\n> quoted\n\n---");
        assert_eq!(b[0], Block::Heading(2, "Heading".into()));
        assert_eq!(b[2], Block::Bullet(0, "one".into()));
        assert_eq!(b[5], Block::Quote("quoted".into()));
        assert_eq!(b[7], Block::Rule);
    }

    #[test]
    fn numbered_items_are_items_too() {
        assert_eq!(blocks("1. first")[0], Block::Bullet(0, "first".into()));
    }

    #[test]
    fn every_destination_is_collected_once() {
        let text = "see [a](https://x.dev/1) and https://x.dev/2\n\n- again [b](https://x.dev/1)";
        assert_eq!(
            links(text),
            vec!["https://x.dev/1".to_string(), "https://x.dev/2".to_string()]
        );
    }

    #[test]
    fn unrecognised_markup_survives_untouched() {
        for s in ["a ** b", "half [a](", "* ", "`unclosed", "100% * 2"] {
            let joined: String = inline(s).iter().map(|p| p.text().to_string()).collect();
            assert!(
                joined.contains(s.trim_end()) || joined == s,
                "{s:?} -> {joined:?}"
            );
        }
    }
}
