//! Durable storage: an archive of events, and a cache of condensations.
//!
//! The transcript files are the source of truth while they exist, but they get
//! rotated and deleted. Everything we parse lands here so the feed outlives
//! them, and so a gist is paid for exactly once.

use std::collections::HashMap;
use std::path::Path;

use crate::db::{count, from_usize, int, opt_int, opt_text, size, text, Db, Engine, Value};
use crate::source::Cursor;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::event::{Event, GistSource, Kind, Role};

/// Bump when the parser's output changes for input it has already read —
/// new event kinds, different filtering, a different gist rule. Sessions
/// parsed by an older version are re-read from the start.
const PARSER_VERSION: i64 = 3;

/// The shape of the database itself, kept in `pragma user_version`.
///
/// An archive from a newer moan is refused rather than misread: opening it
/// with older code would write rows the newer code cannot make sense of, and
/// the damage would not show until much later.
const SCHEMA_VERSION: i64 = 1;

/// How long a claim on a message is believed before another moan may take it.
/// Comfortably longer than a condensation, short enough that a killed instance
/// does not hold anything up for long.
const STALE: i64 = 90;

/// What a shareable archive turned out to contain.
#[derive(Debug, Clone, Copy, Default)]
pub struct Shared {
    pub sessions: usize,
    pub events: usize,
    pub gists: usize,
    /// How much original text is in it. Zero unless it was asked for.
    pub raw_bytes: u64,
    pub bytes: u64,
}

/// A message that matched a search.
#[derive(Debug, Clone)]
pub struct Hit {
    pub session: String,
    pub seq: usize,
    pub ts: String,
    pub kind: String,
    pub gist: String,
}

pub struct Store {
    db: Db,
}

const SCHEMA: &str = "
pragma synchronous = normal;

create table if not exists sessions (
    id        text primary key,
    source    text not null,
    title     text not null,
    path      text not null,
    cursor    text not null default '',
    first_ts  text,
    last_ts   text,
    parser    integer not null default 0,
    seen      integer not null default 0
);

create table if not exists events (
    session   text not null,
    seq       integer not null,
    uid       text not null,
    ts        text not null,
    role      integer not null,
    kind      text not null,
    tool      text,
    raw       text not null,
    gist      text not null,
    gist_src  integer not null,
    outcome   text,
    is_error  integer not null default 0,
    tokens    integer not null default 0,
    primary key (session, seq)
);

create unique index if not exists events_uid on events (session, uid);

create table if not exists scan (
    path     text primary key,
    mtime    text not null,
    bytes    integer not null,
    title    text not null,
    branch   text,
    project  text not null,
    speaker  integer
);

-- Where the reader was. A position, not a preference, which is why it lives
-- beside the archive rather than in the config.
create table if not exists place (
    id      integer primary key check (id = 1),
    session text,
    message integer not null default 0,
    panel   integer not null default 0,
    mode    text not null default ''
);

create table if not exists claims (
    hash text primary key,
    at   text not null
);

create table if not exists gists (
    hash    text primary key,
    gist    text not null,
    model   text not null,
    created text not null,
    -- What it cost, as the provider counted it. Recorded rather than
    -- estimated, so the spend question has a factual answer.
    tok_in  integer not null default 0,
    tok_out integer not null default 0
);
";

