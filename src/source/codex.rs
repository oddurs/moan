//! Codex transcripts: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`.
//!
//! A different vocabulary for the same story. One JSON object per line,
//! append-only, so the reading works exactly as it does for Claude Code — only
//! the names of things differ.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::event::{clip, flatten, Event, GistSource, Kind, Role};
use crate::source::{Cursor, Facts, Source, Stub};

pub struct CodexSource {
    root: PathBuf,
}

impl CodexSource {
    pub fn new() -> Result<Self> {
        let home = directories::UserDirs::new()
            .context("no home directory")?
            .home_dir()
            .to_path_buf();
        Ok(Self {
            root: home.join(".codex/sessions"),
        })
    }
}

impl Source for CodexSource {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn list(&self) -> Result<Vec<Stub>> {
        let mut out = Vec::new();
        // Rollouts are filed under year/month/day rather than by project.
        walk(&self.root, 0, &mut |p: &Path| {
            if p.extension().is_none_or(|e| e != "jsonl") {
                return;
            }
            let Ok(md) = p.metadata() else { return };
            if md.len() == 0 {
                return;
            }
            out.push(Stub {
                // `rollout-<when>-<uuid>`. The uuid is what identifies the
                // conversation; the rest is the filing system.
                id: name_of(p),
                modified: md.modified().map(Into::into).unwrap_or_else(|_| Utc::now()),
                bytes: md.len(),
                source: "codex",
                slug: String::new(),
                path: p.to_path_buf(),
            });
        });
        crate::source::newest_first_stubs(&mut out);
        Ok(out)
    }

    fn describe(&self, stub: &Stub) -> Facts {
        let (cwd, branch, last) = head_meta(&stub.path);
        let title = cwd.unwrap_or_else(|| "codex".into());
        let project = crate::source::repo_root(Path::new(&untilde(&title)))
            .map(|p| crate::ui::app::tilde(&p.to_string_lossy()))
            .unwrap_or_else(|| title.clone());
        Facts {
            assistant_last: last,
            title,
            branch,
            project,
        }
    }

    fn parse(&self, path: &Path, _session: &str, from: &Cursor) -> Result<(Vec<Event>, Cursor)> {
        let (events, at) = parse_rollout(path, from.offset())?;
        Ok((events, Cursor::at(at)))
    }
}

/// The conversation's own id, out of the filename it is filed under.
fn name_of(p: &Path) -> String {
    let stem = p.file_stem().unwrap_or_default().to_string_lossy();
    stem.rsplit_once('-')
        .map(|(_, tail)| tail.to_string())
        .filter(|t| t.len() >= 8)
        .unwrap_or_else(|| stem.to_string())
}

/// Every file under a directory, a few levels down.
fn walk(dir: &Path, depth: usize, f: &mut impl FnMut(&Path)) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, depth + 1, f);
        } else {
            f(&p);
        }
    }
}

fn untilde(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => directories::UserDirs::new()
            .map(|d| d.home_dir().join(rest).to_string_lossy().to_string())
            .unwrap_or_else(|| p.to_string()),
        None => p.to_string(),
    }
}

/// `(cwd, branch, who spoke last)`, from the ends of the file.
fn head_meta(path: &Path) -> (Option<String>, Option<String>, Option<bool>) {
    use std::io::BufRead;
    let Ok(f) = std::fs::File::open(path) else {
        return (None, None, None);
    };
    let mut cwd = None;
    let mut branch = None;
    for line in std::io::BufReader::new(f)
        .lines()
        .take(40)
        .map_while(Result::ok)
    {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if v["type"] == "session_meta" {
            cwd = v["payload"]["cwd"].as_str().map(crate::ui::app::tilde);
            branch = v["payload"]["git"]["branch"]
                .as_str()
                .filter(|b| !b.is_empty() && *b != "HEAD")
                .map(str::to_string);
            break;
        }
    }
    (cwd, branch, last_speaker(path))
}

