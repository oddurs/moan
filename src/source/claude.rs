//! Claude Code transcripts: `~/.claude/projects/<slug>/<session>.jsonl`.
//!
//! One JSON object per line, append-only, written as the session runs. The
//! file is the source of truth; we never write to it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::event::{clip, flatten, Event, GistSource, Kind, Role};
use crate::source::{Cursor, Facts, Source, Stub};

pub struct ClaudeSource {
    root: PathBuf,
}

impl ClaudeSource {
    pub fn new() -> Result<Self> {
        let home = directories::UserDirs::new()
            .context("no home directory")?
            .home_dir()
            .to_path_buf();
        Ok(Self {
            root: home.join(".claude/projects"),
        })
    }
}

impl Source for ClaudeSource {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn list(&self) -> Result<Vec<Stub>> {
        let mut out = Vec::new();
        let Ok(projects) = std::fs::read_dir(&self.root) else {
            return Ok(out);
        };
        for project in projects.flatten() {
            let Ok(files) = std::fs::read_dir(project.path()) else {
                continue;
            };
            let slug = project.file_name().to_string_lossy().to_string();
            for f in files.flatten() {
                let p = f.path();
                if p.extension().is_none_or(|e| e != "jsonl") {
                    continue;
                }
                let Ok(md) = f.metadata() else { continue };
                if md.len() == 0 {
                    continue;
                }
                out.push(Stub {
                    id: p
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    modified: md.modified().map(Into::into).unwrap_or_else(|_| Utc::now()),
                    bytes: md.len(),
                    source: "claude",
                    slug: slug.clone(),
                    path: p,
                });
            }
        }
        crate::source::newest_first_stubs(&mut out);
        Ok(out)
    }

    fn describe(&self, stub: &Stub) -> Facts {
        let (cwd, branch) = head_meta(&stub.path);
        let title = cwd.unwrap_or_else(|| unslug(&stub.slug));
        // A worktree belongs to the project it was cut from, not to itself.
        let project = crate::source::repo_root(Path::new(&untilde(&title)))
            .map(|p| crate::ui::app::tilde(&p.to_string_lossy()))
            .unwrap_or_else(|| title.clone());
        Facts {
            assistant_last: read_tail(&stub.path),
            title,
            branch,
            project,
        }
    }

    fn parse(&self, path: &Path, _session: &str, from: &Cursor) -> Result<(Vec<Event>, Cursor)> {
        // An append-only file: the cursor is a place in it.
        let (events, at) = parse_jsonl(path, from.offset())?;
        Ok((events, Cursor::at(at)))
    }
}

/// The working directory the session ran in, from its first record.
///
/// The directory name is a lossy slug — `ag-test` and `ag/test` encode
/// identically — but every record carries the real `cwd`, so read that.
fn head_meta(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(f) = std::fs::File::open(path) else {
        return (None, None);
    };
    let mut cwd = None;
    let mut branch = None;
    // A session opens with a run of bookkeeping records carrying neither.
    for line in std::io::BufRead::lines(std::io::BufReader::new(f))
        .take(400)
        .map_while(Result::ok)
    {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if cwd.is_none() {
            cwd = v["cwd"].as_str().map(crate::ui::app::tilde);
        }
        if branch.is_none() {
            branch = v["gitBranch"]
                .as_str()
                // A detached head is not a branch worth naming.
                .filter(|b| !b.is_empty() && *b != "HEAD")
                .map(str::to_string);
        }
        if cwd.is_some() && branch.is_some() {
            break;
        }
    }
    (cwd, branch)
}

/// How much of the end of a transcript to read looking for the last word. A
/// handful of records, and a record is rarely more than a few kilobytes.
const TAIL: u64 = 96 * 1024;