impl Store {
    pub fn open(path: &Path, engine: Engine) -> Result<Self> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        strand(path);
        let db = Db::open(path, engine)?;
        Self::prepare(db, Some(path))
    }

    #[cfg(test)]
    pub fn memory() -> Result<Self> {
        Self::prepare(Db::memory(Engine::Sqlite)?, None)
    }

    #[cfg(test)]
    pub fn memory_on(engine: Engine) -> Result<Self> {
        Self::prepare(Db::memory(engine)?, None)
    }

    fn prepare(db: Db, path: Option<&Path>) -> Result<Self> {
        journal(&db).context("setting journal mode")?;
        db.batch(SCHEMA).context("creating the schema")?;
        migrate(&db).context("migrating")?;
        version(&db, path).context("checking the format version")?;
        Ok(Self { db })
    }

    pub fn engine(&self) -> Engine {
        self.db.engine()
    }

    /// Write out the log and shrink it. A write-ahead log is only bounded by
    /// whoever checkpoints it, and a long merged session had left one larger
    /// than the archive itself.
    pub fn checkpoint(&self) {
        let _ = self.db.execute("pragma wal_checkpoint(truncate)", &[]);
    }

    /// Bytes the archive is actually using.
    ///
    /// Pages in use, not pages allocated. Turso cannot yet return free pages
    /// to the filesystem, so a file that has held a large archive stays large;
    /// counting allocated pages would mean retention pruning for ever, chasing
    /// a number that never falls.
    pub fn size(&self) -> Result<u64> {
        let pages = self.pragma_i64("page_count")?;
        let free = self.pragma_i64("freelist_count").unwrap_or(0);
        let page = self.pragma_i64("page_size")?;
        Ok(count((pages - free).max(0) * page) as u64)
    }

    /// Bytes the file occupies, free space and all.
    pub fn on_disk(&self) -> Result<u64> {
        Ok(count(self.pragma_i64("page_count")? * self.pragma_i64("page_size")?) as u64)
    }

    fn pragma_i64(&self, name: &str) -> Result<i64> {
        Ok(self
            .db
            .one(&format!("pragma {name}"), &[])?
            .map(|r| r.i64(0))
            .unwrap_or(0))
    }

    /// Give the filesystem back what pruning freed.
    ///
    /// Only some engines can. Where one cannot, the space is still reused for
    /// what comes next — the file stops growing rather than shrinking.
    pub fn compact(&self) -> Result<bool> {
        match self.db.execute("vacuum", &[]) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub fn enforce(&mut self, r: &crate::config::RetainConfig) -> Result<usize> {
        struct Keep {
            id: String,
            last: String,
            replaceable: bool,
        }
        let mut rows: Vec<Keep> = self
            .db
            .rows("select id, coalesce(last_ts, ''), path from sessions", &[])?
            .into_iter()
            .map(|r| Keep {
                id: r.text(0),
                last: r.text(1),
                replaceable: Path::new(&r.text(2)).exists(),
            })
            .collect();

        // One order, used by every rule: what we would least like to lose comes
        // first. Irreplaceable before replaceable, then newest first.
        rows.sort_by(|a, b| a.replaceable.cmp(&b.replaceable).then(b.last.cmp(&a.last)));

        let cutoff = (r.days > 0).then(|| {
            (Utc::now() - chrono::Duration::days(from_usize(r.days as usize))).to_rfc3339()
        });

        let mut doomed: Vec<String> = Vec::new();
        let mut keeping: Vec<&Keep> = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let too_many = r.sessions > 0 && i >= r.sessions;
            let too_old = cutoff
                .as_ref()
                .is_some_and(|c| !row.last.is_empty() && &row.last < c);
            if too_many || too_old {
                doomed.push(row.id.clone());
            } else {
                keeping.push(row);
            }
        }
        let ids = doomed.clone();
        self.drop_sessions(&ids)?;

        if r.max_mb > 0 {
            let budget = r.max_mb * 1024 * 1024;
            let order: Vec<String> = keeping.iter().rev().map(|x| x.id.clone()).collect();
            for id in order {
                if self.size()? <= budget {
                    break;
                }
                self.drop_sessions(std::slice::from_ref(&id))?;
                doomed.push(id);
            }
        }
        if !doomed.is_empty() {
            let _ = self.compact();
        }
        Ok(doomed.len())
    }

    fn drop_sessions(&mut self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.db.begin()?;
        for id in ids {
            self.db
                .execute("delete from events where session = ?1", &[text(id)])?;
            self.db
                .execute("delete from sessions where id = ?1", &[text(id)])?;
        }
        self.db.commit()?;
        Ok(())
    }

    // ── sessions ────────────────────────────────────────────────────────

    /// Register a session and return where to resume reading it.
    ///
    /// Rows written by an older parser are thrown away rather than trusted: a
    /// fix to what counts as cruft is worthless if the sessions that already
    /// contain the cruft are never read again.
    pub fn register(&self, id: &str, source: &str, title: &str, path: &str) -> Result<Cursor> {
        self.db.execute(
            "insert into sessions (id, source, title, path, parser) values (?1,?2,?3,?4,?5)
             on conflict(id) do update set title = ?3, path = ?4",
            &[
                text(id),
                text(source),
                text(title),
                text(path),
                int(PARSER_VERSION),
            ],
        )?;
        let Some(r) = self.db.one(
            "select cursor, parser from sessions where id = ?1",
            &[text(id)],
        )?
        else {
            return Ok(Cursor::start());
        };
        if r.i64(1) != PARSER_VERSION {
            self.reset_offset(id)?;
            self.db.execute(
                "update sessions set parser = ?2 where id = ?1",
                &[text(id), int(PARSER_VERSION)],
            )?;
            return Ok(Cursor::start());
        }
        Ok(Cursor(r.text(0)))
    }

    /// Re-read a session from scratch on the next poll.
    pub fn reset_offset(&self, id: &str) -> Result<()> {
        self.db
            .execute("update sessions set cursor = '' where id = ?1", &[text(id)])?;
        self.db
            .execute("delete from events where session = ?1", &[text(id)])?;
        Ok(())
    }

    #[cfg(test)]
    pub fn count(&self, session: &str) -> Result<usize> {
        Ok(self
            .db
            .one(
                "select count(*) from events where session = ?1",
                &[text(session)],
            )?
            .map(|r| count(r.i64(0)))
            .unwrap_or(0))
    }

    // ── events ──────────────────────────────────────────────────────────

    /// Store a batch of events and the offset they were read up to, together.
    ///
    /// Together matters: these used to be two transactions, and a crash
    /// between them left the events stored with the offset still behind. The
    /// next run re-read the same lines and appended them again under fresh
    /// sequence numbers, quietly doubling part of a session.
    pub fn append(&mut self, session: &str, events: &[Event], at: &Cursor) -> Result<()> {
        self.db.begin()?;
        let result = self.append_inner(session, events, at);
        if result.is_err() {
            self.db.rollback();
            return result;
        }
        self.db.commit()?;
        Ok(())
    }

    fn append_inner(&self, session: &str, events: &[Event], at: &Cursor) -> Result<()> {
        if !events.is_empty() {
            // Derived inside the transaction rather than counted beforehand,
            // so it cannot be stale by the time it is used.
            let start = self
                .db
                .one(
                    "select coalesce(max(seq) + 1, 0) from events where session = ?1",
                    &[text(session)],
                )?
                .map(|r| r.i64(0))
                .unwrap_or(0);
            for (i, e) in events.iter().enumerate() {
                let (kind, tool) = split_kind(&e.kind);
                self.db.execute(
                    "insert into events
                     (session, seq, uid, ts, role, kind, tool, raw, gist, gist_src, outcome, is_error, tokens)
                     values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
                     on conflict(session, uid) do nothing",
                    &[
                        text(session),
                        int(start + from_usize(i)),
                        text(&e.uid),
                        text(e.ts.to_rfc3339()),
                        int(role_num(e.role)),
                        text(kind),
                        opt_text(tool),
                        text(&e.raw),
                        text(&e.gist),
                        int(src_num(e.gist_src)),
                        opt_text(e.outcome.clone()),
                        int(i64::from(e.is_error)),
                        int(i64::from(e.tokens)),
                    ],
                )?;
            }
            let first = events.first().map(|e| e.ts.to_rfc3339());
            let last = events.last().map(|e| e.ts.to_rfc3339());
            self.db.execute(
                "update sessions set last_ts = ?2, first_ts = coalesce(first_ts, ?3) where id = ?1",
                &[text(session), opt_text(last), opt_text(first)],
            )?;
        }
        self.db.execute(
            "update sessions set cursor = ?2 where id = ?1",
            &[text(session), text(at.to_string())],
        )?;
        Ok(())
    }

    pub fn load(&self, session: &str) -> Result<Vec<Event>> {
        let rows = self.db.rows(
            "select uid, ts, role, kind, tool, raw, gist, gist_src, outcome, is_error, tokens
             from events where session = ?1 order by seq",
            &[text(session)],
        )?;
        Ok(rows
            .into_iter()
            .map(|r| Event {
                uid: r.text(0),
                session: session.to_string(),
                ts: DateTime::parse_from_rfc3339(&r.text(1))
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                role: num_role(r.i64(2) as i32),
                kind: join_kind(&r.text(3), r.opt_text(4)),
                raw: r.text(5),
                gist: r.text(6),
                gist_src: num_src(r.i64(7) as i32),
                outcome: r.opt_text(8),
                is_error: r.i64(9) != 0,
                tokens: u32::try_from(r.i64(10)).unwrap_or(0),
            })
            .collect())
    }

    /// Persist a gist the model produced, against the event row and the cache.
    pub fn save_gist(
        &self,
        session: &str,
        uid: &str,
        hash: &str,
        gist: &str,
        model: &str,
        used: crate::llm::Usage,
    ) -> Result<()> {
        self.db.execute(
            "insert into gists (hash, gist, model, created, tok_in, tok_out)
             values (?1,?2,?3,?4,?5,?6)
             on conflict(hash) do update set gist = ?2, model = ?3, created = ?4,
                                             tok_in = ?5, tok_out = ?6",
            &[
                text(hash),
                text(gist),
                text(model),
                text(Utc::now().to_rfc3339()),
                int(i64::try_from(used.input).unwrap_or(i64::MAX)),
                int(i64::try_from(used.output).unwrap_or(i64::MAX)),
            ],
        )?;
        self.db.execute(
            "update events set gist = ?3, gist_src = ?4 where session = ?1 and uid = ?2",
            &[
                text(session),
                text(uid),
                text(gist),
                int(src_num(GistSource::Model)),
            ],
        )?;
        self.release(hash)?;
        Ok(())
    }

    // ── what we know about a transcript ─────────────────────────────────

    /// What we already know about a transcript, if it has not moved since.
    pub fn scan_get(
        &self,
        path: &str,
        mtime: DateTime<Utc>,
        bytes: u64,
    ) -> Option<crate::source::Facts> {
        self.db
            .one(
                "select title, branch, project, speaker from scan
                 where path = ?1 and mtime = ?2 and bytes = ?3",
                &[
                    text(path),
                    text(mtime.to_rfc3339()),
                    int(from_usize(bytes as usize)),
                ],
            )
            .ok()
            .flatten()
            .map(|r| crate::source::Facts {
                title: r.text(0),
                branch: r.opt_text(1),
                project: r.text(2),
                assistant_last: r.opt_i64(3).map(|v| v != 0),
            })
    }

    pub fn scan_put(
        &self,
        path: &str,
        mtime: DateTime<Utc>,
        bytes: u64,
        f: &crate::source::Facts,
    ) -> Result<()> {
        self.db.execute(
            "insert into scan (path, mtime, bytes, title, branch, project, speaker)
             values (?1,?2,?3,?4,?5,?6,?7)
             on conflict(path) do update set
               mtime = ?2, bytes = ?3, title = ?4, branch = ?5, project = ?6, speaker = ?7",
            &[
                text(path),
                text(mtime.to_rfc3339()),
                int(from_usize(bytes as usize)),
                text(&f.title),
                opt_text(f.branch.clone()),
                text(&f.project),
                opt_int(f.assistant_last.map(|b| b as i64)),
            ],
        )?;
        Ok(())
    }

    // ── what you have read ──────────────────────────────────────────────

    /// Remember where the reader was, so reopening is returning.
    pub fn put_place(&self, session: &str, message: usize, panel: bool, mode: &str) -> Result<()> {
        self.db.execute(
            "insert into place (id, session, message, panel, mode) values (1, ?1, ?2, ?3, ?4)
             on conflict(id) do update set session = ?1, message = ?2, panel = ?3, mode = ?4",
            &[
                text(session),
                size(message),
                int(i64::from(panel)),
                text(mode),
            ],
        )?;
        Ok(())
    }

    /// `(conversation, message, list was open, which list)`.
    pub fn place(&self) -> Option<(String, usize, bool, String)> {
        let r = self
            .db
            .one(
                "select session, message, panel, mode from place where id = 1",
                &[],
            )
            .ok()
            .flatten()?;
        Some((r.opt_text(0)?, count(r.i64(1)), r.i64(2) != 0, r.text(3)))
    }

    pub fn mark_seen(&self, id: &str, seq: usize) -> Result<()> {
        self.db.execute(
            "update sessions set seen = max(seen, ?2) where id = ?1",
            &[text(id), size(seq)],
        )?;
        Ok(())
    }

    /// What has arrived since, as `(messages, of which alerts)`.
    ///
    /// Zero for a session that has never been opened: nothing of it is stored,
    /// so there is nothing to count. The panel says "unread" for those rather
    /// than inventing a number.
    pub fn unread(&self, id: &str, kinds: &[&str]) -> Result<(usize, usize)> {
        let seen = self
            .db
            .one("select seen from sessions where id = ?1", &[text(id)])?
            .map(|r| r.i64(0))
            .unwrap_or(0);
        let mut args = vec![text(id), int(seen)];
        let holes = numbered(kinds, &mut args, 3);
        let sql = format!(
            "select count(*), sum(case when gist like '%' || char(10) || '!%' then 1 else 0 end)
             from events where session = ?1 and seq >= ?2 and kind in ({holes})"
        );
        let Some(r) = self.db.one(&sql, &args)? else {
            return Ok((0, 0));
        };
        Ok((count(r.i64(0)), count(r.opt_i64(1).unwrap_or(0))))
    }

    /// What summarising has cost so far: `(summaries, tokens in, tokens out)`.
    pub fn spent(&self) -> (usize, u64, u64) {
        self.db
            .one(
                "select count(*), coalesce(sum(tok_in), 0), coalesce(sum(tok_out), 0) from gists",
                &[],
            )
            .ok()
            .flatten()
            .map_or((0, 0, 0), |r| {
                (
                    count(r.i64(0)),
                    u64::try_from(r.i64(1)).unwrap_or(0),
                    u64::try_from(r.i64(2)).unwrap_or(0),
                )
            })
    }

    /// How many messages in each conversation were summarised by a model. A
    /// conversation absent from this map has cost nothing.
    pub fn summarised(&self) -> HashMap<String, usize> {
        self.db
            .rows(
                "select session, count(*) from events where gist_src = 2 group by session",
                &[],
            )
            .unwrap_or_default()
            .into_iter()
            .map(|r| (r.text(0), count(r.i64(1))))
            .collect()
    }

    /// Has this session ever been read?
    pub fn known(&self, id: &str) -> bool {
        self.db
            .one(
                "select seen from sessions where id = ?1 and seen > 0",
                &[text(id)],
            )
            .ok()
            .flatten()
            .is_some()
    }

    // ── searching ───────────────────────────────────────────────────────

    /// Find messages across every stored session.
    ///
    /// A scan, not an index: on a real archive of ten thousand events it is a
    /// few milliseconds, and an index over text this size would cost more room
    /// than it saves time. Worth revisiting when somebody has a million.
    pub fn search(&self, q: &str, kinds: &[&str], limit: usize) -> Result<Vec<Hit>> {
        if q.trim().is_empty() || kinds.is_empty() {
            return Ok(Vec::new());
        }
        let like = format!("%{}%", q.trim().replace('%', "\\%").replace('_', "\\_"));
        let mut args = vec![text(like)];
        let holes = numbered(kinds, &mut args, 2);
        args.push(size(limit));
        let at = args.len();
        let sql = format!(
            "select session, seq, ts, kind, gist from events
             where (gist like ?1 escape '\\' or raw like ?1 escape '\\')
               and kind in ({holes})
             order by ts desc limit ?{at}"
        );
        Ok(self
            .db
            .rows(&sql, &args)?
            .into_iter()
            .map(|r| Hit {
                session: r.text(0),
                seq: count(r.i64(1)),
                ts: r.text(2),
                kind: r.text(3),
                gist: r.text(4),
            })
            .collect())
    }

    /// Take in an archive somebody else's moan shared.
    ///
    /// The other half of syncing, and the half that can be done without a
    /// remote: whatever arrives is merged rather than replacing anything.
    /// Summaries are content-addressed, so two machines that condensed the
    /// same message agree by construction; conversations and messages are
    /// keyed on their own ids, so nothing is duplicated and nothing local is
    /// lost.
    pub fn merge_from(&mut self, other: &Path, engine: Engine) -> Result<Shared> {
        let src = Db::open(other, engine)?;
        let mut took = Shared::default();
        self.db.begin()?;
        let result = self.merge_inner(&src, &mut took);
        if result.is_err() {
            self.db.rollback();
            return result.map(|()| took);
        }
        self.db.commit()?;
        Ok(took)
    }

    fn merge_inner(&self, src: &Db, took: &mut Shared) -> Result<()> {
        for r in src.rows(
            "select id, source, title, path, cursor, first_ts, last_ts, parser, seen from sessions",
            &[],
        )? {
            // Counted only when something new arrives: an upsert that always
            // touches a row would report a change on a merge that changed
            // nothing, which is the one thing the count is for.
            let n = self.db.execute(
                "insert into sessions (id, source, title, path, cursor, first_ts, last_ts, parser, seen)
                 values (?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 on conflict(id) do nothing",
                &[
                    text(r.text(0)), text(r.text(1)), text(r.text(2)), text(r.text(3)),
                    text(r.text(4)), opt_text(r.opt_text(5)), opt_text(r.opt_text(6)),
                    int(r.i64(7)), int(r.i64(8)),
                ],
            )?;
            took.sessions += usize::from(n > 0);
            // Having read further on another machine still counts as read.
            self.db.execute(
                "update sessions set seen = max(seen, ?2) where id = ?1",
                &[text(r.text(0)), int(r.i64(8))],
            )?;
        }
        for r in src.rows(
            "select hash, gist, model, created, tok_in, tok_out from gists",
            &[],
        )? {
            let n = self.db.execute(
                "insert into gists (hash, gist, model, created, tok_in, tok_out)
                 values (?1,?2,?3,?4,?5,?6)
                 on conflict(hash) do nothing",
                &[
                    text(r.text(0)),
                    text(r.text(1)),
                    text(r.text(2)),
                    text(r.text(3)),
                    int(r.i64(4)),
                    int(r.i64(5)),
                ],
            )?;
            took.gists += usize::from(n > 0);
        }
        for r in src.rows(
            "select session, seq, uid, ts, role, kind, tool, raw, gist, gist_src, outcome, is_error, tokens
             from events",
            &[],
        )? {
            // Never overwrite a local message with a hollowed-out one: what
            // arrives may have had its original text left behind on purpose.
            let n = self.db.execute(
                "insert into events
                 (session, seq, uid, ts, role, kind, tool, raw, gist, gist_src, outcome, is_error, tokens)
                 values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
                 on conflict(session, uid) do nothing",
                &[
                    text(r.text(0)), int(r.i64(1)), text(r.text(2)), text(r.text(3)),
                    int(r.i64(4)), text(r.text(5)), opt_text(r.opt_text(6)), text(r.text(7)),
                    text(r.text(8)), int(r.i64(9)), opt_text(r.opt_text(10)),
                    int(r.i64(11)), int(r.i64(12)),
                ],
            )?;
            took.events += usize::from(n > 0);
        }
        Ok(())
    }

    /// Build the archive that is allowed to leave this machine.
    ///
    /// A separate database, because Turso replicates a whole file and there is
    /// no way to hold one column back. Condensations and enough of each
    /// session to attach them to; the original text only when asked for.
    ///
    /// Returns what it wrote, so the caller can say what is about to be sent.
    pub fn export_subset(&self, dest: &Path, raw: bool, engine: Engine) -> Result<Shared> {
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }
        strand(dest);
        let out = Db::open(dest, engine)?;
        out.batch(SCHEMA)?;
        out.execute(&format!("pragma user_version = {SCHEMA_VERSION}"), &[])?;

        let mut counts = Shared::default();
        out.begin()?;
        for r in self.db.rows(
            "select id, source, title, path, cursor, first_ts, last_ts, parser, seen from sessions",
            &[],
        )? {
            out.execute(
                "insert into sessions (id, source, title, path, cursor, first_ts, last_ts, parser, seen)
                 values (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                &[
                    text(r.text(0)), text(r.text(1)), text(r.text(2)), text(r.text(3)),
                    text(r.text(4)), opt_text(r.opt_text(5)), opt_text(r.opt_text(6)),
                    int(r.i64(7)), int(r.i64(8)),
                ],
            )?;
            counts.sessions += 1;
        }
        for r in self.db.rows(
            "select hash, gist, model, created, tok_in, tok_out from gists",
            &[],
        )? {
            out.execute(
                "insert into gists (hash, gist, model, created, tok_in, tok_out)
                 values (?1,?2,?3,?4,?5,?6)",
                &[
                    text(r.text(0)),
                    text(r.text(1)),
                    text(r.text(2)),
                    text(r.text(3)),
                    int(r.i64(4)),
                    int(r.i64(5)),
                ],
            )?;
            counts.gists += 1;
        }
        for r in self.db.rows(
            "select session, seq, uid, ts, role, kind, tool, raw, gist, gist_src, outcome, is_error, tokens
             from events",
            &[],
        )? {
            let body = if raw { r.text(7) } else { String::new() };
            counts.raw_bytes += body.len() as u64;
            out.execute(
                "insert into events
                 (session, seq, uid, ts, role, kind, tool, raw, gist, gist_src, outcome, is_error, tokens)
                 values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                &[
                    text(r.text(0)), int(r.i64(1)), text(r.text(2)), text(r.text(3)),
                    int(r.i64(4)), text(r.text(5)), opt_text(r.opt_text(6)), text(body),
                    text(r.text(8)), int(r.i64(9)), opt_text(r.opt_text(10)),
                    int(r.i64(11)), int(r.i64(12)),
                ],
            )?;
            counts.events += 1;
        }
        out.commit()?;
        out.checkpoint_now();
        counts.bytes = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        Ok(counts)
    }

    // ── sharing the archive ─────────────────────────────────────────────

    /// Try to become the one condensing this message.
    ///
    /// False means another moan is already on it, and its answer will land in
    /// the shared cache shortly. A claim older than `STALE` is treated as
    /// abandoned, so an instance that was killed mid-call blocks nobody.
    pub fn claim(&self, hash: &str) -> Result<bool> {
        let cutoff = (Utc::now() - chrono::Duration::seconds(STALE)).to_rfc3339();
        let n = self.db.execute(
            "insert into claims (hash, at) values (?1, ?2)
             on conflict(hash) do update set at = ?2 where claims.at < ?3",
            &[text(hash), text(Utc::now().to_rfc3339()), text(cutoff)],
        )?;
        Ok(n > 0)
    }

    pub fn release(&self, hash: &str) -> Result<()> {
        self.db
            .execute("delete from claims where hash = ?1", &[text(hash)])?;
        Ok(())
    }

    pub fn cached_gist(&self, hash: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .one("select gist from gists where hash = ?1", &[text(hash)])?
            .map(|r| r.text(0)))
    }

    /// Drop every cached condensation, so the next pass re-runs the model.
    pub fn clear_gists(&self) -> Result<usize> {
        let n = self.db.execute("delete from gists", &[])?;
        self.db.execute(
            "update events set gist_src = ?1 where gist_src = ?2",
            &[
                int(src_num(GistSource::Pending)),
                int(src_num(GistSource::Model)),
            ],
        )?;
        Ok(count(i64::try_from(n).unwrap_or(0)))
    }

    pub fn stats(&self) -> Result<(usize, usize, usize)> {
        let n = |sql: &str| -> usize {
            self.db
                .one(sql, &[])
                .ok()
                .flatten()
                .map(|r| r.i64(0))
                .map_or(0, count)
        };
        Ok((
            n("select count(*) from sessions"),
            n("select count(*) from events"),
            n("select count(*) from gists"),
        ))
    }
}