/// Did the assistant have the last word? Read from the end, as for Claude.
fn last_speaker(path: &Path) -> Option<bool> {
    use std::io::{Read, Seek, SeekFrom};
    const TAIL: u64 = 96 * 1024;
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let from = len.saturating_sub(TAIL);
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    f.take(TAIL).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().collect();
    if from > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    for line in lines.iter().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["type"] != "response_item" || v["payload"]["type"] != "message" {
            continue;
        }
        match v["payload"]["role"].as_str() {
            Some("assistant") => return Some(true),
            Some("user") if !injected(&said(&v["payload"]["content"])) => return Some(false),
            _ => {}
        }
    }
    None
}

/// Read whole lines from `from_byte`; return events and where to resume.
pub fn parse_rollout(path: &Path, from_byte: u64) -> Result<(Vec<Event>, u64)> {
    let bytes = std::fs::read(path)?;
    if from_byte as usize >= bytes.len() {
        return Ok((Vec::new(), from_byte));
    }
    let tail = &bytes[from_byte as usize..];
    // Only past the last newline, so a half-written record is read next time
    // rather than parsed as though it were whole.
    let Some(end) = tail.iter().rposition(|&b| b == b'\n').map(|i| i + 1) else {
        return Ok((Vec::new(), from_byte));
    };
    let text = String::from_utf8_lossy(&tail[..end]);

    let mut events = Vec::new();
    let mut pending: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut session = String::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        absorb(&v, &mut events, &mut pending, &mut session);
    }
    Ok((events, from_byte + end as u64))
}

fn absorb(
    v: &Value,
    out: &mut Vec<Event>,
    pending: &mut std::collections::HashMap<String, usize>,
    session: &mut String,
) {
    let ts = v["timestamp"]
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    if v["type"] == "session_meta" {
        if let Some(id) = v["payload"]["id"].as_str() {
            *session = id.to_string();
        }
        return;
    }
    // `event_msg`, `token_usage_record`, `world_state` and `turn_context` are
    // the machinery of a run rather than anything that was said.
    if v["type"] != "response_item" {
        return;
    }
    let p = &v["payload"];
    let uid = p["id"]
        .as_str()
        .or_else(|| p["call_id"].as_str())
        .unwrap_or("")
        .to_string();

    match p["type"].as_str() {
        Some("message") => {
            let body = said(&p["content"]);
            if body.trim().is_empty() {
                return;
            }
            match p["role"].as_str() {
                // The harness introduces itself, and wraps each turn in a
                // description of the environment. Neither is the human.
                Some("developer" | "system") => {}
                Some("user") if injected(&body) => {}
                Some("user") => out.push(prose(
                    uid,
                    session.clone(),
                    ts,
                    Role::User,
                    Kind::Prompt,
                    &body,
                )),
                Some("assistant") => out.push(prose(
                    uid,
                    session.clone(),
                    ts,
                    Role::Assistant,
                    Kind::Say,
                    &body,
                )),
                _ => {}
            }
        }
        Some("custom_tool_call" | "function_call") => {
            let name = p["name"].as_str().unwrap_or("tool").to_string();
            let body = p["input"]
                .as_str()
                .or_else(|| p["arguments"].as_str())
                .unwrap_or("")
                .to_string();
            if let Some(id) = p["call_id"].as_str().or_else(|| p["id"].as_str()) {
                pending.insert(id.to_string(), out.len());
            }
            out.push(Event {
                uid,
                session: session.clone(),
                ts,
                role: Role::Tool,
                kind: Kind::Tool { name: name.clone() },
                gist: tool_gist(&name, &body),
                raw: body,
                gist_src: GistSource::Rule,
                outcome: None,
                is_error: false,
                tokens: 0,
            });
        }
        Some("custom_tool_call_output" | "function_call_output") => {
            let id = p["call_id"].as_str().unwrap_or("");
            let body = said(&p["output"]);
            let body = if body.is_empty() {
                p["output"].as_str().unwrap_or("").to_string()
            } else {
                body
            };
            if let Some(i) = pending.remove(id) {
                if let Some(e) = out.get_mut(i) {
                    e.is_error = body.to_lowercase().contains("error");
                    e.outcome = Some(result_hint(&body));
                    if !body.is_empty() {
                        e.raw = format!("{}\n\n── result ──\n{body}", e.raw);
                    }
                }
            }
        }
        // Reasoning arrives encrypted, with an empty summary: there is nothing
        // to show, so showing a row saying so would be worse than nothing.
        _ => {}
    }
}

