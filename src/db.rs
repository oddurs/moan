//! The database, behind one synchronous surface.
//!
//! moan can keep its archive in SQLite or in Turso. Turso is SQLite-compatible
//! and pure Rust — no C toolchain — and it can sync a database between
//! machines, which is the whole reason for the change. It is also pre-release,
//! and this is the component holding durable data, so both remain available
//! and the SQL above this layer is written once.
//!
//! Turso's API is asynchronous and the draw loop is not. Measured over five
//! hundred reads of the shape the feed actually performs: awaiting directly
//! cost 6.5µs, driving the future with `block_in_place` cost 6.1µs, and
//! handing the work to a store thread cost 13.8µs. Blocking in place is
//! therefore free, and it keeps `.await` out of code that is really just
//! arithmetic.

use anyhow::{Context, Result};
use std::path::Path;

pub use turso::Value;

/// Which engine holds the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// The default, because it is the only one that currently does everything
    /// moan asks of a database: several instances sharing one archive, and
    /// giving disk space back when history is pruned.
    #[default]
    Sqlite,
    /// Pure Rust, no C toolchain, and the path to syncing between machines.
    /// Two things it cannot do yet are in `docs/0001-turso.md`.
    Turso,
}

/// Where the futures get driven.
///
/// Borrowed inside moan, which is already running a runtime; owned in a test,
/// which is not. `block_in_place` is only legal inside a runtime, and
/// `block_on` is only legal outside one.
enum Rt {
    Borrowed(tokio::runtime::Handle),
    Owned(tokio::runtime::Runtime),
}

impl Rt {
    fn here() -> Result<Self> {
        Ok(match tokio::runtime::Handle::try_current() {
            Ok(h) => Rt::Borrowed(h),
            Err(_) => Rt::Owned(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()?,
            ),
        })
    }

    fn run<F: std::future::Future>(&self, f: F) -> F::Output {
        match self {
            Rt::Borrowed(h) => tokio::task::block_in_place(|| h.block_on(f)),
            Rt::Owned(r) => r.block_on(f),
        }
    }
}

pub enum Db {
    Sqlite(rusqlite::Connection),
    Turso(Box<Turso>),
}

pub struct Turso {
    conn: turso::Connection,
    rt: Rt,
}

/// One row, already read out of whichever engine produced it.
pub struct Row(Vec<Value>);