/// Walk back through the last records for the first one that was somebody
/// speaking, and say whether that somebody was the assistant.
fn read_tail(path: &Path) -> Option<bool> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let from = len.saturating_sub(TAIL);
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    f.take(TAIL).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);

    // Starting mid-file means the first line is half a record.
    let mut lines: Vec<&str> = text.lines().collect();
    if from > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    for line in lines.iter().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["isSidechain"].as_bool() == Some(true) {
            continue;
        }
        match v["type"].as_str() {
            Some("assistant") => {
                let said = v["message"]["content"]
                    .as_array()
                    .is_some_and(|b| b.iter().any(|x| x["type"] == "text"));
                if said {
                    return Some(true);
                }
            }
            Some("user") => {
                // A tool result or an injected notice is not the human.
                if v["isMeta"].as_bool() == Some(true) || v["promptSource"] == "system" {
                    continue;
                }
                let c = &v["message"]["content"];
                let typed = c.is_string()
                    || c.as_array()
                        .is_some_and(|b| b.iter().any(|x| x["type"] == "text"));
                if typed {
                    return Some(false);
                }
            }
            Some("attachment") if v["attachment"]["type"] == "queued_command" => {
                return Some(false);
            }
            _ => {}
        }
    }
    None
}

/// Undo `tilde`, so a stored path can be looked at on disk again.
fn untilde(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => directories::UserDirs::new()
            .map(|d| d.home_dir().join(rest).to_string_lossy().to_string())
            .unwrap_or_else(|| p.to_string()),
        None => p.to_string(),
    }
}

/// Turn `-Users-oddurs-Code-moan` back into `~/Code/moan`. Last resort: the
/// slug cannot distinguish a hyphen in a directory name from a separator.
fn unslug(slug: &str) -> String {
    let p = slug.replace('-', "/");
    let home = directories::UserDirs::new()
        .map(|d| d.home_dir().to_string_lossy().to_string())
        .unwrap_or_default();
    match p.strip_prefix(&home) {
        Some(rest) => format!("~{rest}"),
        None => p,
    }
}

/// Read whole lines starting at `from_byte`; return events and the new offset.
///
/// The offset only advances past complete lines, so a half-written tail is
/// re-read on the next poll rather than dropped.
pub fn parse_jsonl(path: &Path, from_byte: u64) -> Result<(Vec<Event>, u64)> {
    let bytes = std::fs::read(path)?;
    if (from_byte as usize) >= bytes.len() {
        return Ok((Vec::new(), from_byte));
    }
    let tail = &bytes[from_byte as usize..];
    let complete = match tail.iter().rposition(|&b| b == b'\n') {
        Some(i) => i + 1,
        None => return Ok((Vec::new(), from_byte)),
    };
    let text = String::from_utf8_lossy(&tail[..complete]);

    let mut events = Vec::new();
    // tool_use_id -> index into `events`, so a result can fold into its call.
    let mut pending: HashMap<String, usize> = HashMap::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        absorb(&v, &mut events, &mut pending);
    }
    Ok((events, from_byte + complete as u64))
}