/// Flatten Codex's content blocks into the text they carry.
fn said(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The harness wraps a turn in a description of the environment. It arrives as
/// the user, and the user did not write it.
fn injected(body: &str) -> bool {
    let t = body.trim_start();
    t.starts_with("<environment_context>")
        || t.starts_with("<user_instructions>")
        || t.starts_with("<skills_instructions>")
        || t.starts_with('<') && t.contains("_context>")
}

fn prose(
    uid: String,
    session: String,
    ts: DateTime<Utc>,
    role: Role,
    kind: Kind,
    text: &str,
) -> Event {
    let limit = if kind == Kind::Prompt {
        crate::PROMPT_VERBATIM_LIMIT
    } else {
        crate::VERBATIM_LIMIT
    };
    let one = flatten(&crate::event::preview(text));
    let (gist, src) = if one.chars().count() <= limit {
        (one, GistSource::Verbatim)
    } else {
        (
            clip(&one, limit.min(crate::PENDING_CLIP)),
            GistSource::Pending,
        )
    };
    Event {
        uid,
        session,
        ts,
        role,
        kind,
        raw: text.to_string(),
        gist,
        gist_src: src,
        outcome: None,
        is_error: false,
        tokens: 0,
    }
}

/// Codex's `exec` tool wraps a shell command in a line of JavaScript. What the
/// reader cares about is the command.
fn tool_gist(name: &str, body: &str) -> String {
    if let Some(cmd) = between(body, "\"cmd\":\"", "\"") {
        return clip(&flatten(&unescape(cmd)), 110);
    }
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(o) = v.as_object() {
            if let Some(longest) = o
                .values()
                .filter_map(|x| x.as_str())
                .max_by_key(|s| s.len())
            {
                return clip(&flatten(longest), 110);
            }
        }
    }
    clip(&flatten(&format!("{name} {body}")), 110)
}

fn unescape(s: &str) -> String {
    s.replace("\\n", " ")
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let a = s.find(open)? + open.len();
    let b = s[a..].find(close)? + a;
    Some(&s[a..b])
}