/// Bind a list of values and give back the `?n` placeholders naming them.
///
/// Every placeholder is numbered: mixing `?1` with a bare `?` leaves the engine
/// to decide what the bare ones mean, and it does not decide what you expect.
fn numbered(items: &[&str], args: &mut Vec<Value>, from: usize) -> String {
    let mut holes = Vec::with_capacity(items.len());
    for (i, k) in items.iter().enumerate() {
        args.push(text(*k));
        holes.push(format!("?{}", from + i));
    }
    holes.join(",")
}

/// Put the database in write-ahead mode, once.
///
/// Changing the journal mode needs exclusive access, and unlike an ordinary
/// write it will not wait for it. With several moans starting together they
/// would all try at once and all but one would fail. The setting lives in the
/// file, so only whoever gets there first has to do it.
fn journal(db: &Db) -> Result<()> {
    let mode = db
        .one("pragma journal_mode", &[])?
        .map(|r| r.text(0))
        .unwrap_or_default();
    if mode.eq_ignore_ascii_case("wal") {
        return Ok(());
    }
    // Somebody else doing it this instant is just as good.
    let _ = db.execute("pragma journal_mode = wal", &[]);
    Ok(())
}

/// Clear a `-wal`/`-shm` pair left behind by a database that is gone.
///
/// Deleting `moan.db` and leaving its sidecars is an easy thing to do while
/// tidying a directory, and the engine then fails every query with a disk I/O
/// error rather than saying what is wrong. They describe a file that no longer
/// exists, so there is nothing in them worth keeping.
fn strand(path: &Path) {
    if path.exists() {
        return;
    }
    for ext in ["-wal", "-shm"] {
        let mut p = path.as_os_str().to_os_string();
        p.push(ext);
        let _ = std::fs::remove_file(std::path::PathBuf::from(p));
    }
}

