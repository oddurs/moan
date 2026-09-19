//! opencode transcripts, which are not files.
//!
//! opencode keeps its conversations in a SQLite database at
//! `~/.local/share/opencode/opencode.db`: a `session` row per conversation, a
//! `message` per turn, and a `part` per piece of one, each with its payload as
//! JSON in a `data` column.
//!
//! The cursor is therefore a time rather than a place. It has to be: a part is
//! written when a tool *starts* and written again when it finishes, so a
//! watermark over insertion order would read the running version once and
//! never see the result.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

use crate::event::{clip, flatten, Event, GistSource, Kind, Role};
use crate::source::{Cursor, Facts, Source, Stub};

pub struct OpencodeSource {
    db: PathBuf,
}

impl OpencodeSource {
    pub fn new() -> Result<Self> {
        let home = directories::UserDirs::new()
            .context("no home directory")?
            .home_dir()
            .to_path_buf();
        Ok(Self {
            db: home.join(".local/share/opencode/opencode.db"),
        })
    }
}

/// Open the store without touching it. opencode may be running.
fn read_only(path: &Path) -> Result<rusqlite::Connection> {
    use rusqlite::OpenFlags;
    let c = rusqlite::Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    c.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(c)
}

fn when(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Utc::now)
}

impl Source for OpencodeSource {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn list(&self) -> Result<Vec<Stub>> {
        if !self.db.exists() {
            return Ok(Vec::new());
        }
        let c = read_only(&self.db)?;
        let mut q = c.prepare(
            "select s.id, s.time_updated,
                    (select count(*) from part p where p.session_id = s.id),
                    (select coalesce(sum(length(p.data)), 0) from part p
                     where p.session_id = s.id)
             from session s",
        )?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for (id, updated, parts, bytes) in rows.flatten() {
            if parts == 0 {
                continue;
            }
            out.push(Stub {
                id,
                modified: when(updated),
                // There is no file, so this is how much of the store this
                // conversation takes — which is what the number means for the
                // others too.
                bytes: u64::try_from(bytes).unwrap_or(0),
                source: "opencode",
                slug: String::new(),
                path: self.db.clone(),
            });
        }
        crate::source::newest_first_stubs(&mut out);
        Ok(out)
    }

    fn describe(&self, stub: &Stub) -> Facts {
        let dir = read_only(&self.db)
            .ok()
            .and_then(|c| {
                c.query_row(
                    "select directory from session where id = ?1",
                    rusqlite::params![stub.id],
                    |r| r.get::<_, String>(0),
                )
                .ok()
            })
            .map(|d| crate::ui::app::tilde(&d));
        let title = dir.unwrap_or_else(|| "opencode".into());
        let project = crate::source::repo_root(Path::new(&untilde(&title)))
            .map(|p| crate::ui::app::tilde(&p.to_string_lossy()))
            .unwrap_or_else(|| title.clone());
        Facts {
            assistant_last: last_speaker(&self.db, &stub.id),
            title,
            branch: None,
            project,
        }
    }

    fn parse(&self, _path: &Path, session: &str, from: &Cursor) -> Result<(Vec<Event>, Cursor)> {
        // The cursor is a time, and the conversation is given, so the two
        // together say exactly what to read.
        read(&self.db, session, i64::try_from(from.offset()).unwrap_or(0))
    }
}

/// The cursor is simply the millisecond last read.
pub fn cursor_for(since: i64) -> Cursor {
    Cursor::at(u64::try_from(since).unwrap_or(0))
}

fn untilde(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => directories::UserDirs::new()
            .map(|d| d.home_dir().join(rest).to_string_lossy().to_string())
            .unwrap_or_else(|| p.to_string()),
        None => p.to_string(),
    }
}

fn last_speaker(db: &Path, session: &str) -> Option<bool> {
    let c = read_only(db).ok()?;
    c.query_row(
        "select json_extract(m.data, '$.role') from message m
         where m.session_id = ?1 order by m.time_created desc limit 1",
        rusqlite::params![session],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .map(|role| role == "assistant")
}

/// Everything in a conversation written at or after `since`.
///
/// At, not after: a part finished in the same millisecond as the last one read
/// would otherwise be skipped for ever. Re-reading is free — the archive keys
/// rows on `(conversation, id)` and inserts nothing it already has.
pub fn read(db: &Path, session: &str, since: i64) -> Result<(Vec<Event>, Cursor)> {
    let c = read_only(db)?;
    let mut q = c.prepare(
        "select p.id, p.message_id, p.time_created, p.time_updated, p.data,
                json_extract(m.data, '$.role')
         from part p join message m on m.id = p.message_id
         where p.session_id = ?1 and p.time_updated >= ?2
         order by p.time_updated, p.time_created, p.id",
    )?;
    let rows = q.query_map(rusqlite::params![session, since], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?.unwrap_or_default(),
        ))
    })?;

    let mut out = Vec::new();
    let mut high = since;
    for (id, created, updated, data, role) in rows.flatten() {
        high = high.max(updated);
        let Ok(v) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if let Some(e) = event_of(&v, &id, session, when(created), &role) {
            out.push(e);
        }
    }
    Ok((out, cursor_for(high)))
}

