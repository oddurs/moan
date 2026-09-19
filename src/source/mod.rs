//! Transcript sources. Claude Code today; Codex and opencode slot in behind
//! the same two methods.

pub mod claude;
pub mod codex;
pub mod opencode;

use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::event::Event;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    /// Project path or session name — whatever identifies it to a human.
    pub title: String,
    pub source: &'static str,
    pub path: PathBuf,
    pub modified: DateTime<Utc>,
    pub bytes: u64,
    /// The branch the session was working on, when it recorded one.
    pub branch: Option<String>,
    /// The repository this belongs to. Worktrees of one project share it.
    pub project: String,
    /// Whether the last thing said was the assistant's — so the session is
    /// waiting on a human. `None` when it could not be told.
    pub assistant_last: Option<bool>,
}

impl Session {
    /// What to call this session inside its project: the branch if it has one,
    /// otherwise the directory, otherwise a short id.
    pub fn tag(&self) -> String {
        if let Some(b) = &self.branch {
            return b.clone();
        }
        match self.title.rsplit('/').next() {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => self.id.chars().take(6).collect(),
        }
    }
}

/// Where a source had read up to.
///
/// Opaque on purpose. A byte offset is right for an append-only file and
/// meaningless for a transcript kept in a database, so the store writes this
/// down and hands it back without ever looking inside. A source handed one it
/// cannot make sense of returns the start, and the conversation is read again
/// — which is free, because a re-read inserts nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cursor(pub String);

impl Cursor {
    pub fn start() -> Self {
        Self(String::new())
    }

    /// For a source whose position is a place in a file.
    pub fn at(n: u64) -> Self {
        Self(n.to_string())
    }

    /// The position as a number, or the start if it is not one.
    pub fn offset(&self) -> u64 {
        self.0.parse().unwrap_or(0)
    }
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a `stat` alone tells us about a transcript.
#[derive(Debug, Clone)]
pub struct Stub {
    pub id: String,
    pub path: PathBuf,
    pub modified: DateTime<Utc>,
    pub bytes: u64,
    pub source: &'static str,
    /// The directory's name, as a last-resort identity.
    pub slug: String,
}

/// What reading into a transcript tells us. Cached against the file's
/// timestamp and size, because finding it out means opening the file and
/// there are hundreds of them.
#[derive(Debug, Clone)]
pub struct Facts {
    pub title: String,
    pub branch: Option<String>,
    pub project: String,
    pub assistant_last: Option<bool>,
}

pub trait Source: Send + Sync {
    fn name(&self) -> &'static str;

    /// Every transcript, from `stat` alone. Runs on every launch, so it must
    /// not open anything.
    fn list(&self) -> Result<Vec<Stub>>;

    /// What the transcript says about itself. Only ever called when the cache
    /// has nothing for this version of the file.
    fn describe(&self, stub: &Stub) -> Facts;

    /// Read from where we left off; return new events and where to resume.
    ///
    /// Both the path and the conversation are given, because a source's
    /// transcripts are not necessarily one per file: opencode keeps every
    /// conversation in one database, and a path alone would not say which.
    fn parse(&self, path: &Path, session: &str, from: &Cursor) -> Result<(Vec<Event>, Cursor)>;
}

impl Session {
    pub fn build(stub: Stub, facts: Facts) -> Self {
        Session {
            id: stub.id,
            title: facts.title,
            branch: facts.branch,
            project: facts.project,
            assistant_last: facts.assistant_last,
            source: stub.source,
            path: stub.path,
            modified: stub.modified,
            bytes: stub.bytes,
        }
    }
}

/// Newest first, before anything has been read.
pub fn newest_first_stubs(v: &mut [Stub]) {
    v.sort_by_key(|s| std::cmp::Reverse(s.modified));
}

/// Newest first — the order every listing wants.
pub fn newest_first(v: &mut [Session]) {
    v.sort_by_key(|s| std::cmp::Reverse(s.modified));
}

/// Every session every source knows about, reading each transcript to find
/// out about it. For the command line, where there is no cache to consult.
pub fn all_sessions() -> Result<Vec<Session>> {
    let mut v = Vec::new();
    for src in all() {
        // One source being unreadable is not a reason to list none of them.
        let Ok(stubs) = src.list() else { continue };
        for stub in stubs {
            let facts = src.describe(&stub);
            v.push(Session::build(stub, facts));
        }
    }
    newest_first(&mut v);
    Ok(v)
}

pub fn all() -> Vec<Box<dyn Source>> {
    let mut v: Vec<Box<dyn Source>> = Vec::new();
    if let Ok(c) = claude::ClaudeSource::new() {
        v.push(Box::new(c));
    }
    if let Ok(c) = codex::CodexSource::new() {
        v.push(Box::new(c));
    }
    if let Ok(c) = opencode::OpencodeSource::new() {
        v.push(Box::new(c));
    }
    v
}

/// The repository a working directory belongs to, so every worktree of a
/// project groups with its main checkout.
///
/// A worktree's `.git` is a file pointing at `<main>/.git/worktrees/<name>`,
/// which is how the two are tied back together. Done by reading the files
/// rather than shelling out to git: the list resolves hundreds of these.
pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    for a in dir.ancestors() {
        let g = a.join(".git");
        if g.is_dir() {
            return Some(a.to_path_buf());
        }
        if g.is_file() {
            let text = std::fs::read_to_string(&g).ok()?;
            let target = text.trim().strip_prefix("gitdir:")?.trim();
            // `<main>/.git/worktrees/<name>` — walk back to what holds `.git`.
            let dotgit = Path::new(target)
                .ancestors()
                .find(|p| p.file_name().is_some_and(|n| n == ".git"))?;
            return dotgit.parent().map(Path::to_path_buf);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worktree_resolves_to_its_main_checkout() {
        let tmp = std::env::temp_dir().join("moan-repo-test");
        let main = tmp.join("proj");
        let wt = tmp.join("wt/feat-x");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(main.join(".git/worktrees/feat-x")).unwrap();
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}/.git/worktrees/feat-x\n", main.display()),
        )
        .unwrap();

        assert_eq!(repo_root(&main).as_deref(), Some(main.as_path()));
        assert_eq!(repo_root(&wt).as_deref(), Some(main.as_path()));
        // A directory inside the worktree resolves the same way.
        assert_eq!(repo_root(&wt.join("src")).as_deref(), Some(main.as_path()));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn a_plain_directory_has_no_repo() {
        assert!(repo_root(Path::new("/")).is_none());
    }
}