/// Refuse an archive from a newer moan; stamp one that predates versioning.
fn version(db: &Db, path: Option<&Path>) -> Result<()> {
    let found = db
        .one("pragma user_version", &[])?
        .map(|r| r.i64(0))
        .unwrap_or(0);
    if found > SCHEMA_VERSION {
        anyhow::bail!(
            "{} was written by a newer moan (format {found}, this build reads {SCHEMA_VERSION}) — upgrade moan, or point --data elsewhere",
            path.map(|p| p.display().to_string()).unwrap_or_default()
        );
    }
    if found < SCHEMA_VERSION {
        db.execute(&format!("pragma user_version = {SCHEMA_VERSION}"), &[])?;
    }
    Ok(())
}

/// Bring a database written by an older moan up to the current shape.
///
/// `create table if not exists` does nothing for a table that already exists,
/// so a column added later has to be added by hand. Adding one that is already
/// there is the normal case, not an error worth reporting.
fn migrate(db: &Db) -> Result<()> {
    for stmt in [
        "alter table sessions add column parser integer not null default 0",
        "alter table sessions add column seen integer not null default 0",
        // The read position used to be a byte offset. It is now whatever the
        // source says it is, so an archive written before this carries its
        // number across as the text of a number.
        "alter table sessions add column cursor text not null default ''",
        "alter table place add column mode text not null default ''",
        "alter table gists add column tok_in integer not null default 0",
        "alter table gists add column tok_out integer not null default 0",
        "update sessions set cursor = cast(offset as text) where cursor = '' and offset > 0",
        // Never read: nothing filters or orders by ts, and it cost 200KB on a
        // single-project archive.
        "drop index if exists events_ts",
    ] {
        let _ = db.execute(stmt, &[]);
    }
    Ok(())
}