/// Turn one transcript record into zero or more feed events.
fn absorb(v: &Value, out: &mut Vec<Event>, pending: &mut HashMap<String, usize>) {
    let ty = v["type"].as_str().unwrap_or("");
    let session = v["sessionId"].as_str().unwrap_or("").to_string();
    let ts = v["timestamp"]
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let uid = v["uuid"].as_str().unwrap_or("").to_string();

    match ty {
        // Sidechains are subagent traffic. The parent's Task line already
        // stands for them; showing both doubles the feed.
        _ if v["isSidechain"].as_bool() == Some(true) => {}

        // A message typed while the assistant was still working. It arrives
        // as an attachment, but it is the human speaking — the one attachment
        // that is not bookkeeping.
        "attachment" if v["attachment"]["type"] == "queued_command" => {
            if v["attachment"]["origin"]["kind"] == "human" {
                let text = v["attachment"]["prompt"].as_str().unwrap_or("");
                // The attachment carries when it was typed, not when it landed.
                let typed = v["attachment"]["timestamp"]
                    .as_str()
                    .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or(ts);
                push_prompt(out, uid, session, typed, text);
            }
        }

        // Session bookkeeping the human never asked to see.
        "attachment"
        | "file-history-snapshot"
        | "ai-title"
        | "mode"
        | "permission-mode"
        | "last-prompt"
        | "bridge-session"
        | "atis-latch"
        | "summary" => {}

        "system" => {
            let sub = v["subtype"].as_str().unwrap_or("");
            // `compact_boundary` is a real event in the story of a session.
            if sub == "compact_boundary" {
                out.push(Event {
                    uid,
                    session,
                    ts,
                    role: Role::System,
                    kind: Kind::Note,
                    raw: "context compacted".into(),
                    gist: format!(
                        "context compacted after {} messages",
                        v["messageCount"].as_u64().unwrap_or(0)
                    ),
                    gist_src: GistSource::Rule,
                    outcome: None,
                    is_error: false,
                    tokens: 0,
                });
            }
        }

        "user" => {
            // Meta users are injected reminders, not the human speaking.
            if v["isMeta"].as_bool() == Some(true) {
                return;
            }
            // Nor is a task notification, which the harness writes into the
            // user's turn. Its summary is worth a line; its plumbing is not.
            if v["promptSource"] == "system" {
                if let Some(sum) = between(
                    v["message"]["content"].as_str().unwrap_or(""),
                    "<summary>",
                    "</summary>",
                ) {
                    out.push(Event {
                        uid,
                        session,
                        ts,
                        role: Role::System,
                        kind: Kind::Note,
                        raw: v["message"]["content"].as_str().unwrap_or("").to_string(),
                        gist: clip(&flatten(sum), 96),
                        gist_src: GistSource::Rule,
                        outcome: None,
                        is_error: false,
                        tokens: 0,
                    });
                }
                return;
            }
            let content = &v["message"]["content"];
            if let Some(s) = content.as_str() {
                push_prompt(out, uid, session, ts, s);
            } else if let Some(arr) = content.as_array() {
                for b in arr {
                    match b["type"].as_str() {
                        Some("text") => push_prompt(
                            out,
                            uid.clone(),
                            session.clone(),
                            ts,
                            b["text"].as_str().unwrap_or(""),
                        ),
                        Some("tool_result") => fold_result(b, out, pending, &session, ts),
                        _ => {}
                    }
                }
            }
        }

        "assistant" => {
            let msg = &v["message"];
            let tokens = msg["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;
            let Some(blocks) = msg["content"].as_array() else {
                return;
            };
            // Text and thinking blocks share their message's uuid. One per
            // message is all anyone has ever written, but the format does not
            // promise it, and the store keys rows on this. The first block
            // keeps the bare uuid so nothing already stored has to move.
            let mut nth = 0usize;
            let mut next_uid = |uid: &str| {
                nth += 1;
                if nth == 1 {
                    uid.to_string()
                } else {
                    format!("{uid}#{}", nth - 1)
                }
            };
            for b in blocks {
                match b["type"].as_str() {
                    Some("text") => {
                        let t = b["text"].as_str().unwrap_or("").trim();
                        if t.is_empty() {
                            continue;
                        }
                        out.push(prose(
                            next_uid(&uid),
                            session.clone(),
                            ts,
                            Role::Assistant,
                            Kind::Say,
                            t,
                            tokens,
                        ));
                    }
                    Some("thinking") => {
                        let t = b["thinking"].as_str().unwrap_or("").trim();
                        if t.is_empty() {
                            continue;
                        }
                        out.push(prose(
                            next_uid(&uid),
                            session.clone(),
                            ts,
                            Role::Assistant,
                            Kind::Think,
                            t,
                            0,
                        ));
                    }
                    Some("tool_use") => {
                        let name = b["name"].as_str().unwrap_or("tool").to_string();
                        let input = &b["input"];
                        let gist = tool_gist(&name, input);
                        let raw = tool_body(&name, input);
                        if let Some(id) = b["id"].as_str() {
                            pending.insert(id.to_string(), out.len());
                        }
                        out.push(Event {
                            uid: b["id"].as_str().unwrap_or(&uid).to_string(),
                            session: session.clone(),
                            ts,
                            role: Role::Tool,
                            kind: Kind::Tool { name },
                            raw,
                            gist,
                            gist_src: GistSource::Rule,
                            outcome: None,
                            is_error: false,
                            tokens: 0,
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn push_prompt(out: &mut Vec<Event>, uid: String, session: String, ts: DateTime<Utc>, s: &str) {
    let s = s.trim();
    if s.is_empty() {
        return;
    }
    // Slash commands and their expansions arrive as prompts; keep the command,
    // drop the several-kilobyte body the harness pasted in behind it.
    if let Some(cmd) = slash_command(s) {
        out.push(Event {
            uid,
            session,
            ts,
            role: Role::User,
            kind: Kind::Prompt,
            raw: s.to_string(),
            gist: cmd,
            gist_src: GistSource::Rule,
            outcome: None,
            is_error: false,
            tokens: 0,
        });
        return;
    }
    // A prompt is shown as written. A summary of your own sentence in
    // somebody else's voice is worse than the sentence.
    let one = flatten(s);
    let (gist, src) = if one.chars().count() <= crate::PROMPT_VERBATIM_LIMIT {
        (one, GistSource::Verbatim)
    } else {
        (
            clip(&one, crate::PROMPT_VERBATIM_LIMIT),
            GistSource::Pending,
        )
    };
    out.push(Event {
        uid,
        session,
        ts,
        role: Role::User,
        kind: Kind::Prompt,
        raw: s.to_string(),
        gist,
        gist_src: src,
        outcome: None,
        is_error: false,
        tokens: 0,
    });
}

/// Recognise the `<command-name>` envelope Claude Code wraps slash commands in.
fn slash_command(s: &str) -> Option<String> {
    let name = between(s, "<command-name>", "</command-name>")?;
    let args = between(s, "<command-args>", "</command-args>").unwrap_or_default();
    Some(
        flatten(&format!("{} {}", name.trim(), args.trim()))
            .trim()
            .to_string(),
    )
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let a = s.find(open)? + open.len();
    let b = s[a..].find(close)? + a;
    Some(&s[a..b])
}

/// Build a prose event, marking it for the model only if it is long enough to
/// be worth a call.
fn prose(
    uid: String,
    session: String,
    ts: DateTime<Utc>,
    role: Role,
    kind: Kind,
    text: &str,
    tokens: u32,
) -> Event {
    let one = flatten(text);
    let (gist, src) = if one.chars().count() <= crate::VERBATIM_LIMIT {
        (one, GistSource::Verbatim)
    } else {
        // Until the model answers, show the message's own opening — as prose,
        // not as the markdown it was written in.
        (
            clip(&flatten(&crate::event::preview(text)), crate::PENDING_CLIP),
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
        tokens,
    }
}

/// Attach a tool result to the call it answers; surface it alone only on error.
fn fold_result(
    b: &Value,
    out: &mut Vec<Event>,
    pending: &mut HashMap<String, usize>,
    session: &str,
    ts: DateTime<Utc>,
) {
    let id = b["tool_use_id"].as_str().unwrap_or("");
    let is_error = b["is_error"].as_bool().unwrap_or(false);
    let body = result_text(&b["content"]);

    if let Some(i) = pending.remove(id) {
        if let Some(ev) = out.get_mut(i) {
            ev.is_error = is_error;
            ev.outcome = Some(result_hint(&body, is_error));
            if !body.is_empty() {
                ev.raw = format!("{}\n\n── result ──\n{}", ev.raw, body);
            }
            return;
        }
    }
    // The call is in an earlier chunk we have already handed off. Only an
    // error is worth its own line at this point.
    if is_error {
        out.push(Event {
            uid: id.to_string(),
            session: session.to_string(),
            ts,
            role: Role::Tool,
            kind: Kind::Fail,
            raw: body.clone(),
            gist: clip(&flatten(&body), 90),
            gist_src: GistSource::Rule,
            outcome: None,
            is_error: true,
            tokens: 0,
        });
    }
}

fn result_text(v: &Value) -> String {
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

/// A few words about how a tool call turned out — `42 lines`, `exit 1`.
fn result_hint(body: &str, is_error: bool) -> String {
    let t = body.trim();
    if t.is_empty() {
        return if is_error {
            "failed".into()
        } else {
            "ok".into()
        };
    }
    if is_error {
        return clip(&flatten(t), 60);
    }
    let lines = t.lines().count();
    if lines <= 1 && t.chars().count() <= 48 {
        flatten(t)
    } else {
        format!("{lines} line{}", if lines == 1 { "" } else { "s" })
    }
}

/// Deterministic one-liners for tool calls.
///
/// This is where most of the compression happens, and it costs nothing: a tool
/// call is already structured, so a model adds latency without adding meaning.
fn tool_gist(name: &str, input: &Value) -> String {
    let s = |k: &str| input[k].as_str().unwrap_or("").to_string();
    let text = match name {
        "Bash" | "BashOutput" => return bash_gist(&s("command")),
        "Read" => {
            let f = base(&s("file_path"));
            match (input["offset"].as_u64(), input["limit"].as_u64()) {
                (Some(o), Some(l)) => format!("{f}:{o}–{}", o + l),
                _ => f,
            }
        }
        "Write" => {
            let lines = s("content").lines().count();
            format!("{} ({lines} lines)", base(&s("file_path")))
        }
        "Edit" | "NotebookEdit" => {
            let old = s("old_string").lines().count();
            let new = s("new_string").lines().count();
            format!("{} +{new}/−{old}", base(&s("file_path")))
        }
        "Grep" => {
            let p = s("pattern");
            let where_ = s("path");
            if where_.is_empty() {
                p
            } else {
                format!("{p}  in {}", base(&where_))
            }
        }
        "Glob" => s("pattern"),
        "WebFetch" => s("url"),
        "WebSearch" => s("query"),
        "Task" | "Agent" => {
            let d = s("description");
            let t = s("subagent_type");
            if t.is_empty() {
                d
            } else {
                format!("{t}: {d}")
            }
        }
        "Skill" => format!("/{} {}", s("skill"), s("args")).trim().to_string(),
        "TodoWrite" => todo_gist(input),
        // The one tool call that is addressed to the reader. Its arguments are
        // nested, so the generic rule below found no text at all and drew a
        // blank line where the question should have been.
        "AskUserQuestion" => {
            let qs = input["questions"].as_array().cloned().unwrap_or_default();
            let first = qs
                .first()
                .and_then(|q| q["question"].as_str())
                .unwrap_or("asked a question")
                .to_string();
            match qs.len() {
                0 | 1 => first,
                n => format!("{first}  (+{} more)", n - 1),
            }
        }
        _ => {
            let d = s("description");
            if !d.is_empty() {
                d
            } else {
                // Unknown tool: show its most substantial string argument.
                // Whatever text it carries — and failing that its own name,
                // because a row with nothing on it is worse than a vague one.
                input
                    .as_object()
                    .and_then(|o| {
                        o.values()
                            .filter_map(|v| v.as_str())
                            .max_by_key(|s| s.len())
                            .map(str::to_string)
                    })
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| name.to_string())
            }
        }
    };
    clip(&flatten(&text), 110)
}

/// What to show when a tool line is expanded.
///
/// Pretty-printed JSON is right for an unfamiliar tool, but for the common
/// ones the argument *is* the content, and JSON escaping makes it unreadable.
fn tool_body(name: &str, input: &Value) -> String {
    let s = |k: &str| input[k].as_str().unwrap_or("").to_string();
    match name {
        "Bash" | "BashOutput" => s("command"),
        "Write" => format!("{}\n\n{}", s("file_path"), s("content")),
        "Edit" => format!(
            "{}\n\n─ before ─\n{}\n\n─ after ─\n{}",
            s("file_path"),
            s("old_string"),
            s("new_string")
        ),
        "Task" | "Agent" => s("prompt"),
        _ => serde_json::to_string_pretty(input).unwrap_or_default(),
    }
}

/// A shell command, minus the payload.
///
/// Agents write files with heredocs, so the "command" is often a hundred lines
/// of file content. The interesting part is the line before the `<<`.
fn bash_gist(cmd: &str) -> String {
    let cmd = home_tilde(cmd);
    if let Some(i) = cmd.find("<<") {
        let head = flatten(&cmd[..i]);
        // Subtract the opening line and the closing delimiter.
        let body = cmd[i..].lines().count().saturating_sub(2);
        return clip(&format!("{head} ‹{body} lines›"), 110);
    }
    clip(&flatten(&cmd), 110)
}

fn home_tilde(s: &str) -> String {
    let home = directories::UserDirs::new()
        .map(|d| d.home_dir().to_string_lossy().to_string())
        .unwrap_or_default();
    if home.is_empty() {
        s.to_string()
    } else {
        s.replace(&home, "~")
    }
}

fn todo_gist(input: &Value) -> String {
    let todos = input["todos"].as_array().cloned().unwrap_or_default();
    let done = todos.iter().filter(|t| t["status"] == "completed").count();
    let active = todos
        .iter()
        .find(|t| t["status"] == "in_progress")
        .and_then(|t| t["activeForm"].as_str().or_else(|| t["content"].as_str()))
        .unwrap_or("");
    if active.is_empty() {
        format!("{done}/{} done", todos.len())
    } else {
        format!("{done}/{} · {active}", todos.len())
    }
}

fn base(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Kind;

    fn tmp(name: &str, body: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("moan-test-{name}.jsonl"));
        std::fs::write(&p, body).unwrap();
        p
    }

    const PROMPT: &str = r#"{"type":"user","uuid":"u1","sessionId":"s","timestamp":"2026-01-01T00:00:00Z","message":{"role":"user","content":"fix the drift"}}"#;
    const CALL: &str = r#"{"type":"assistant","uuid":"a1","sessionId":"s","timestamp":"2026-01-01T00:00:01Z","message":{"usage":{"output_tokens":12},"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cargo   test\n --all"}}]}}"#;
    const RESULT: &str = r#"{"type":"user","uuid":"u2","sessionId":"s","timestamp":"2026-01-01T00:00:02Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"a\nb\nc"}]}}"#;

    #[test]
    fn the_title_is_the_real_cwd_not_the_slug() {
        // `~/Code/ag-test` slugs to `-Users-oddurs-Code-ag-test`, which is
        // indistinguishable from `~/Code/ag/test`. The record knows better.
        let bookkeeping = r#"{"type":"bridge-session","sessionId":"s"}"#;
        let withcwd = r#"{"type":"user","uuid":"u","sessionId":"s","cwd":"/tmp/ag-test","message":{"content":"hi"}}"#;
        let p = tmp("cwd", &format!("{bookkeeping}\n{withcwd}\n"));
        assert_eq!(head_meta(&p).0.as_deref(), Some("/tmp/ag-test"));
    }

    #[test]
    fn folds_a_result_into_its_call() {
        let p = tmp("fold", &format!("{PROMPT}\n{CALL}\n{RESULT}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(evs.len(), 2, "the result must not get its own line");
        assert_eq!(
            evs[1].kind,
            Kind::Tool {
                name: "Bash".into()
            }
        );
        assert_eq!(evs[1].gist, "cargo test --all");
        assert_eq!(evs[1].outcome.as_deref(), Some("3 lines"));
        assert!(evs[1].raw.contains("── result ──"));
    }

    #[test]
    fn a_half_written_line_is_left_for_the_next_poll() {
        let partial = &CALL[..40];
        let p = tmp("partial", &format!("{PROMPT}\n{partial}"));
        let (evs, off) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(
            off as usize,
            PROMPT.len() + 1,
            "offset stops at the last newline"
        );

        // Now the line completes; resuming must not repeat the prompt.
        std::fs::write(&p, format!("{PROMPT}\n{CALL}\n")).unwrap();
        let (more, _) = parse_jsonl(&p, off).unwrap();
        assert_eq!(more.len(), 1);
        assert_eq!(
            more[0].kind,
            Kind::Tool {
                name: "Bash".into()
            }
        );
    }

    #[test]
    fn a_message_typed_mid_turn_still_shows() {
        // Typing while the assistant works records the message as an
        // attachment. Dropping every attachment as bookkeeping loses it.
        let queued = r#"{"type":"attachment","uuid":"q1","sessionId":"s","timestamp":"2026-01-01T00:05:00Z","attachment":{"type":"queued_command","prompt":"and hide the bash lines","origin":{"kind":"human"},"timestamp":"2026-01-01T00:04:00Z"}}"#;
        let p = tmp("queued", &format!("{PROMPT}\n{queued}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[1].kind, Kind::Prompt);
        assert_eq!(evs[1].gist, "and hide the bash lines");
        // Shown at the moment it was typed, not when it was delivered.
        assert_eq!(evs[1].ts.to_rfc3339(), "2026-01-01T00:04:00+00:00");
    }

    #[test]
    fn a_queued_command_from_a_hook_is_not_the_human() {
        let injected = r#"{"type":"attachment","uuid":"q2","sessionId":"s","attachment":{"type":"queued_command","prompt":"injected","origin":{"kind":"hook"}}}"#;
        let p = tmp("injected", &format!("{injected}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert!(evs.is_empty());
    }

    #[test]
    fn a_prompt_is_shown_in_your_own_words() {
        // Long enough that assistant prose would be sent to a model.
        let words = "do we have an openrouter key in one of our projects i think namesync has one";
        let line = format!(
            r#"{{"type":"user","uuid":"u5","sessionId":"s","message":{{"content":"{words}"}}}}"#
        );
        let p = tmp("verbatim", &format!("{line}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(evs[0].gist, words, "a prompt is never paraphrased");
        assert_eq!(evs[0].gist_src, GistSource::Verbatim);
    }

    #[test]
    fn a_task_notification_is_not_you_speaking() {
        // The harness writes these into the user's turn. Attributing them to
        // the human puts words in their mouth.
        let note = r#"{"type":"user","uuid":"n1","sessionId":"s","promptSource":"system","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>\n<summary>Monitor event: CI checks settled</summary>\n<status>complete</status>\n</task-notification>"}}"#;
        let plumbing = r#"{"type":"user","uuid":"n2","sessionId":"s","promptSource":"system","message":{"content":"<task-notification><tool-use-id>toolu_1</tool-use-id></task-notification>"}}"#;
        let p = tmp("notif", &format!("{note}\n{plumbing}\n{PROMPT}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(
            evs.len(),
            2,
            "the summary and the real prompt, not the plumbing"
        );
        assert_eq!(evs[0].kind, Kind::Note);
        assert_eq!(evs[0].gist, "Monitor event: CI checks settled");
        assert_eq!(evs[1].kind, Kind::Prompt);
    }

    #[test]
    fn two_blocks_in_one_message_get_distinct_ids() {
        // The store keys rows on (session, uid). Blocks of one message share
        // the message's uuid, so the second onwards has to be distinguished.
        let two = r#"{"type":"assistant","uuid":"a9","sessionId":"s","message":{"content":[{"type":"text","text":"first"},{"type":"text","text":"second"}]}}"#;
        let p = tmp("twoblocks", &format!("{two}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].uid, "a9", "the first keeps the bare uuid");
        assert_eq!(evs[1].uid, "a9#1");
    }

    #[test]
    fn the_tail_says_who_spoke_last() {
        let said = r#"{"type":"assistant","uuid":"a","sessionId":"s","message":{"content":[{"type":"text","text":"done"}]}}"#;
        let asked = r#"{"type":"user","uuid":"u","sessionId":"s","message":{"content":"do it"}}"#;
        let result = r#"{"type":"user","uuid":"r","sessionId":"s","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"ok"}]}}"#;
        let notice = r#"{"type":"user","uuid":"n","sessionId":"s","promptSource":"system","message":{"content":"<task-notification/>"}}"#;

        // Waiting on a human: the assistant finished.
        let p = tmp("tail-a", &format!("{asked}\n{said}\n{result}\n{notice}\n"));
        assert_eq!(
            read_tail(&p),
            Some(true),
            "a tool result is not somebody speaking"
        );

        // Working: the human asked last.
        let p = tmp("tail-b", &format!("{said}\n{asked}\n"));
        assert_eq!(read_tail(&p), Some(false));

        // A message typed while it worked counts as the human.
        let queued = r#"{"type":"attachment","uuid":"q","sessionId":"s","attachment":{"type":"queued_command","prompt":"and this","origin":{"kind":"human"}}}"#;
        let p = tmp("tail-c", &format!("{said}\n{queued}\n"));
        assert_eq!(read_tail(&p), Some(false));

        // Nothing to go on.
        let p = tmp("tail-d", &format!("{result}\n"));
        assert_eq!(read_tail(&p), None);
    }

    /// The same guard for Claude Code: the fixtures pin the mapping, this
    /// pins the format. An empty feed is the failure that matters, because it
    /// looks exactly like having nothing to read.
    #[test]
    fn real_transcripts_still_parse() {
        let Ok(src) = ClaudeSource::new() else { return };
        let Ok(stubs) = src.list() else { return };
        for stub in stubs.iter().take(5) {
            let (evs, at) = parse_jsonl(&stub.path, 0).unwrap();
            assert!(at > 0, "{} read nothing", stub.path.display());
            if evs.is_empty() {
                continue;
            }
            assert!(
                evs.iter().all(|e| !e.gist.is_empty()),
                "{} produced a message with nothing to show",
                stub.path.display()
            );
            assert!(!src.describe(stub).title.is_empty());
        }
    }

    #[test]
    fn drops_cruft() {
        let cruft = [
            r#"{"type":"attachment","uuid":"x","attachment":{"type":"total_tokens_reminder"}}"#,
            r#"{"type":"file-history-snapshot","messageId":"m"}"#,
            r#"{"type":"ai-title","aiTitle":"t"}"#,
            r#"{"type":"user","uuid":"m1","isMeta":true,"message":{"content":"reminder"}}"#,
            r#"{"type":"assistant","uuid":"sc","isSidechain":true,"message":{"content":[{"type":"text","text":"subagent"}]}}"#,
        ]
        .join("\n");
        let p = tmp("cruft", &format!("{cruft}\n{PROMPT}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, Kind::Prompt);
    }

    #[test]
    fn a_slash_command_shows_as_the_command() {
        let line = r#"{"type":"user","uuid":"u1","sessionId":"s","message":{"content":"<command-name>/ship</command-name>\n<command-args>--draft</command-args>\n<command-contents>...a great deal of text...</command-contents>"}}"#;
        let p = tmp("slash", &format!("{line}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(evs[0].gist, "/ship --draft");
    }

    #[test]
    fn an_orphan_failure_gets_its_own_line() {
        let line = r#"{"type":"user","uuid":"u9","sessionId":"s","message":{"content":[{"type":"tool_result","tool_use_id":"gone","is_error":true,"content":"Exit code 143\nCommand timed out"}]}}"#;
        let p = tmp("orphan", &format!("{line}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, Kind::Fail);
        assert!(evs[0].is_error);
    }

    #[test]
    fn short_prose_stands_as_its_own_gist() {
        let p = tmp("short", &format!("{PROMPT}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(evs[0].gist, "fix the drift");
        assert_eq!(evs[0].gist_src, GistSource::Verbatim);
    }

    #[test]
    fn long_prose_is_queued_for_the_model() {
        let long = "word ".repeat(80);
        let line = format!(
            r#"{{"type":"assistant","uuid":"a2","sessionId":"s","message":{{"content":[{{"type":"text","text":"{long}"}}]}}}}"#
        );
        let p = tmp("long", &format!("{line}\n"));
        let (evs, _) = parse_jsonl(&p, 0).unwrap();
        assert_eq!(evs[0].gist_src, GistSource::Pending);
        assert!(evs[0].gist.ends_with('…'));
    }

    #[test]
    fn a_heredoc_shows_as_its_command_not_its_payload() {
        let cmd = format!(
            "cat > Synth.swift <<'EOF'\n{}EOF\n",
            "let x = 1\n".repeat(68)
        );
        assert_eq!(bash_gist(&cmd), "cat > Synth.swift ‹68 lines›");
    }

    #[test]
    fn no_tool_call_ever_draws_a_blank_line() {
        // `AskUserQuestion` keeps its text nested, so the generic rule found
        // nothing and drew an empty row — for the one tool call that is
        // addressed to the reader.
        let ask = serde_json::json!({
            "questions": [
                {"question": "Which direction here?", "header": "Direction"},
                {"question": "And what should it be called?", "header": "Name"}
            ]
        });
        let g = tool_gist("AskUserQuestion", &ask);
        assert!(g.starts_with("Which direction here?"), "{g}");
        assert!(g.contains("+1 more"));

        // And nothing at all still names itself rather than showing nothing.
        assert_eq!(tool_gist("Mystery", &serde_json::json!({})), "Mystery");
        assert_eq!(
            tool_gist("Mystery", &serde_json::json!({"a": "   "})),
            "Mystery"
        );
    }

    #[test]
    fn tool_gists_stay_tight() {
        let cases = [
            (
                "Read",
                serde_json::json!({"file_path": "/a/b/parser.rs"}),
                "parser.rs",
            ),
            (
                "Edit",
                serde_json::json!({"file_path": "/a/x.rs", "old_string": "1\n2", "new_string": "1\n2\n3"}),
                "x.rs +3/−2",
            ),
            (
                "Write",
                serde_json::json!({"file_path": "/a/y.rs", "content": "a\nb"}),
                "y.rs (2 lines)",
            ),
            ("Glob", serde_json::json!({"pattern": "**/*.rs"}), "**/*.rs"),
            (
                "Task",
                serde_json::json!({"description": "hunt bug", "subagent_type": "Explore"}),
                "Explore: hunt bug",
            ),
        ];
        for (name, input, want) in cases {
            assert_eq!(tool_gist(name, &input), want, "{name}");
        }
    }
}