fn event_of(v: &Value, id: &str, session: &str, ts: DateTime<Utc>, role: &str) -> Option<Event> {
    match v["type"].as_str()? {
        "text" => {
            let body = v["text"].as_str().unwrap_or("").trim();
            if body.is_empty() {
                return None;
            }
            let (r, kind) = if role == "assistant" {
                (Role::Assistant, Kind::Say)
            } else {
                (Role::User, Kind::Prompt)
            };
            Some(prose(id, session, ts, r, kind, body))
        }
        "reasoning" => {
            let body = v["text"].as_str().unwrap_or("").trim();
            if body.is_empty() {
                return None;
            }
            Some(prose(id, session, ts, Role::Assistant, Kind::Think, body))
        }
        "tool" => {
            let state = &v["state"];
            // A tool still running has no result. Its row is written again
            // when it finishes, and the cursor is a time, so it comes back.
            // Emitting it now would mean showing a call that never got an
            // answer, because a re-read inserts nothing.
            let status = state["status"].as_str().unwrap_or("");
            if status == "running" || status == "pending" {
                return None;
            }
            let name = v["tool"].as_str().unwrap_or("tool").to_string();
            let input = &state["input"];
            let body = serde_json::to_string_pretty(input).unwrap_or_default();
            let output = state["output"].as_str().unwrap_or("");
            let failed = status == "error";
            Some(Event {
                uid: id.to_string(),
                session: session.to_string(),
                ts,
                role: Role::Tool,
                kind: Kind::Tool { name: name.clone() },
                gist: tool_gist(&name, input),
                raw: if output.is_empty() {
                    body
                } else {
                    format!("{body}\n\n── result ──\n{output}")
                },
                gist_src: GistSource::Rule,
                outcome: Some(result_hint(output, failed)),
                is_error: failed,
                tokens: 0,
            })
        }
        // `step-start` and `step-finish` are the shape of a turn, not anything
        // that was said.
        _ => None,
    }
}

fn prose(id: &str, session: &str, ts: DateTime<Utc>, role: Role, kind: Kind, text: &str) -> Event {
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
        uid: id.to_string(),
        session: session.to_string(),
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

fn tool_gist(name: &str, input: &Value) -> String {
    let s = |k: &str| input[k].as_str().unwrap_or("").to_string();
    let text = match name {
        "bash" => s("command"),
        "read" | "write" | "edit" => base(&s("filePath")),
        "grep" => s("pattern"),
        "glob" => s("pattern"),
        "webfetch" => s("url"),
        _ => input
            .as_object()
            .and_then(|o| {
                o.values()
                    .filter_map(Value::as_str)
                    .max_by_key(|s| s.len())
                    .map(str::to_string)
            })
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| name.to_string()),
    };
    let text = if text.trim().is_empty() {
        name.to_string()
    } else {
        text
    };
    clip(&flatten(&text), 110)
}

fn base(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |s| s.to_string_lossy().to_string())
}