// ── enum <-> column mapping ─────────────────────────────────────────────

fn role_num(r: Role) -> i64 {
    match r {
        Role::User => 0,
        Role::Assistant => 1,
        Role::Tool => 2,
        Role::System => 3,
    }
}

fn num_role(n: i32) -> Role {
    match n {
        0 => Role::User,
        1 => Role::Assistant,
        2 => Role::Tool,
        _ => Role::System,
    }
}

fn src_num(s: GistSource) -> i64 {
    match s {
        GistSource::Verbatim => 0,
        GistSource::Rule => 1,
        GistSource::Model => 2,
        // Being worked on is a fact about this run, not about the archive.
        GistSource::Pending | GistSource::Working | GistSource::Failed => 3,
        // Worth remembering: it should not be asked for again on reopening.
        GistSource::Withheld => 4,
    }
}

fn num_src(n: i32) -> GistSource {
    match n {
        0 => GistSource::Verbatim,
        1 => GistSource::Rule,
        2 => GistSource::Model,
        4 => GistSource::Withheld,
        _ => GistSource::Pending,
    }
}

fn split_kind(k: &Kind) -> (&'static str, Option<String>) {
    match k {
        Kind::Prompt => ("prompt", None),
        Kind::Say => ("say", None),
        Kind::Think => ("think", None),
        Kind::Tool { name } => ("tool", Some(name.clone())),
        Kind::Fail => ("fail", None),
        Kind::Note => ("note", None),
    }
}