impl Row {
    pub fn i64(&self, i: usize) -> i64 {
        match self.0.get(i) {
            Some(Value::Integer(n)) => *n,
            Some(Value::Real(f)) => *f as i64,
            Some(Value::Text(t)) => t.parse().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn text(&self, i: usize) -> String {
        self.opt_text(i).unwrap_or_default()
    }

    pub fn opt_text(&self, i: usize) -> Option<String> {
        match self.0.get(i) {
            Some(Value::Text(t)) => Some(t.clone()),
            Some(Value::Integer(n)) => Some(n.to_string()),
            _ => None,
        }
    }

    pub fn opt_i64(&self, i: usize) -> Option<i64> {
        match self.0.get(i) {
            Some(Value::Integer(n)) => Some(*n),
            _ => None,
        }
    }
}

/// A count or an index that came out of the database.
///
/// Storage speaks `i64` and Rust indexes with `usize`. Every crossing is a
/// place a negative or absurd value could become a very large index, so they
/// all go through here and clamp instead.
pub fn count(n: i64) -> usize {
    usize::try_from(n).unwrap_or(0)
}

/// The same crossing the other way, for a value going into a statement.
pub fn from_usize(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Anything that can be bound to a statement.
pub fn text(s: impl Into<String>) -> Value {
    Value::Text(s.into())
}
pub fn int(n: impl Into<i64>) -> Value {
    Value::Integer(n.into())
}

/// Bind a length or an index.
pub fn size(n: usize) -> Value {
    Value::Integer(from_usize(n))
}
pub fn opt_text(s: Option<impl Into<String>>) -> Value {
    s.map_or(Value::Null, |v| Value::Text(v.into()))
}
pub fn opt_int(n: Option<impl Into<i64>>) -> Value {
    n.map_or(Value::Null, |v| Value::Integer(v.into()))
}

impl Db {
    pub fn open(path: &Path, engine: Engine) -> Result<Self> {
        match engine {
            Engine::Sqlite => {
                let c = rusqlite::Connection::open(path)
                    .with_context(|| format!("opening {}", path.display()))?;
                c.busy_timeout(std::time::Duration::from_secs(5))?;
                Ok(Db::Sqlite(c))
            }
            Engine::Turso => {
                let rt = Rt::here()?;
                let p = path.to_string_lossy().to_string();
                let conn = rt.run(async {
                    // Turso takes an exclusive lock on the file, so a second
                    // moan cannot open an archive a first one is holding.
                    // `experimental_multiprocess_wal` is meant to lift that
                    // and does not yet: with it on, three instances gave one
                    // clean failure, one coordination-file error and one
                    // panic. Left off, so the failure is at least legible.
                    let db = turso::Builder::new_local(&p).build().await?;
                    db.connect()
                })?;
                let me = Db::Turso(Box::new(Turso { conn, rt }));
                me.execute("pragma busy_timeout = 5000", &[])?;
                Ok(me)
            }
        }
    }

    #[cfg(test)]
    pub fn memory(engine: Engine) -> Result<Self> {
        match engine {
            Engine::Sqlite => Ok(Db::Sqlite(rusqlite::Connection::open_in_memory()?)),
            Engine::Turso => {
                let rt = Rt::here()?;
                let conn = rt.run(async {
                    let db = turso::Builder::new_local(":memory:").build().await?;
                    db.connect()
                })?;
                Ok(Db::Turso(Box::new(Turso { conn, rt })))
            }
        }
    }

    pub fn engine(&self) -> Engine {
        match self {
            Db::Sqlite(_) => Engine::Sqlite,
            Db::Turso(_) => Engine::Turso,
        }
    }

    /// Run a statement; returns how many rows it changed.
    pub fn execute(&self, sql: &str, args: &[Value]) -> Result<u64> {
        match self {
            Db::Sqlite(c) => {
                let p = sqlite_params(args);
                let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
                Ok(c.execute(sql, refs.as_slice())? as u64)
            }
            Db::Turso(t) => {
                let a = args.to_vec();
                Ok(t.rt.run(t.conn.execute(sql, a))?)
            }
        }
    }

    /// Several statements, for schema work. No parameters.
    pub fn batch(&self, sql: &str) -> Result<()> {
        match self {
            Db::Sqlite(c) => Ok(c.execute_batch(sql)?),
            Db::Turso(t) => {
                for stmt in sql.split(';') {
                    let s = strip_comments(stmt);
                    if s.trim().is_empty() {
                        continue;
                    }
                    t.rt.run(t.conn.execute(&s, ()))?;
                }
                Ok(())
            }
        }
    }

    pub fn rows(&self, sql: &str, args: &[Value]) -> Result<Vec<Row>> {
        match self {
            Db::Sqlite(c) => {
                let p = sqlite_params(args);
                let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
                let mut st = c.prepare(sql)?;
                let n = st.column_count();
                let out = st
                    .query_map(refs.as_slice(), |r| {
                        let mut v = Vec::with_capacity(n);
                        for i in 0..n {
                            v.push(from_sqlite(r.get_ref(i)?));
                        }
                        Ok(Row(v))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(out)
            }
            Db::Turso(t) => {
                let a = args.to_vec();
                t.rt.run(async {
                    let mut rows = t.conn.query(sql, a).await?;
                    let mut out = Vec::new();
                    while let Some(r) = rows.next().await? {
                        let mut v = Vec::new();
                        let mut i = 0;
                        while let Ok(x) = r.get_value(i) {
                            v.push(x);
                            i += 1;
                        }
                        out.push(Row(v));
                    }
                    Ok(out)
                })
            }
        }
    }

    pub fn one(&self, sql: &str, args: &[Value]) -> Result<Option<Row>> {
        Ok(self.rows(sql, args)?.into_iter().next())
    }

    /// Flush the write-ahead log into the file itself.
    pub fn checkpoint_now(&self) {
        let _ = self.execute("pragma wal_checkpoint(truncate)", &[]);
    }

    /// A transaction that intends to write, taking the lock up front.
    ///
    /// The default is deferred: it opens as a reader and upgrades on the first
    /// write, and an engine refuses that upgrade immediately rather than
    /// waiting — which is how a second moan used to die with a locked-database
    /// error while a busy timeout sat unused.
    pub fn begin(&self) -> Result<()> {
        self.execute("begin immediate", &[])?;
        Ok(())
    }

    pub fn commit(&self) -> Result<()> {
        self.execute("commit", &[])?;
        Ok(())
    }

    pub fn rollback(&self) {
        let _ = self.execute("rollback", &[]);
    }
}

/// Turso parses one statement at a time and has no comment stripper.
fn strip_comments(s: &str) -> String {
    s.lines()
        .map(|l| match l.find("--") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sqlite_params(args: &[Value]) -> Vec<Box<dyn rusqlite::ToSql>> {
    args.iter()
        .map(|v| -> Box<dyn rusqlite::ToSql> {
            match v {
                Value::Null => Box::new(Option::<i64>::None),
                Value::Integer(n) => Box::new(*n),
                Value::Real(f) => Box::new(*f),
                Value::Text(t) => Box::new(t.clone()),
                Value::Blob(b) => Box::new(b.clone()),
            }
        })
        .collect()
}

fn from_sqlite(v: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(n) => Value::Integer(n),
        ValueRef::Real(f) => Value::Real(f),
        ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).to_string()),
        ValueRef::Blob(b) => Value::Blob(b.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn each() -> [Engine; 2] {
        [Engine::Sqlite, Engine::Turso]
    }

    #[test]
    fn both_engines_speak_the_same_sql() {
        for e in each() {
            let db = Db::memory(e).unwrap();
            db.batch(
                "create table t (a text primary key, b integer);
                 -- a comment, which one of them cannot parse
                 create unique index if not exists t_b on t (b);",
            )
            .unwrap();
            assert_eq!(
                db.execute("insert into t values (?1, ?2)", &[text("x"), int(1)])
                    .unwrap(),
                1,
                "{e:?}"
            );
            // A conditional upsert must report that it changed nothing: that is
            // how one moan learns another already claimed the work.
            let n = db
                .execute(
                    "insert into t values (?1, ?2) on conflict(a) do update set b = ?2 where t.b < 0",
                    &[text("x"), int(9)],
                )
                .unwrap();
            assert_eq!(n, 0, "{e:?} must report a refused upsert");

            let r = db
                .one("select a, b from t where a = ?1", &[text("x")])
                .unwrap()
                .unwrap();
            assert_eq!(r.text(0), "x", "{e:?}");
            assert_eq!(r.i64(1), 1, "{e:?}");
            assert!(db
                .one("select a from t where a = ?1", &[text("nope")])
                .unwrap()
                .is_none());
        }
    }

    #[test]
    fn nulls_survive_the_round_trip() {
        for e in each() {
            let db = Db::memory(e).unwrap();
            db.batch("create table t (a text, b integer)").unwrap();
            db.execute(
                "insert into t values (?1, ?2)",
                &[
                    opt_text(Option::<String>::None),
                    opt_int(Option::<i64>::None),
                ],
            )
            .unwrap();
            let r = db.one("select a, b from t", &[]).unwrap().unwrap();
            assert_eq!(r.opt_text(0), None, "{e:?}");
            assert_eq!(r.opt_i64(1), None, "{e:?}");
        }
    }

    #[test]
    fn a_write_transaction_commits_and_rolls_back() {
        for e in each() {
            let db = Db::memory(e).unwrap();
            db.batch("create table t (a integer)").unwrap();
            db.begin().unwrap();
            db.execute("insert into t values (1)", &[]).unwrap();
            db.commit().unwrap();
            db.begin().unwrap();
            db.execute("insert into t values (2)", &[]).unwrap();
            db.rollback();
            let rows = db.rows("select a from t", &[]).unwrap();
            assert_eq!(rows.len(), 1, "{e:?} rolled back the second");
        }
    }
}