fn result_hint(body: &str, failed: bool) -> String {
    let t = body.trim();
    if t.is_empty() {
        return if failed { "failed".into() } else { "ok".into() };
    }
    if failed {
        return clip(&flatten(t), 60);
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

    /// A store shaped like opencode's, with whatever rows a test wants.
    fn store(name: &str, parts: &[(&str, &str, i64, i64, &str)]) -> PathBuf {
        let p = std::env::temp_dir().join(format!("moan-oc-{name}.db"));
        let _ = std::fs::remove_file(&p);
        let c = rusqlite::Connection::open(&p).unwrap();
        c.execute_batch(
            "create table session (id text primary key, directory text, title text, time_updated integer);
             create table message (id text primary key, session_id text, time_created integer, data text);
             create table part (id text primary key, message_id text, session_id text,
                                time_created integer, time_updated integer, data text);
             insert into session values ('s1', '/tmp/proj', 'x', 10);
             insert into message values ('m1', 's1', 1, '{\"role\":\"user\"}');
             insert into message values ('m2', 's1', 2, '{\"role\":\"assistant\"}');",
        )
        .unwrap();
        for (id, msg, created, updated, data) in parts {
            c.execute(
                "insert into part values (?1, ?2, 's1', ?3, ?4, ?5)",
                rusqlite::params![id, msg, created, updated, data],
            )
            .unwrap();
        }
        p
    }

    #[test]
    fn a_conversation_becomes_the_same_feed() {
        let db = store(
            "basic",
            &[
                (
                    "p1",
                    "m1",
                    1,
                    1,
                    r#"{"type":"text","text":"plan a ui update"}"#,
                ),
                ("p2", "m2", 2, 2, r#"{"type":"step-start"}"#),
                (
                    "p3",
                    "m2",
                    3,
                    3,
                    r#"{"type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"/a/b/site.tsx"},"output":"x\ny"}}"#,
                ),
                (
                    "p4",
                    "m2",
                    4,
                    4,
                    r#"{"type":"text","text":"the header was unaligned"}"#,
                ),
            ],
        );
        let (evs, at) = read(&db, "s1", 0).unwrap();
        assert_eq!(
            evs.len(),
            3,
            "the step marker is not something anybody said"
        );
        assert_eq!(evs[0].kind, Kind::Prompt);
        assert_eq!(evs[0].gist, "plan a ui update");
        assert_eq!(
            evs[1].kind,
            Kind::Tool {
                name: "read".into()
            }
        );
        assert_eq!(evs[1].gist, "site.tsx");
        assert_eq!(evs[1].outcome.as_deref(), Some("2 lines"));
        assert_eq!(evs[2].kind, Kind::Say);
        assert_eq!(at, cursor_for(4));
    }

    #[test]
    fn a_tool_still_running_is_not_read_until_it_finishes() {
        // Its row is written when it starts and written again when it ends. A
        // watermark over insertion order would read the running one and never
        // see the answer, because the archive will not insert it twice.
        let running = r#"{"type":"tool","tool":"bash","state":{"status":"running","input":{"command":"cargo test"}}}"#;
        let db = store("torn", &[("p1", "m2", 1, 1, running)]);
        let (evs, at) = read(&db, "s1", 0).unwrap();
        assert!(evs.is_empty(), "nothing to show until it has an answer");
        assert_eq!(at, cursor_for(1));

        // It finishes: same row, later timestamp.
        let done = r#"{"type":"tool","tool":"bash","state":{"status":"completed","input":{"command":"cargo test"},"output":"28 passed"}}"#;
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute(
            "update part set time_updated = 9, data = ?1 where id = 'p1'",
            rusqlite::params![done],
        )
        .unwrap();
        drop(c);

        let (evs, _) = read(&db, "s1", i64::try_from(at.offset()).unwrap()).unwrap();
        assert_eq!(evs.len(), 1, "and now it is there, whole");
        assert_eq!(evs[0].gist, "cargo test");
        assert_eq!(evs[0].outcome.as_deref(), Some("28 passed"));
    }

    #[test]
    fn reading_again_from_the_same_moment_sees_it_again() {
        // The watermark is inclusive on purpose: a part written in the same
        // millisecond as the last one read would otherwise be lost for ever.
        // Re-reading costs nothing, because the archive will not insert twice.
        let db = store(
            "same-ms",
            &[
                ("p1", "m2", 5, 5, r#"{"type":"text","text":"first"}"#),
                ("p2", "m2", 5, 5, r#"{"type":"text","text":"second"}"#),
            ],
        );
        let (evs, at) = read(&db, "s1", 0).unwrap();
        assert_eq!(evs.len(), 2);
        let (again, _) = read(&db, "s1", i64::try_from(at.offset()).unwrap()).unwrap();
        assert_eq!(
            again.len(),
            2,
            "both come back rather than one being skipped"
        );
    }

    #[test]
    fn a_failed_tool_says_so() {
        let bad = r#"{"type":"tool","tool":"bash","state":{"status":"error","input":{"command":"false"},"output":"exit 1"}}"#;
        let db = store("err", &[("p1", "m2", 1, 1, bad)]);
        let (evs, _) = read(&db, "s1", 0).unwrap();
        assert!(evs[0].is_error);
        assert_eq!(evs[0].outcome.as_deref(), Some("exit 1"));
    }

    /// Whatever opencode has actually written on this machine.
    #[test]
    fn a_real_store_still_reads() {
        let Ok(src) = OpencodeSource::new() else {
            return;
        };
        let Ok(stubs) = src.list() else { return };
        for stub in stubs.iter().take(5) {
            let (evs, _) = read(&src.db, &stub.id, 0).unwrap();
            assert!(
                evs.iter().all(|e| !e.gist.is_empty()),
                "{} produced a message with nothing to show",
                stub.id
            );
            assert!(!src.describe(stub).title.is_empty());
        }
    }
}