fn join_kind(k: &str, tool: Option<String>) -> Kind {
    match k {
        "prompt" => Kind::Prompt,
        "say" => Kind::Say,
        "think" => Kind::Think,
        "tool" => Kind::Tool {
            name: tool.unwrap_or_else(|| "tool".into()),
        },
        "fail" => Kind::Fail,
        _ => Kind::Note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every engine moan can keep an archive in. A port that only passes on
    /// one of them is not a port.
    fn both(f: impl Fn(&mut Store, Engine)) {
        for e in [Engine::Sqlite, Engine::Turso] {
            let mut s = Store::memory_on(e).unwrap();
            f(&mut s, e);
        }
    }
    use crate::event::Kind;

    fn ev(uid: &str, kind: Kind, raw: &str) -> Event {
        Event {
            uid: uid.into(),
            session: "s".into(),
            ts: Utc::now(),
            role: Role::Assistant,
            kind,
            raw: raw.into(),
            gist: raw.into(),
            gist_src: GistSource::Pending,
            outcome: None,
            is_error: false,
            tokens: 7,
        }
    }

    #[test]
    fn round_trips_events() {
        both(|s, _e| {
            s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
            let e = ev(
                "a",
                Kind::Tool {
                    name: "Bash".into(),
                },
                "ls",
            );
            s.append("s", std::slice::from_ref(&e), &Cursor::at(10))
                .unwrap();
            let back = s.load("s").unwrap();
            assert_eq!(back.len(), 1);
            assert_eq!(
                back[0].kind,
                Kind::Tool {
                    name: "Bash".into()
                }
            );
            assert_eq!(back[0].tokens, 7);
        });
    }

    #[test]
    fn appending_twice_does_not_overwrite() {
        both(|s, _e| {
            s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
            s.append("s", &[ev("a", Kind::Say, "one")], &Cursor::at(10))
                .unwrap();
            s.append("s", &[ev("b", Kind::Say, "two")], &Cursor::at(20))
                .unwrap();
            let back = s.load("s").unwrap();
            assert_eq!(back.len(), 2);
            assert_eq!(back[1].raw, "two");
        });
    }

    #[test]
    fn a_crash_between_writing_and_recording_cannot_double_up() {
        both(|s, _e| {
            // These were two transactions. A crash between them stored the events
            // with the offset still behind, and the next run appended them again
            // under fresh sequence numbers.

            s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
            let batch = [ev("a", Kind::Say, "one"), ev("b", Kind::Say, "two")];
            s.append("s", &batch, &Cursor::at(100)).unwrap();
            // Re-reading the same lines, as a resumed run would.
            s.append("s", &batch, &Cursor::at(100)).unwrap();
            assert_eq!(s.count("s").unwrap(), 2, "a re-read is a no-op");
            assert_eq!(s.load("s").unwrap().len(), 2);
        });
    }

    #[test]
    fn a_gist_is_found_by_id_not_by_walking_the_session() {
        let mut s = Store::memory().unwrap();
        s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
        let many: Vec<Event> = (0..200)
            .map(|i| ev(&format!("u{i}"), Kind::Say, "x"))
            .collect();
        s.append("s", &many, &Cursor::at(1)).unwrap();

        let plan: String =
            s.db.one(
                "explain query plan update events set gist='g' where session='s' and uid='u7'",
                &[],
            )
            .unwrap()
            .map(|r| r.text(3))
            .unwrap_or_default();
        assert!(
            plan.contains("events_uid"),
            "the write must use the index, not walk the session: {plan}"
        );
    }

    #[test]
    fn an_archive_from_a_newer_moan_is_refused() {
        let dir = std::env::temp_dir().join("moan-version-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("moan.db");
        {
            let s = Store::open(&path, Engine::Sqlite).unwrap();
            s.db.execute(
                &format!("pragma user_version = {}", SCHEMA_VERSION + 1),
                &[],
            )
            .unwrap();
        }
        let err = match Store::open(&path, Engine::Sqlite) {
            Ok(_) => panic!("a newer archive must not open"),
            // The whole chain: the reason is behind the context it was given.
            Err(e) => format!("{e:#}"),
        };
        assert!(err.contains("newer moan"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidecars_left_behind_by_a_deleted_database_do_not_stop_startup() {
        // Deleting moan.db while tidying and leaving its -wal and -shm made
        // every query fail with a disk I/O error and no way to guess why.
        let dir = std::env::temp_dir().join("moan-strand-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("moan.db");
        {
            let mut s = Store::open(&path, Engine::Sqlite).unwrap();
            s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
            s.append("s", &[ev("a", Kind::Say, "one")], &Cursor::at(1))
                .unwrap();
        }
        std::fs::remove_file(&path).unwrap();
        // A clean close removes them, so recreate what `rm moan.db` leaves:
        // sidecars describing a database that is no longer there.
        std::fs::write(dir.join("moan.db-wal"), b"not a real write-ahead log").unwrap();
        std::fs::write(dir.join("moan.db-shm"), b"nor a real shared-memory file").unwrap();
        let s = Store::open(&path, Engine::Sqlite).expect("opens despite the orphans");
        assert_eq!(s.stats().unwrap().1, 0, "and starts empty");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn dated(s: &mut Store, id: &str, day: &str, path: &str) {
        s.register(id, "claude", "~/x", path).unwrap();
        let mut e = ev("a", Kind::Say, "x");
        e.ts = DateTime::parse_from_rfc3339(&format!("{day}T00:00:00Z"))
            .unwrap()
            .with_timezone(&Utc);
        s.append(id, &[e], &Cursor::at(1)).unwrap();
    }

    #[test]
    fn retention_keeps_the_newest_and_drops_the_rest() {
        both(|s, _e| {
            for (i, day) in ["2026-09-01", "2026-09-02", "2026-09-03"]
                .iter()
                .enumerate()
            {
                dated(s, &format!("s{i}"), day, "/gone.jsonl");
            }
            let r = crate::config::RetainConfig {
                max_mb: 0,
                days: 0,
                sessions: 2,
            };
            assert_eq!(s.enforce(&r).unwrap(), 1);
            assert_eq!(s.count("s0").unwrap(), 0, "the oldest went");
            assert_eq!(s.count("s2").unwrap(), 1, "the newest stayed");
        });
    }

    #[test]
    fn a_session_whose_transcript_is_gone_outlives_one_that_can_be_read_again() {
        // Events are only worth keeping when nothing else has them. A session
        // whose transcript is still on disk costs nothing to rebuild.
        let mut s = Store::memory().unwrap();
        let here = std::env::temp_dir().join("moan-retain-live.jsonl");
        std::fs::write(&here, b"{}").unwrap();

        // The replaceable one is *newer*, and should still go first.
        dated(&mut s, "gone", "2026-09-01", "/no/such/transcript.jsonl");
        dated(&mut s, "live", "2026-09-09", here.to_str().unwrap());

        let r = crate::config::RetainConfig {
            max_mb: 0,
            days: 0,
            sessions: 1,
        };
        s.enforce(&r).unwrap();
        assert_eq!(
            s.count("live").unwrap(),
            0,
            "it can be read again, so it goes"
        );
        assert_eq!(s.count("gone").unwrap(), 1, "this is the only copy");
        let _ = std::fs::remove_file(&here);
    }

    #[test]
    fn age_drops_what_is_stale() {
        both(|s, _e| {
            let old = (Utc::now() - chrono::Duration::days(400))
                .format("%Y-%m-%d")
                .to_string();
            let new = Utc::now().format("%Y-%m-%d").to_string();
            dated(s, "old", &old, "/gone.jsonl");
            dated(s, "new", &new, "/gone.jsonl");
            let r = crate::config::RetainConfig {
                max_mb: 0,
                days: 30,
                sessions: 0,
            };
            assert_eq!(s.enforce(&r).unwrap(), 1);
            assert_eq!(s.count("old").unwrap(), 0);
            assert_eq!(s.count("new").unwrap(), 1);
        });
    }

    #[test]
    fn condensations_survive_any_amount_of_pruning() {
        let mut s = Store::memory().unwrap();
        dated(&mut s, "s", "2026-09-01", "/gone.jsonl");
        s.save_gist(
            "s",
            "a",
            "hash",
            "the expensive part",
            "test",
            crate::llm::Usage::default(),
        )
        .unwrap();
        let r = crate::config::RetainConfig {
            max_mb: 0,
            days: 0,
            sessions: 0,
        };
        // Everything goes.
        s.enforce(&crate::config::RetainConfig { sessions: 0, ..r })
            .unwrap();
        s.drop_sessions(&["s".to_string()]).unwrap();
        assert_eq!(
            s.cached_gist("hash").unwrap().as_deref(),
            Some("the expensive part"),
            "what was paid for is never thrown away"
        );
    }

    #[test]
    fn several_moans_can_share_one_archive() {
        let dir = std::env::temp_dir().join("moan-share-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("moan.db");

        let mut a = Store::open(&path, Engine::Sqlite).unwrap();
        let mut b = Store::open(&path, Engine::Sqlite).expect("a second instance opens");
        let c = Store::open(&path, Engine::Sqlite).expect("and a third");

        a.register("s", "claude", "~/x", "/x.jsonl").unwrap();
        b.register("s", "claude", "~/x", "/x.jsonl").unwrap();
        a.append("s", &[ev("a", Kind::Say, "one")], &Cursor::at(10))
            .unwrap();
        b.append("s", &[ev("b", Kind::Say, "two")], &Cursor::at(20))
            .unwrap();
        assert_eq!(
            c.load("s").unwrap().len(),
            2,
            "both writers are visible to a reader"
        );

        // And they do not both pay for the same message.
        assert!(a.claim("h").unwrap());
        assert!(!b.claim("h").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_moans_do_not_both_pay_for_one_message() {
        both(|s, _e| {
            assert!(s.claim("h").unwrap(), "the first takes it");
            assert!(!s.claim("h").unwrap(), "the second leaves it alone");

            // Whoever finishes releases it.
            s.save_gist(
                "s",
                "u",
                "h",
                "condensed",
                "test",
                crate::llm::Usage::default(),
            )
            .unwrap();
            assert!(s.claim("h").unwrap(), "and it is free again");
        });
    }

    #[test]
    fn an_abandoned_claim_does_not_hold_anything_up() {
        let s = Store::memory().unwrap();
        assert!(s.claim("h").unwrap());
        // What a killed instance leaves behind.
        let old = (Utc::now() - chrono::Duration::seconds(STALE + 10)).to_rfc3339();
        s.db.execute("update claims set at = ?1 where hash = 'h'", &[text(old)])
            .unwrap();
        assert!(s.claim("h").unwrap(), "a stale claim is taken over");
    }

    #[test]
    fn search_finds_messages_and_respects_what_the_feed_shows() {
        both(|s, _e| {
            s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
            let mut prose = ev("a", Kind::Say, "the release is ready");
            prose.gist = "release ready".into();
            let mut cmd = ev(
                "b",
                Kind::Tool {
                    name: "Bash".into(),
                },
                "gh release create",
            );
            cmd.gist = "gh release create".into();
            s.append("s", &[prose, cmd], &Cursor::at(1)).unwrap();

            let chat = s.search("release", &["prompt", "say", "note"], 50).unwrap();
            assert_eq!(chat.len(), 1, "a shell command is not a conversation");
            assert_eq!(chat[0].gist, "release ready");

            let all = s
                .search("release", &["prompt", "say", "note", "tool"], 50)
                .unwrap();
            assert_eq!(all.len(), 2, "unless you asked to see the tools");

            assert!(s.search("", &["say"], 50).unwrap().is_empty());
            assert!(s
                .search("nothing like this", &["say"], 50)
                .unwrap()
                .is_empty());
        });
    }

    #[test]
    fn a_wildcard_in_the_query_is_not_a_wildcard() {
        both(|s, _e| {
            s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
            let mut e = ev("a", Kind::Say, "literally 50% done");
            e.gist = "50% done".into();
            s.append("s", &[e], &Cursor::at(1)).unwrap();
            assert_eq!(s.search("50%", &["say"], 50).unwrap().len(), 1);
            assert!(
                s.search("%%%%", &["say"], 50).unwrap().is_empty(),
                "percent signs are text, not a pattern that matches everything"
            );
        });
    }

    #[test]
    fn unread_counts_what_arrived_after_you_looked() {
        both(|s, _e| {
            s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
            assert!(!s.known("s"), "never opened is not the same as nothing new");

            let mut alert = ev("b", Kind::Say, "and this");
            alert.gist = "headline\n! something went wrong".into();
            s.append("s", &[ev("a", Kind::Say, "one"), alert], &Cursor::at(1))
                .unwrap();

            let kinds = ["prompt", "say", "note"];
            assert_eq!(
                s.unread("s", &kinds).unwrap(),
                (2, 1),
                "both, one of them an alert"
            );

            s.mark_seen("s", 2).unwrap();
            assert!(s.known("s"));
            assert_eq!(s.unread("s", &kinds).unwrap(), (0, 0), "read to the end");

            s.append("s", &[ev("c", Kind::Say, "later")], &Cursor::at(2))
                .unwrap();
            assert_eq!(s.unread("s", &kinds).unwrap(), (1, 0));

            // The watermark never goes backwards.
            s.mark_seen("s", 1).unwrap();
            assert_eq!(s.unread("s", &kinds).unwrap(), (1, 0));
        });
    }

    #[test]
    fn what_leaves_the_machine_holds_no_commands_by_default() {
        let dir = std::env::temp_dir().join("moan-subset-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut s = Store::memory_on(Engine::Sqlite).unwrap();
        s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
        let mut cmd = ev(
            "a",
            Kind::Tool {
                name: "Bash".into(),
            },
            "grep -o 'sk-or-v1-[A-Za-z0-9]*' ~/.config/keys",
        );
        cmd.gist = "looked for the key".into();
        s.append("s", &[cmd], &Cursor::at(1)).unwrap();
        s.save_gist(
            "s",
            "a",
            "h",
            "looked for the key",
            "test",
            crate::llm::Usage::default(),
        )
        .unwrap();

        // The default: condensations travel, commands do not.
        let quiet = dir.join("quiet.db");
        let held = s.export_subset(&quiet, false, Engine::Sqlite).unwrap();
        assert_eq!(held.sessions, 1);
        assert_eq!(held.events, 1);
        assert_eq!(held.gists, 1);
        assert_eq!(held.raw_bytes, 0, "nothing of the original text");

        let out = Store::open(&quiet, Engine::Sqlite).unwrap();
        let back = out.load("s").unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].gist, "looked for the key", "still readable");
        assert!(back[0].raw.is_empty(), "the command did not travel");
        assert!(
            !std::fs::read(&quiet)
                .unwrap()
                .windows(9)
                .any(|w| w == b"sk-or-v1-"),
            "no trace of it anywhere in the file"
        );
        assert_eq!(
            out.cached_gist("h").unwrap().as_deref(),
            Some("looked for the key")
        );

        // And when somebody asks for everything, they get everything.
        let full = dir.join("full.db");
        let held = s.export_subset(&full, true, Engine::Sqlite).unwrap();
        assert!(held.raw_bytes > 0);
        let out = Store::open(&full, Engine::Sqlite).unwrap();
        assert!(out.load("s").unwrap()[0].raw.contains("grep"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_export_leaves_the_real_archive_alone() {
        // Whatever happens to the copy, the archive it came from is untouched:
        // it is only ever read.
        let dir = std::env::temp_dir().join("moan-subset-crash");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Store::memory_on(Engine::Sqlite).unwrap();
        s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
        s.append("s", &[ev("a", Kind::Say, "one")], &Cursor::at(1))
            .unwrap();

        let dest = dir.join("shared.db");
        s.export_subset(&dest, false, Engine::Sqlite).unwrap();
        // Truncate it, as a process killed mid-push would.
        std::fs::write(&dest, b"not a database any more").unwrap();
        // Building it again recovers; the source never changed.
        let held = s.export_subset(&dest, false, Engine::Sqlite).unwrap();
        assert_eq!(held.events, 1);
        assert_eq!(s.count("s").unwrap(), 1, "the archive is only ever read");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Numbers to read. `scripts/task bench`.
    #[test]
    #[ignore]
    fn report_store_cost() {
        for engine in [Engine::Sqlite, Engine::Turso] {
            let mut s = Store::memory_on(engine).unwrap();
            s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
            let batch: Vec<Event> = (0..5_000)
                .map(|i| {
                    let mut e = ev(&format!("u{i}"), Kind::Say, "a message of some length");
                    e.gist = format!("gist number {i}");
                    e
                })
                .collect();
            let t = std::time::Instant::now();
            s.append("s", &batch, &Cursor::at(1)).unwrap();
            let write = t.elapsed();

            for i in 0..500 {
                s.save_gist(
                    "s",
                    &format!("u{i}"),
                    &format!("h{i}"),
                    "x",
                    "t",
                    crate::llm::Usage::default(),
                )
                .unwrap();
            }
            let t = std::time::Instant::now();
            for i in 0..500 {
                let _ = s.cached_gist(&format!("h{i}")).unwrap();
            }
            let read = t.elapsed().as_secs_f64() * 1e6 / 500.0;

            let t = std::time::Instant::now();
            let _ = s.load("s").unwrap();
            println!(
                "\n  {engine:?}\n    append 5000     {:6.0} ms\n    cached_gist     {read:6.1} µs\n    load 5000       {:6.0} ms",
                write.as_secs_f64() * 1000.0,
                t.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    #[test]
    fn where_you_were_survives_closing() {
        both(|s, _e| {
            assert!(s.place().is_none(), "nowhere to return to yet");
            s.put_place("conv", 42, true, "search").unwrap();
            assert_eq!(s.place(), Some(("conv".into(), 42, true, "search".into())));
            // Moving is remembered, not appended to.
            s.put_place("other", 7, false, "").unwrap();
            assert_eq!(s.place(), Some(("other".into(), 7, false, String::new())));
        });
    }

    #[test]
    fn taking_in_another_machines_archive_loses_nothing() {
        let dir = std::env::temp_dir().join("moan-merge-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // What the other machine shares: a conversation of its own, and a
        // summary of a message both machines have.
        let mut them = Store::memory_on(Engine::Sqlite).unwrap();
        them.register("theirs", "claude", "~/t", "/t.jsonl")
            .unwrap();
        them.append(
            "theirs",
            &[ev("t1", Kind::Say, "over there")],
            &Cursor::at(1),
        )
        .unwrap();
        them.save_gist(
            "theirs",
            "t1",
            "shared",
            "we both saw this",
            "test",
            crate::llm::Usage::default(),
        )
        .unwrap();
        let shared = dir.join("shared.db");
        them.export_subset(&shared, false, Engine::Sqlite).unwrap();

        // What this machine has: its own conversation, with its own text.
        let mut mine = Store::memory_on(Engine::Sqlite).unwrap();
        mine.register("mine", "claude", "~/m", "/m.jsonl").unwrap();
        mine.append("mine", &[ev("m1", Kind::Say, "over here")], &Cursor::at(1))
            .unwrap();

        let took = mine.merge_from(&shared, Engine::Sqlite).unwrap();
        assert_eq!(took.sessions, 1);
        assert_eq!(took.events, 1);
        assert_eq!(took.gists, 1);

        // Theirs arrived, mine is untouched, and the summary is now here.
        assert_eq!(mine.load("theirs").unwrap().len(), 1);
        assert_eq!(mine.load("mine").unwrap()[0].raw, "over here");
        assert_eq!(
            mine.cached_gist("shared").unwrap().as_deref(),
            Some("we both saw this")
        );

        // Doing it twice changes nothing, and a hollowed-out copy of a message
        // this machine has in full never replaces the full one.
        let again = mine.merge_from(&shared, Engine::Sqlite).unwrap();
        assert_eq!((again.sessions, again.events, again.gists), (0, 0, 0));
        assert_eq!(mine.load("mine").unwrap()[0].raw, "over here");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gist_cache_is_content_addressed() {
        both(|s, _e| {
            let e = ev("a", Kind::Say, "a long message");
            let h = e.content_hash();
            assert!(s.cached_gist(&h).unwrap().is_none());
            s.save_gist("s", "a", &h, "short", "test", crate::llm::Usage::default())
                .unwrap();
            assert_eq!(s.cached_gist(&h).unwrap().as_deref(), Some("short"));
        });
    }

    #[test]
    fn offset_survives_reopen() {
        both(|s, _e| {
            assert_eq!(
                s.register("s", "claude", "~/x", "/x.jsonl").unwrap(),
                Cursor::start()
            );
            s.append("s", &[], &Cursor::at(4096)).unwrap();
            assert_eq!(
                s.register("s", "claude", "~/x", "/x.jsonl").unwrap(),
                Cursor::at(4096)
            );
        });
    }

    #[test]
    fn a_database_without_the_parser_column_still_opens() {
        // What every database written before this change looks like.
        let db = Db::memory(Engine::Sqlite).unwrap();
        db.batch(
            "create table sessions (id text primary key, source text not null,
             title text not null, path text not null, offset integer not null default 0,
             first_ts text, last_ts text);
             insert into sessions (id, source, title, path, offset)
             values ('s', 'claude', '~/x', '/x.jsonl', 4096);",
        )
        .unwrap();
        // Everything else about the old schema is unchanged.
        db.batch(SCHEMA).unwrap();
        migrate(&db).unwrap();
        let s = Store { db };
        // The rows predate the current parser, so they are re-read.
        assert_eq!(
            s.register("s", "claude", "~/x", "/x.jsonl").unwrap(),
            Cursor::start()
        );
    }

    #[test]
    fn an_older_parsers_rows_are_thrown_away() {
        both(|s, _e| {
            s.register("s", "claude", "~/x", "/x.jsonl").unwrap();
            s.append(
                "s",
                &[ev("a", Kind::Say, "parsed by yesterday")],
                &Cursor::at(4096),
            )
            .unwrap();
            s.db.execute("update sessions set parser = 0 where id = 's'", &[])
                .unwrap();

            // Re-registering under the current parser rewinds and clears.
            assert_eq!(
                s.register("s", "claude", "~/x", "/x.jsonl").unwrap(),
                Cursor::start()
            );
            assert_eq!(s.count("s").unwrap(), 0);
            // And it does not rewind a second time.
            s.append("s", &[], &Cursor::at(512)).unwrap();
            assert_eq!(
                s.register("s", "claude", "~/x", "/x.jsonl").unwrap(),
                Cursor::at(512)
            );
        });
    }

    /// The promise the list's ≈ mark makes: a conversation nobody read has
    /// cost nothing. Listing and ingesting must never buy a summary.
    #[test]
    fn unread_conversations_cost_nothing() {
        let s = &mut Store::memory_on(Engine::Sqlite).unwrap();
        let of = |sess: &str, uid: &str| Event {
            session: sess.into(),
            ..ev(uid, Kind::Say, "some prose worth condensing")
        };
        s.register("a", "claude", "~/a", "/a.jsonl").unwrap();
        s.register("b", "claude", "~/b", "/b.jsonl").unwrap();
        s.append("a", &[of("a", "a0"), of("a", "a1")], &Cursor::at(2))
            .unwrap();
        s.append("b", &[of("b", "b0")], &Cursor::at(1)).unwrap();

        assert_eq!(s.spent(), (0, 0, 0), "ingesting buys nothing");
        assert!(s.summarised().is_empty(), "nothing is marked as paid for");

        s.save_gist(
            "a",
            "a0",
            "hash",
            "a gist",
            "some/model",
            crate::llm::Usage {
                input: 40,
                output: 9,
            },
        )
        .unwrap();

        assert_eq!(s.spent(), (1, 40, 9), "and what is bought is counted");
        let paid = s.summarised();
        assert_eq!(paid.get("a"), Some(&1));
        assert_eq!(paid.get("b"), None, "the conversation nobody read is free");
    }
}