fn result_hint(body: &str) -> String {
    let t = body.trim();
    if t.is_empty() {
        return "ok".into();
    }
    let lines = t.lines().count();
    if lines <= 1 && t.chars().count() <= 48 {
        flatten(t)
    } else {
        format!("{lines} line{}", if lines == 1 { "" } else { "s" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, body: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("moan-codex-{name}.jsonl"));
        std::fs::write(&p, body).unwrap();
        p
    }

    const META: &str = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"id":"s1","cwd":"/tmp/proj","git":{"branch":"main"}}}"#;
    const ASKED: &str = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"response_item","payload":{"type":"message","role":"user","id":"u1","content":[{"type":"input_text","text":"fix the kerning"}]}}"#;
    const SAID: &str = r#"{"timestamp":"2026-01-01T00:00:03Z","type":"response_item","payload":{"type":"message","role":"assistant","id":"a1","content":[{"type":"output_text","text":"kerning pairs were unsorted"}]}}"#;
    const CALL: &str = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","call_id":"c1","input":"const r = await tools.exec_command({\"cmd\":\"cargo test --all\"})"}}"#;
    const OUT: &str = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"c1","output":[{"type":"input_text","text":"a\nb\nc"}]}}"#;

    #[test]
    fn a_rollout_becomes_the_same_feed() {
        let p = tmp(
            "basic",
            &format!("{META}\n{ASKED}\n{CALL}\n{OUT}\n{SAID}\n"),
        );
        let (evs, _) = parse_rollout(&p, 0).unwrap();
        assert_eq!(
            evs.len(),
            3,
            "prompt, tool call, reply — the output folds in"
        );
        assert_eq!(evs[0].kind, Kind::Prompt);
        assert_eq!(evs[0].gist, "fix the kerning");
        assert_eq!(evs[0].session, "s1");
        assert_eq!(
            evs[1].kind,
            Kind::Tool {
                name: "exec".into()
            }
        );
        assert_eq!(
            evs[1].gist, "cargo test --all",
            "the command, not the wrapper"
        );
        assert_eq!(evs[1].outcome.as_deref(), Some("3 lines"));
        assert_eq!(evs[2].kind, Kind::Say);
    }

    #[test]
    fn the_harness_is_not_the_human() {
        let env = r#"{"type":"response_item","payload":{"type":"message","role":"user","id":"u2","content":[{"type":"input_text","text":"<environment_context>\n  <cwd>/tmp</cwd>\n</environment_context>"}]}}"#;
        let dev = r#"{"type":"response_item","payload":{"type":"message","role":"developer","id":"d1","content":[{"type":"input_text","text":"You are the primary agent"}]}}"#;
        let noise = r#"{"type":"event_msg","payload":{"type":"token_count","total":12}}"#;
        let p = tmp(
            "cruft",
            &format!("{META}\n{env}\n{dev}\n{noise}\n{ASKED}\n"),
        );
        let (evs, _) = parse_rollout(&p, 0).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].gist, "fix the kerning");
    }

    #[test]
    fn a_half_written_record_is_left_for_the_next_read() {
        let partial = &SAID[..40];
        let p = tmp("partial", &format!("{META}\n{ASKED}\n{partial}"));
        let (evs, at) = parse_rollout(&p, 0).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(at as usize, META.len() + ASKED.len() + 2);

        std::fs::write(&p, format!("{META}\n{ASKED}\n{SAID}\n")).unwrap();
        let (more, _) = parse_rollout(&p, at).unwrap();
        assert_eq!(more.len(), 1, "and the prompt is not read twice");
        assert_eq!(more[0].kind, Kind::Say);
    }

    /// Whatever is actually on this machine, if anything is.
    ///
    /// The fixtures above pin the mapping; this pins the *format*. A rollout
    /// that Codex writes differently tomorrow fails here rather than quietly
    /// producing an empty feed — which is the failure that matters, because it
    /// looks exactly like having nothing to read.
    #[test]
    fn real_rollouts_still_parse() {
        let Ok(src) = CodexSource::new() else { return };
        let Ok(stubs) = src.list() else { return };
        let mut checked = 0;
        for stub in stubs.iter().take(5) {
            let (evs, at) = parse_rollout(&stub.path, 0).unwrap();
            assert!(at > 0, "{} read nothing at all", stub.path.display());
            if evs.is_empty() {
                continue;
            }
            checked += 1;
            assert!(
                evs.iter().any(|e| e.kind == Kind::Prompt),
                "{} has no human in it",
                stub.path.display()
            );
            assert!(
                evs.iter().all(|e| !e.gist.is_empty()),
                "{} produced a message with nothing to show",
                stub.path.display()
            );
            let facts = src.describe(stub);
            assert!(!facts.title.is_empty());
        }
        if checked == 0 {
            eprintln!("no codex rollouts on this machine — format not checked");
        }
    }

    #[test]
    fn who_spoke_last() {
        let p = tmp("tail-a", &format!("{META}\n{ASKED}\n{SAID}\n"));
        assert_eq!(last_speaker(&p), Some(true));
        let p = tmp("tail-b", &format!("{META}\n{SAID}\n{ASKED}\n"));
        assert_eq!(last_speaker(&p), Some(false));
    }
}
