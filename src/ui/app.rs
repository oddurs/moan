//! Application state and the event loop.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use tokio::sync::mpsc;

use crate::config::Config;
use crate::event::{Event, GistSource};
use crate::llm::Client;
use crate::source::{self, Cursor, Session, Source};
use crate::store::Store;

/// How long the newest message must go untouched before it is worth paying to
/// condense. Long enough to cover the gap between blocks of one turn, short
/// enough that a finished turn resolves while you are still looking at it.
const SETTLE_SECS: i64 = 20;

/// What one row of the feed points at: a parsed event, or a message merged
/// from a run of them.
#[derive(Debug, Clone, Copy)]
pub enum Row {
    One(usize),
    Many(usize),
    /// A caption saying whose feed the messages below it came from.
    Origin(usize),
}

/// Group assistant prose into turns.
///
/// A run is the text blocks of one turn: consecutive `Say` events of the same
/// session, uninterrupted by anything the human or the harness said. Tool calls
/// and reasoning sit inside a turn and do not end it.
fn fuse_runs(events: &[Event]) -> Vec<Vec<usize>> {
    use crate::event::Kind;
    let mut runs = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    let flush = |cur: &mut Vec<usize>, runs: &mut Vec<Vec<usize>>| {
        if cur.len() > 1 {
            runs.push(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    for (i, e) in events.iter().enumerate() {
        let same = cur.first().is_none_or(|&f| events[f].session == e.session);
        match e.kind {
            Kind::Say if same => cur.push(i),
            Kind::Say => {
                flush(&mut cur, &mut runs);
                cur.push(i);
            }
            // A prompt or a note closes the turn; tools and thinking do not.
            Kind::Prompt | Kind::Note => flush(&mut cur, &mut runs),
            _ => {}
        }
    }
    flush(&mut cur, &mut runs);
    runs
}

/// Fuse a run of assistant text blocks into the single message they formed.
fn merge_run(events: &[Event], idx: &[usize]) -> Event {
    let first = &events[idx[0]];
    let last = &events[idx[idx.len() - 1]];
    let raw = idx
        .iter()
        .map(|&i| events[i].raw.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut e = Event {
        // Stable across rebuilds, so an expanded message stays expanded.
        uid: format!("{}+{}", first.uid, idx.len()),
        session: first.session.clone(),
        ts: last.ts,
        role: crate::event::Role::Assistant,
        kind: crate::event::Kind::Say,
        gist: String::new(),
        gist_src: GistSource::Pending,
        raw,
        outcome: None,
        is_error: false,
        tokens: idx.iter().map(|&i| events[i].tokens).sum(),
    };
    let one = crate::event::flatten(&e.raw);
    if one.chars().count() <= crate::VERBATIM_LIMIT {
        e.gist = one;
        e.gist_src = GistSource::Verbatim;
    } else {
        e.gist = crate::event::clip(
            &crate::event::flatten(&crate::event::preview(&e.raw)),
            crate::PENDING_CLIP,
        );
    }
    e
}

/// A condensation handed to the worker pool.
pub struct Job {
    pub session: String,
    pub uid: String,
    pub hash: String,
    pub kind: crate::event::Kind,
    pub text: String,
    /// Ask for a headline plus bullets rather than a single line.
    pub bullets: bool,
}

/// A condensation coming back.
pub struct Done {
    pub session: String,
    pub uid: String,
    pub hash: String,
    pub result: Result<String, String>,
    /// What it cost, as the provider counted it.
    pub used: crate::llm::Usage,
}

#[derive(PartialEq, Eq)]
pub enum Mode {
    Feed,
    Help,
    Filter,
}

pub struct App {
    pub cfg: Config,
    pub store: Store,
    pub sources: Vec<Box<dyn Source>>,

    pub session: Option<Session>,
    pub events: Vec<Event>,
    /// One row per chat message. Assistant prose that ran across a turn is
    /// merged into a single message, so the feed reads you, claude, you.
    pub view: Vec<Row>,
    /// The fused messages `Row::Many` points at.
    pub fused: Vec<Event>,
    /// Divider captions `Row::Origin` points at.
    pub origins: Vec<String>,
    pub expanded: HashSet<String>,

    pub mode: Mode,
    pub selected: usize,
    /// Whether the cursor must be brought into view before the next frame.
    ///
    /// One-shot, and set by the keyboard rather than the wheel: the layout it
    /// needs only exists inside the renderer, and expanding a message or a new
    /// arrival invalidates it, so the decision is deferred to where the sizes
    /// are known.
    pub reveal: bool,
    /// Blank rows above the conversation, when it is shorter than the pane and
    /// sits against the bottom. A click has to skip them to land on a message.
    pub feed_pad: usize,
    /// First display line of the feed's viewport.
    ///
    /// A `usize`, not the `u16` ratatui's scroll offset takes: this is an
    /// absolute line number into a feed that can run to tens of thousands, and
    /// the mouse handler adds a row to it to find what was clicked. Truncating
    /// it would put clicks on the wrong message. Only the small windowed
    /// offset actually handed to ratatui has to fit a `u16`.
    pub scroll: usize,
    pub follow: bool,
    pub filter: String,

    pub sessions: Vec<Session>,
    /// Conversation id → messages a model was paid to summarise. Absent means
    /// it has cost nothing.
    pub paid: HashMap<String, usize>,
    /// Every session of the open session's project, newest first. One entry
    /// when nothing else is running there.
    pub peers: Vec<Session>,
    /// Show every peer interleaved rather than one session at a time.
    pub merged: bool,
    /// How many rows the feed can show, set by the renderer. Condensation
    /// reaches one screen either side of this, so what it costs follows the
    /// size of the window rather than a number picked in advance.
    pub viewport: usize,
    /// How wide the terminal is, so the mouse can tell which pane it is over.
    pub width: u16,
    /// Condense the whole view regardless of where the reader is. Only
    /// `moan export`, which is the whole conversation by definition.
    pub condense_all: bool,
    /// Whether the newest turn has been left long enough to be worth paying
    /// for. Reset whenever anything arrives.
    settled_done: bool,

    pub status: String,
    pub status_is_error: bool,
    pub pending: usize,
    pub model_label: String,
    /// Nobody has read anything here before, so the keys are worth naming.
    ///
    /// For the whole of that first run, and no longer: it used to clear on
    /// reaching the end of a conversation, which happens on the first poll,
    /// so the line it was meant to show appeared for a few milliseconds and
    /// was never read by anybody.
    /// When the conversations were last looked for.
    scanned: Option<DateTime<Utc>>,
    /// How many messages were kept back this run because they hold something
    /// that must not leave the machine.
    /// Whichever agent wrote most of what is here.
    ///
    /// Only the others are marked in the list. A letter on every row when
    /// ninety-five of a hundred say the same thing is noise; marking the
    /// exception is the information.
    pub common_agent: &'static str,
    pub withheld: usize,
    pub first_run: bool,
    pub quit: bool,
    pub dirty: bool,
    /// Advances on each poll; drives the working indicator's animation.
    pub tick: u64,
    /// Everything running, beside what you are reading.
    pub panel: crate::ui::panel::Panel,
    /// Resolved once at startup; every colour on screen comes from here.
    pub theme: crate::ui::theme::Theme,
    /// First display line of each row. Maps a mouse click back to a message,
    /// and lets the renderer find the top of the viewport by bisection.
    ///
    /// Cached because it is the one part of drawing that has to touch every
    /// row: summing heights per frame made the frame linear in the length of
    /// the conversation. Rebuilt when the rows change, when something is
    /// expanded, or when the terminal is resized — never per frame.
    pub row_starts: Vec<usize>,
    /// Total height of the conversation, in lines.
    pub total_lines: usize,
    /// The width `row_starts` was measured at.
    laid_out_at: u16,

    /// How far we have read into each transcript, by session id.
    offsets: HashMap<String, Cursor>,
    /// Hashes already queued, so a re-poll does not double-spend.
    queued: HashSet<String>,
    jobs: mpsc::Sender<Job>,
    pub done: mpsc::Receiver<Done>,
}

impl App {
    pub fn new(cfg: Config, store: Store) -> Result<Self> {
        let cfg_theme = cfg.ui.theme;
        let (jtx, jrx) = mpsc::channel::<Job>(1024);
        let (dtx, drx) = mpsc::channel::<Done>(1024);

        let (model_label, status, err) = match Client::new(cfg.model.clone()) {
            Ok(c) if cfg.model.missing_key() => {
                let l = c.describe();
                (
                    l,
                    format!(
                        "${} is not set — showing truncations",
                        cfg.model.api_key_env
                    ),
                    true,
                )
            }
            Ok(c) => {
                let l = c.describe();
                spawn_pool(Arc::new(c), jrx, dtx, cfg.model.concurrency);
                (l, String::new(), false)
            }
            Err(e) => ("none".into(), format!("model unavailable: {e}"), true),
        };

        Ok(Self {
            paid: HashMap::new(),
            cfg,
            store,
            sources: source::all(),
            session: None,
            events: Vec::new(),
            view: Vec::new(),
            fused: Vec::new(),
            origins: Vec::new(),
            expanded: HashSet::new(),
            mode: Mode::Feed,
            selected: 0,
            scroll: 0,
            follow: true,
            filter: String::new(),
            sessions: Vec::new(),
            peers: Vec::new(),
            merged: false,
            reveal: false,
            feed_pad: 0,
            viewport: 40,
            width: 80,
            condense_all: false,
            settled_done: false,
            status,
            status_is_error: err,
            pending: 0,
            model_label,
            scanned: None,
            common_agent: "claude",
            withheld: 0,
            first_run: true,
            quit: false,
            dirty: true,
            tick: 0,
            panel: crate::ui::panel::Panel::default(),
            theme: crate::ui::theme::Theme::of(cfg_theme),
            row_starts: Vec::new(),
            total_lines: 0,
            laid_out_at: 0,
            offsets: HashMap::new(),
            queued: HashSet::new(),
            jobs: jtx,
            done: drx,
        })
    }

    // ── sessions ────────────────────────────────────────────────────────

    /// Find every session, reading a transcript only when what we know about
    /// it is out of date.
    ///
    /// A `stat` per file and a lookup per file, against opening and reading
    /// several hundred of them. This runs on every launch and every time the
    /// panel opens, and it is the difference between reopening feeling
    /// instant and taking half a second.
    pub fn refresh_sessions(&mut self) -> Result<()> {
        // Opening the list should show what is there now, but opening it twice
        // in a moment — which resuming does — should not scan twice.
        if self
            .scanned
            .is_some_and(|t| (Utc::now() - t).num_milliseconds() < 500)
        {
            return Ok(());
        }
        let mut all = Vec::new();
        let mut unreadable: Vec<&str> = Vec::new();
        for src in &self.sources {
            // A source that cannot be read contributes nothing. It must not
            // take the others with it: opencode keeps its conversations in a
            // database that can be busy, missing or unreadable, and none of
            // that is a reason for moan not to start.
            let stubs = match src.list() {
                Ok(v) => v,
                Err(_) => {
                    unreadable.push(src.name());
                    continue;
                }
            };
            for stub in stubs {
                let key = stub.path.to_string_lossy().to_string();
                let facts = match self.store.scan_get(&key, stub.modified, stub.bytes) {
                    Some(f) => f,
                    None => {
                        let f = src.describe(&stub);
                        let _ = self.store.scan_put(&key, stub.modified, stub.bytes, &f);
                        f
                    }
                };
                all.push(source::Session::build(stub, facts));
            }
        }
        source::newest_first(&mut all);
        let mut tally: HashMap<&'static str, usize> = HashMap::new();
        for s in &all {
            *tally.entry(s.source).or_default() += 1;
        }
        if let Some((name, _)) = tally.into_iter().max_by_key(|(_, n)| *n) {
            self.common_agent = name;
        }
        self.sessions = all;
        // Which conversations have been sent to a model, so the list can say
        // so. Cheap: one grouped count over an indexed column.
        self.paid = self.store.summarised();
        if !unreadable.is_empty() && self.status.is_empty() {
            self.status = format!("could not read {}", unreadable.join(" or "));
            self.status_is_error = false;
        }
        // Stamped on finishing, not on starting: a cold scan reads every
        // transcript and can outlast the window, so a debounce measured from
        // the start expires before the work it was meant to cover is done.
        self.scanned = Some(Utc::now());
        Ok(())
    }

    /// Pick the newest session for `dir`, falling back to the newest anywhere.
    pub fn pick_default(&mut self, dir: Option<&str>) -> Option<Session> {
        if let Some(d) = dir {
            let want = crate::ui::app::tilde(d);
            if let Some(s) = self.sessions.iter().find(|s| s.title == want) {
                return Some(s.clone());
            }
        }
        self.sessions.first().cloned()
    }

    pub fn open(&mut self, s: Session) -> Result<()> {
        // Everything running on this project, so the header and the merged
        // view have something to show without a second scan.
        if self.sessions.is_empty() {
            self.refresh_sessions()?;
        }
        self.peers = self
            .sessions
            .iter()
            .filter(|p| p.project == s.project)
            .cloned()
            .collect();
        if !self.peers.iter().any(|p| p.id == s.id) {
            self.peers.insert(0, s.clone());
        }
        self.session = Some(s);
        self.expanded.clear();
        self.queued.clear();
        self.follow = true;
        self.reload()
    }

    /// Load the active set from the store, then catch up from disk.
    fn reload(&mut self) -> Result<()> {
        self.events.clear();
        self.offsets.clear();
        for s in self.active() {
            let at = self
                .store
                .register(&s.id, s.source, &s.title, &s.path.to_string_lossy())?;
            // A truncated or rewritten transcript invalidates a saved offset.
            // A transcript that has shrunk was rewritten or rotated; whatever
            // position we had in it means nothing now.
            let at = if at.offset() > s.bytes {
                self.store.reset_offset(&s.id)?;
                Cursor::start()
            } else {
                at
            };
            self.offsets.insert(s.id.clone(), at);
            self.events.extend(self.store.load(&s.id)?);
        }
        self.order();
        self.poll()?;
        self.rebuild();
        self.selected = self.view.len().saturating_sub(1);
        self.dirty = true;
        Ok(())
    }

    /// The sessions being read right now.
    pub fn active(&self) -> Vec<Session> {
        match (&self.session, self.merged) {
            (Some(_), true) => self.peers.clone(),
            (Some(s), false) => vec![s.clone()],
            (None, _) => Vec::new(),
        }
    }

    /// Interleave by time. Within one session the file order already is time
    /// order, so this only ever matters across sessions.
    fn order(&mut self) {
        if self.merged {
            self.events.sort_by_key(|e| e.ts);
        }
    }

    /// Show the project's sessions together, or just the open one.
    pub fn set_merged(&mut self, on: bool) -> Result<()> {
        if self.merged == on {
            return Ok(());
        }
        self.merged = on;
        self.reload()
    }

    /// Move to another session of the same project.
    pub fn cycle(&mut self, delta: i64) -> Result<()> {
        if self.peers.len() < 2 {
            return Ok(());
        }
        let here = self
            .session
            .as_ref()
            .and_then(|c| self.peers.iter().position(|p| p.id == c.id))
            .unwrap_or(0);
        // Wrapping in both directions over a handful of sessions, without
        // leaving usize to do it.
        // Wrapping in both directions over a handful of sessions, without
        // leaving usize to do it.
        let n = self.peers.len();
        let step = delta.rem_euclid(crate::db::from_usize(n)).unsigned_abs() as usize;
        let next = (here + step) % n;
        let s = self.peers[next].clone();
        self.merged = false;
        self.open(s)
    }

    /// Jump straight to the nth session of the project.
    pub fn jump(&mut self, n: usize) -> Result<()> {
        if let Some(s) = self.peers.get(n).cloned() {
            self.merged = false;
            self.open(s)?;
        }
        Ok(())
    }

    /// Read whatever the transcript has grown by since the last poll.
    pub fn poll(&mut self) -> Result<()> {
        let mut arrived = false;
        for s in self.active() {
            let Some(src) = self.sources.iter().find(|x| x.name() == s.source) else {
                continue;
            };
            let from = self.offsets.get(&s.id).cloned().unwrap_or_default();
            // The archive outlives the transcripts it was built from, which is
            // most of the reason it exists. A conversation whose transcript has
            // been rotated away still reads; it just stops growing.
            if !s.path.exists() {
                continue;
            }
            // One unreadable transcript must not stop the others being read.
            // A conversation can be truncated, replaced underneath us, or not
            // be valid UTF-8; none of that is a reason to stop.
            let (mut fresh, at) = match src.parse(&s.path, &s.id, &from) {
                Ok(v) => v,
                Err(e) => {
                    self.status = format!("{}: {e}", self.name_of(&s.id));
                    self.status_is_error = true;
                    continue;
                }
            };
            if at == from && fresh.is_empty() {
                continue;
            }
            self.offsets.insert(s.id.clone(), at.clone());

            // A cached gist is free; only a miss becomes a job, and only then
            // if the view ends up showing it.
            for e in &mut fresh {
                if e.gist_src != GistSource::Pending {
                    continue;
                }
                if let Ok(Some(g)) = self.store.cached_gist(&e.content_hash()) {
                    e.gist = g;
                    e.gist_src = GistSource::Model;
                }
            }
            self.store.append(&s.id, &fresh, &at)?;
            arrived |= !fresh.is_empty();
            self.events.extend(fresh);
        }
        if !arrived {
            return Ok(());
        }
        self.settled_done = false;
        self.order();
        self.rebuild();
        if self.follow {
            self.selected = self.view.len().saturating_sub(1);
            self.mark_read();
        }
        self.dirty = true;
        Ok(())
    }

    /// Send a message for condensing. False if it was already on its way.
    fn enqueue(&mut self, e: &Event) -> bool {
        if !e.kind.needs_model() {
            return false;
        }
        let hash = e.content_hash();
        if !self.queued.insert(hash.clone()) {
            return false;
        }
        let job = Job {
            session: e.session.clone(),
            uid: e.uid.clone(),
            hash: hash.clone(),
            kind: e.kind.clone(),
            text: e.raw.clone(),
            bullets: e.is_complex(),
        };
        let sent = self.jobs.try_send(job).is_ok();
        if sent {
            self.pending += 1;
        } else {
            // The pool is gone or full; let it be tried again later.
            self.queued.remove(&hash);
        }
        sent
    }

    pub fn absorb(&mut self, d: Done) {
        self.pending = self.pending.saturating_sub(1);
        match d.result {
            Ok(gist) if !gist.is_empty() => {
                let _ = self.store.save_gist(
                    &d.session,
                    &d.uid,
                    &d.hash,
                    &gist,
                    &self.model_label,
                    d.used,
                );
                // The same paragraph can be showing on several rows, and a
                // merged message is not in `events` at all — both have to be
                // reached, or the gist lands only in the cache and the feed
                // keeps showing its truncation until the next run.
                let apply = |e: &mut Event| {
                    // Anything still waiting on an answer — sent, not sent, or
                    // previously refused. Narrowing this to one state is how a
                    // finished condensation ends up in the cache and never on
                    // the row. Checked before hashing, which is not free.
                    // Withheld is deliberate, not a wait: an answer arriving
                    // for the same text from elsewhere must not overwrite it.
                    let waiting = matches!(
                        e.gist_src,
                        GistSource::Pending | GistSource::Working | GistSource::Failed
                    );
                    if waiting && e.content_hash() == d.hash {
                        e.gist = gist.clone();
                        e.gist_src = GistSource::Model;
                    }
                };
                self.events.iter_mut().for_each(apply);
                self.fused.iter_mut().for_each(apply);
            }
            Ok(_) => {}
            Err(msg) => {
                self.status = msg;
                self.status_is_error = true;
                // Say so on the row, and let `r` ask again. Drop the claim so
                // another moan is free to try.
                self.queued.remove(&d.hash);
                let _ = self.store.release(&d.hash);
                let mark = |e: &mut Event| {
                    if e.gist_src == GistSource::Working && e.content_hash() == d.hash {
                        e.gist_src = GistSource::Failed;
                    }
                };
                self.events.iter_mut().for_each(mark);
                self.fused.iter_mut().for_each(mark);
            }
        }
        self.dirty = true;
    }

    /// Forget what has been queued, so those hashes can be sent again.
    pub fn forget_queued(&mut self) {
        self.queued.clear();
    }

    /// Queue every still-pending event. Used after a key becomes available.
    pub fn retry_pending(&mut self) {
        self.sync_view();
    }

    /// Force a fresh condensation of the selected event.
    pub fn recondense(&mut self) {
        self.anchor_cursor();
        let Some(e) = self.selected_event().cloned() else {
            return;
        };
        if !e.kind.needs_model() {
            self.status = format!("{} lines are condensed by rule", e.kind.label());
            self.status_is_error = false;
            return;
        }
        let hash = e.content_hash();
        self.queued.remove(&hash);
        self.enqueue(&e);
    }

    // ── view ────────────────────────────────────────────────────────────

    pub fn rebuild(&mut self) {
        let f = self.filter.to_lowercase();
        let show = |e: &Event| match &e.kind {
            crate::event::Kind::Think => self.cfg.ui.show_thinking,
            crate::event::Kind::Tool { .. } => self.cfg.ui.show_tools,
            _ => true,
        };

        // A turn's prose arrives as several blocks with tool calls between
        // them, and ten lines from one turn is not a conversation. Fuse each
        // run into the single message it always was.
        //
        // Grouped over the whole stream rather than over what is on screen, so
        // hiding the tool calls does not change what a message contains. It
        // used to: the gist is keyed on content, so every toggle of `o` bought
        // the entire session again.
        let runs = fuse_runs(&self.events);
        let mut fused: Vec<Event> = Vec::new();
        // Where each run's fused message goes, and which run a row belongs to.
        let mut anchor: HashMap<usize, usize> = HashMap::new();
        let mut member: HashMap<usize, usize> = HashMap::new();
        for run in &runs {
            fused.push(merge_run(&self.events, run));
            let id = fused.len() - 1;
            anchor.insert(*run.last().unwrap(), id);
            for &m in run {
                member.insert(m, id);
            }
        }

        let matches = |e: &Event| {
            f.is_empty()
                || e.gist.to_lowercase().contains(&f)
                || e.raw.to_lowercase().contains(&f)
                || e.kind.label().to_lowercase().contains(&f)
        };

        let mut view = Vec::new();
        let mut origins = Vec::new();
        let mut showing: Option<String> = None;
        for (i, e) in self.events.iter().enumerate() {
            // A fused run is drawn once, where its last block sits.
            let row = match member.get(&i) {
                Some(_) if !anchor.contains_key(&i) => continue,
                Some(&id) => {
                    if !matches(&fused[id]) {
                        continue;
                    }
                    Row::Many(id)
                }
                None => {
                    if !show(e) || !matches(e) {
                        continue;
                    }
                    Row::One(i)
                }
            };

            // In the merged view, say whose feed this is — but only when it
            // changes, the way a chat groups a run of messages under one name.
            if self.merged && showing.as_deref() != Some(e.session.as_str()) {
                showing = Some(e.session.clone());
                origins.push(self.name_of(&e.session));
                view.push(Row::Origin(origins.len() - 1));
            }
            view.push(row);
        }

        self.view = view;
        self.fused = fused;
        self.origins = origins;
        self.invalidate_layout();
        if self.selected >= self.view.len() {
            self.selected = self.view.len().saturating_sub(1);
        }
        self.sync_view();
    }

    /// Give every row the feed will actually show a gist: from the cache if
    /// we have paid for it before, otherwise by queueing it.
    ///
    /// Only what is on screen costs anything. When a turn's prose is merged
    /// into one message, the blocks it was made of are never condensed — that
    /// would be a dozen model calls for a line nobody sees.
    #[cfg(test)]
    pub fn sync_view_for_test(&mut self) {
        self.sync_view();
    }

    fn sync_view(&mut self) {
        // Nearest the reader first, and only out to the window: scrolling
        // pulls more in, so nothing you look at stays a truncation for long.
        let (lo, hi) = if self.condense_all {
            (0, self.view.len())
        } else {
            // Keyed off the viewport rather than the cursor. What you are
            // looking at is what wants condensing, and since the wheel moves
            // the viewport without the cursor, the cursor stopped being a
            // reliable stand-in for where you are reading.
            //
            // One screen either side: enough that scrolling stays ahead of
            // you, and nowhere near enough to buy a whole project.
            let (first, last) = self.rows_in_view();
            let reach = self.viewport.max(12);
            (
                first.saturating_sub(reach),
                last.saturating_add(reach)
                    .saturating_add(1)
                    .min(self.view.len()),
            )
        };
        let last = self.view.len().saturating_sub(1);
        for i in (lo..hi).rev() {
            let row = self.view[i];
            let e = match row {
                Row::One(i) => &self.events[i],
                Row::Many(i) => &self.fused[i],
                Row::Origin(_) => continue,
            };
            if !matches!(e.gist_src, GistSource::Pending | GistSource::Failed)
                || !e.kind.needs_model()
            {
                continue;
            }
            // Never send a message that holds a credential. Refused rather
            // than redacted: a redaction that misses looks safe.
            if let Some(found) = crate::secrets::found_in(&e.raw) {
                let slot = match row {
                    Row::One(i) => &mut self.events[i],
                    Row::Many(i) => &mut self.fused[i],
                    Row::Origin(_) => continue,
                };
                slot.gist_src = GistSource::Withheld;
                self.withheld += 1;
                self.status = format!("a message holds {} — summarised here, not sent", found.0);
                self.status_is_error = false;
                continue;
            }
            // The newest message is still being written: a turn arrives as
            // several blocks, and each one changes what the message says.
            // Condensing on every block pays ten times for one message, and
            // only the last answer would have been right.
            if i == last && !self.settled(e) {
                continue;
            }
            let hash = e.content_hash();
            match self.store.cached_gist(&hash) {
                Ok(Some(g)) => {
                    let slot = match row {
                        Row::One(i) => &mut self.events[i],
                        Row::Many(i) => &mut self.fused[i],
                        Row::Origin(_) => continue,
                    };
                    slot.gist = g;
                    slot.gist_src = GistSource::Model;
                }
                // Another moan may already be condensing this one. Leave it
                // pending: its answer lands in the shared cache, and the next
                // sweep picks it up.
                Ok(None) if !self.store.claim(&hash).unwrap_or(true) => continue,
                _ => {
                    let e = e.clone();
                    if self.enqueue(&e) {
                        let slot = match row {
                            Row::One(i) => &mut self.events[i],
                            Row::Many(i) => &mut self.fused[i],
                            Row::Origin(_) => continue,
                        };
                        slot.gist_src = GistSource::Working;
                    }
                }
            }
        }
    }

    /// Open the first place the selected message points at.
    ///
    /// Most terminals will already make a printed url clickable. This is for
    /// the ones that will not, and for a link written as `[text](url)` whose
    /// destination is not on screen.
    pub fn open_link(&mut self) {
        self.anchor_cursor();
        let Some(e) = self.selected_event() else {
            return;
        };
        let found = crate::ui::markdown::links(&e.raw);
        let Some(url) = found.first().cloned() else {
            self.status = "no link in this message".into();
            self.status_is_error = false;
            self.dirty = true;
            return;
        };
        // Only ever a web address, and never handed to a shell.
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            self.status = format!("not opening {url}");
            self.status_is_error = true;
            self.dirty = true;
            return;
        }
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        match std::process::Command::new(opener)
            .arg(&url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(_) => {
                let more = found.len().saturating_sub(1);
                self.status = match more {
                    0 => format!("opened {url}"),
                    n => format!("opened {url}  (+{n} more in this message)"),
                };
                self.status_is_error = false;
            }
            Err(e) => {
                self.status = format!("could not run {opener}: {e}");
                self.status_is_error = true;
            }
        }
        self.dirty = true;
    }

    /// Has this message stopped changing?
    ///
    /// Only ever asked of the newest one; everything before it has something
    /// after it and so is finished by definition.
    fn settled(&self, _e: &Event) -> bool {
        // Judged by the session going quiet, not by this message's own last
        // line. A turn's text pauses while a tool runs, and asking the message
        // when it was last touched says "a while ago" in the middle of a turn
        // that is still being written — so it gets condensed, then grows, then
        // has to be condensed again.
        self.events
            .last()
            .is_none_or(|last| (Utc::now() - last.ts).num_seconds() >= SETTLE_SECS)
    }

    /// Has the assistant said nothing since you last posted?
    ///
    /// True while it is still working, including when it is running tools and
    /// has not spoken yet — which is most of the wait. Goes quiet on its own
    /// if the session was abandoned, because nothing is being written.
    pub fn awaiting(&self) -> bool {
        let Some(last) = self.events.last() else {
            return false;
        };
        // An abandoned session must not spin forever.
        if (Utc::now() - last.ts).num_seconds() > 300 {
            return false;
        }
        for e in self.events.iter().rev() {
            match e.kind {
                crate::event::Kind::Say => return false,
                crate::event::Kind::Prompt => return true,
                _ => {}
            }
        }
        false
    }

    /// Look again for gists that appeared since last time.
    ///
    /// Most of the time this finds nothing. It matters when another moan is
    /// sharing the archive: whatever it condensed shows up here without
    /// anybody touching a key.
    pub fn sweep(&mut self) {
        if self.view.iter().any(|r| {
            matches!(r, Row::One(_) | Row::Many(_))
                && self
                    .at_row(*r)
                    .is_some_and(|e| e.gist_src == GistSource::Pending)
        }) {
            self.sync_view();
            self.dirty = true;
        }
    }

    fn at_row(&self, row: Row) -> Option<&Event> {
        match row {
            Row::One(i) => self.events.get(i),
            Row::Many(i) => self.fused.get(i),
            Row::Origin(_) => None,
        }
    }

    /// Pick up the turn we deferred, once it has gone quiet. Fires once per
    /// lull rather than on every tick, so a settled feed costs nothing.
    pub fn settle(&mut self) {
        if self.settled_done {
            return;
        }
        let ready = self.events.last().is_some_and(|e| self.settled(e));
        if !ready {
            return;
        }
        self.settled_done = true;
        self.sync_view();
        self.dirty = true;
    }

    /// Is anything recent enough to still be fading, and so worth redrawing?
    pub fn fading(&self) -> bool {
        self.events
            .last()
            .is_some_and(|e| (Utc::now() - e.ts).num_seconds() < crate::ui::render::FADE_MAX)
    }

    /// Recompute where every row starts, if anything it depends on moved.
    ///
    /// Heights are knowable without laying a row out — one line per note, plus
    /// the original text when it is expanded — so this is cheap, and it only
    /// runs when the rows, the expansions or the width actually change.
    pub fn lay_out(&mut self, width: u16, body_height: impl Fn(&Event, u16) -> usize) {
        #[cfg(test)]
        LAYOUTS.with(|n| n.set(n.get() + 1));
        self.laid_out_at = width;
        self.row_starts.clear();
        self.row_starts.reserve(self.view.len());
        let mut total = 0;
        for i in 0..self.view.len() {
            self.row_starts.push(total);
            total += match self.view[i] {
                Row::Origin(_) => 1,
                _ => match self.at(i) {
                    Some(e) => {
                        let expanded = self.expanded.contains(&e.uid);
                        crate::ui::render::message_height(e)
                            + if expanded { body_height(e, width) } else { 0 }
                    }
                    None => 1,
                },
            };
        }
        self.total_lines = total;
    }

    /// Has anything the layout depends on changed since it was measured?
    pub fn needs_layout(&self, width: u16) -> bool {
        self.laid_out_at != width || self.row_starts.len() != self.view.len()
    }

    /// Say that the layout is stale — the rows or the expansions moved.
    pub fn invalidate_layout(&mut self) {
        self.laid_out_at = 0;
    }

    /// The event a row stands for.
    pub fn at(&self, row: usize) -> Option<&Event> {
        match self.view.get(row)? {
            Row::One(i) => self.events.get(*i),
            Row::Many(i) => self.fused.get(*i),
            Row::Origin(_) => None,
        }
    }

    // ── the panel ───────────────────────────────────────────────────────

    /// Rebuild the panel's rows for whatever it is showing.
    pub fn rebuild_panel(&mut self) {
        use crate::ui::panel::{Mode, Row};
        let mut rows = Vec::new();
        match self.panel.mode {
            Mode::Sessions => {
                // Typing narrows the list. With a hundred conversations on a
                // machine, arrow keys are not navigation.
                let f = self.panel.query.to_lowercase();
                let matches = |s: &Session| {
                    f.is_empty()
                        || s.project.to_lowercase().contains(&f)
                        || s.tag().to_lowercase().contains(&f)
                        || s.source.contains(f.as_str())
                };

                let mut seen: Vec<String> = Vec::new();
                // Where you are comes first; everything else by how recently
                // it moved. Hunting for the project you are in, in a list
                // sorted by somebody else's activity, is not navigation.
                let here = self.session.as_ref().map(|s| s.project.clone());
                let order: Vec<&Session> = here
                    .iter()
                    .flat_map(|p| self.sessions.iter().filter(move |s| &s.project == p))
                    .chain(
                        self.sessions
                            .iter()
                            .filter(|s| Some(&s.project) != here.as_ref()),
                    )
                    .collect();
                for s in order {
                    if seen.contains(&s.project) {
                        continue;
                    }
                    seen.push(s.project.clone());
                    // A project matches if it does, or if any of its
                    // conversations does — so typing a branch finds it.
                    let mine: Vec<usize> = self
                        .sessions
                        .iter()
                        .enumerate()
                        .filter(|(_, q)| q.project == s.project && matches(q))
                        .map(|(i, _)| i)
                        .collect();
                    if mine.is_empty() {
                        continue;
                    }
                    // A project with one conversation does not need a heading
                    // over a single line repeating it. No blank row between
                    // projects either: with sixty of them that is sixty rows
                    // of nothing, and the heading already separates them.
                    if mine.len() > 1 {
                        rows.push(Row::Project {
                            title: s.project.clone(),
                            count: mine.len(),
                        });
                    }
                    rows.extend(mine.into_iter().map(Row::Session));
                }
            }
            // Search builds its own rows from what it finds in the archive.
            Mode::Search => {
                self.run_search();
                return;
            }
        }
        self.panel.rows = rows;
        self.panel.settle();
    }

    /// Run the panel's query and lay the hits out under their sessions.
    pub fn run_search(&mut self) {
        use crate::ui::panel::Row;
        let kinds = self.shown_kinds();
        self.panel.hits = self
            .store
            .search(&self.panel.query, &kinds, 200)
            .unwrap_or_default();

        let mut rows = Vec::new();
        let mut showing: Option<String> = None;
        for (i, h) in self.panel.hits.iter().enumerate() {
            if showing.as_deref() != Some(h.session.as_str()) {
                showing = Some(h.session.clone());
                let title = self
                    .sessions
                    .iter()
                    .find(|s| s.id == h.session)
                    .map(|s| s.project.clone())
                    .unwrap_or_else(|| h.session.chars().take(8).collect());
                if !rows.is_empty() {
                    rows.push(Row::Gap);
                }
                rows.push(Row::Project { title, count: 0 });
            }
            rows.push(Row::Result(i));
        }
        self.panel.rows = rows;
        self.panel.settle();
        self.dirty = true;
    }

    /// Put a stored message under the cursor, by where it sits in its session.
    fn jump_to(&mut self, session: &str, seq: usize) {
        let Some(target) = self
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.session == session)
            .nth(seq)
            .map(|(i, _)| i)
        else {
            self.status = "that message is no longer in the archive".into();
            self.status_is_error = true;
            return;
        };
        // A fused message stands for every block it was made of.
        let row = self.view.iter().position(|r| match r {
            Row::One(i) => *i == target,
            Row::Many(i) => self.fused[*i].uid.starts_with(&self.events[target].uid),
            Row::Origin(_) => false,
        });
        if let Some(r) = row.or_else(|| {
            self.view
                .iter()
                .position(|x| matches!(x, Row::One(i) if *i >= target))
        }) {
            self.selected = r;
            self.follow = false;
            self.reveal = true;
            self.sync_view();
            self.dirty = true;
        }
    }

    /// Write down where the reader is, so the next run can return them to it.
    pub fn remember_place(&mut self) {
        let Some(s) = self.session.clone() else {
            return;
        };
        // Which message, counted in the conversation rather than in the view,
        // so it survives the tools being toggled or the filter changing.
        let message = self
            .selected_event()
            .and_then(|e| self.events.iter().position(|x| x.uid == e.uid))
            .unwrap_or(0);
        let _ = self
            .store
            .put_place(&s.id, message, self.panel.open, self.panel.mode.title());
    }

    /// Put the reader back where they were, if that place still exists.
    pub fn resume(&mut self) -> Result<bool> {
        let Some((id, message, panel, mode)) = self.store.place() else {
            return Ok(false);
        };
        let Some(s) = self.sessions.iter().find(|s| s.id == id).cloned() else {
            return Ok(false);
        };
        self.open(s)?;
        if panel {
            if let Some((m, _)) = crate::ui::panel::MODES.iter().find(|(_, n)| *n == mode) {
                self.panel.switch(*m);
                self.panel.typing = false;
            }
            self.toggle_panel()?;
            // Returning means returning to the conversation, not to the list.
            self.panel.focus = crate::ui::panel::Focus::Feed;
        }
        // The conversation may have grown since; landing on the same message
        // means landing where you stopped reading, not at the end.
        //
        // If that message has gone — pruned, or the parser re-read the
        // conversation — match nothing rather than everything: an empty
        // prefix is a prefix of every id, which would land on the first row.
        let want = self.events.get(message).map(|e| e.uid.clone());
        let row = want.as_ref().and_then(|uid| {
            self.view.iter().position(|r| match r {
                Row::One(i) => *i >= message,
                Row::Many(i) => self.fused[*i].uid.starts_with(uid.as_str()),
                Row::Origin(_) => false,
            })
        });
        // Landing on the last row is the same as following, so leave it
        // following rather than quietly pausing on arrival.
        if let Some(row) = row.filter(|r| r + 1 < self.view.len()) {
            self.selected = row;
            self.follow = false;
            self.reveal = true;
            self.sync_view();
        }
        self.dirty = true;
        Ok(true)
    }

    /// The kinds the feed is currently showing.
    pub fn shown_kinds(&self) -> Vec<&'static str> {
        let mut k = vec!["prompt", "say", "note"];
        if self.cfg.ui.show_tools {
            k.push("tool");
            k.push("fail");
        }
        if self.cfg.ui.show_thinking {
            k.push("think");
        }
        k
    }

    /// Note that the open session has been read to the end.
    ///
    /// Only when you are actually at the bottom: scrolling back through old
    /// messages is not reading the new ones.
    pub fn mark_read(&mut self) {
        if !self.follow || self.merged {
            return;
        }
        let Some(s) = self.session.clone() else {
            return;
        };
        let n = self.events.iter().filter(|e| e.session == s.id).count();
        let _ = self.store.mark_seen(&s.id, n);
    }

    /// What has arrived in a session since it was last read.
    pub fn unread_of(&self, id: &str) -> Option<(usize, usize)> {
        if !self.store.known(id) {
            return None;
        }
        self.store.unread(id, &self.shown_kinds()).ok()
    }

    /// Which agent wrote a conversation, for naming who is speaking.
    ///
    /// Read from the conversation rather than stored on every message: it is
    /// a fact about the transcript, and one word repeated across ten thousand
    /// rows is ten thousand copies of the same word.
    pub fn agent_of(&self, session: &str) -> &'static str {
        self.session
            .iter()
            .chain(self.peers.iter())
            .chain(self.sessions.iter())
            .find(|s| s.id == session)
            .map_or("claude", |s| s.source)
    }

    /// What to call a conversation, wherever it is named.
    ///
    /// A project with one conversation is named by the project: `~/Code/moan`
    /// says more than `main` does, and far more than six characters of a uuid.
    /// Where a project has several, the project is already written above them,
    /// so the branch is what tells them apart — and where two share a branch,
    /// a little of the id is added to make them distinct.
    pub fn name_of(&self, id: &str) -> String {
        let pool = if self.sessions.iter().any(|s| s.id == id) {
            &self.sessions
        } else {
            &self.peers
        };
        let Some(s) = pool.iter().find(|s| s.id == id) else {
            return id.chars().take(6).collect();
        };
        let siblings: Vec<&Session> = pool.iter().filter(|q| q.project == s.project).collect();
        if siblings.len() == 1 {
            return s.project.clone();
        }
        let tag = s.tag();
        if siblings.iter().filter(|q| q.tag() == tag).count() > 1 {
            format!("{tag}·{}", &s.id[..4.min(s.id.len())])
        } else {
            tag
        }
    }

    /// How a session is doing, for the panel's marks.
    pub fn session_state(&self, s: &Session) -> crate::ui::panel::State {
        // Only the open session's events are loaded, so the question of who
        // spoke last can only be answered for it. Everything else is judged
        // by its file, which is enough to say whether it is live.
        // The open session is known exactly from what is loaded; every other
        // is judged from the end of its transcript.
        let mine = self.session.as_ref().is_some_and(|c| c.id == s.id);
        let last_assistant = if !mine {
            s.assistant_last
        } else {
            Some(
                self.events
                    .iter()
                    .rev()
                    .find_map(|e| match e.kind {
                        crate::event::Kind::Say => Some(true),
                        crate::event::Kind::Prompt => Some(false),
                        _ => None,
                    })
                    .unwrap_or(false),
            )
        };
        crate::ui::panel::state(s, last_assistant)
    }

    pub fn toggle_panel(&mut self) -> Result<()> {
        use crate::ui::panel::Focus;
        self.panel.open = !self.panel.open;
        if self.panel.open {
            self.refresh_sessions()?;
            self.rebuild_panel();
            // Opening it is a request to browse.
            self.panel.focus = Focus::Panel;
            if let Some(cur) = self.session.clone() {
                if let Some(i) = self.sessions.iter().position(|s| s.id == cur.id) {
                    if let Some(row) = self
                        .panel
                        .rows
                        .iter()
                        .position(|r| matches!(r, crate::ui::panel::Row::Session(j) if *j == i))
                    {
                        self.panel.selected = row;
                    }
                }
            }
        } else {
            self.panel.focus = Focus::Feed;
        }
        self.dirty = true;
        Ok(())
    }

    /// Open whatever the panel has selected.
    pub fn panel_enter(&mut self) -> Result<()> {
        let Some(row) = self.panel.current().cloned() else {
            return Ok(());
        };
        match row {
            crate::ui::panel::Row::Session(i) => {
                if let Some(s) = self.sessions.get(i).cloned() {
                    // The panel stays open: choosing one session is usually
                    // the first of several.
                    self.merged = false;
                    self.open(s)?;
                    self.rebuild_panel();
                }
            }
            crate::ui::panel::Row::Result(i) => {
                let Some(hit) = self.panel.hits.get(i).cloned() else {
                    return Ok(());
                };
                // Open its session if we are not already in it, then put the
                // matching message under the cursor. The results stay up, so
                // the next one is one keypress away.
                if self.session.as_ref().is_none_or(|c| c.id != hit.session) {
                    if let Some(s) = self.sessions.iter().find(|s| s.id == hit.session).cloned() {
                        self.merged = false;
                        self.open(s)?;
                    }
                }
                self.jump_to(&hit.session, hit.seq);
            }
            _ => {}
        }
        Ok(())
    }

    pub fn selected_event(&self) -> Option<&Event> {
        self.at(self.selected)
    }

    pub fn toggle_expand(&mut self) {
        self.anchor_cursor();
        if let Some(e) = self.selected_event() {
            let k = e.uid.clone();
            if !self.expanded.remove(&k) {
                self.expanded.insert(k);
            }
            self.invalidate_layout();
            // The row just changed height under the cursor, so where it sits
            // has to be worked out again from the new sizes.
            self.reveal = true;
            self.dirty = true;
        }
    }

    /// The first and last view rows the viewport is showing.
    ///
    /// Falls back to the cursor before anything has been laid out, which is
    /// the first frame and nothing after it.
    fn rows_in_view(&self) -> (usize, usize) {
        let last_row = self.view.len().saturating_sub(1);
        if self.row_starts.is_empty() {
            let at = self.selected.min(last_row);
            return (at, at);
        }
        let h = self.viewport.max(1);
        let first = self
            .row_starts
            .partition_point(|&s| s <= self.scroll)
            .saturating_sub(1);
        let last = self
            .row_starts
            .partition_point(|&s| s < self.scroll + h)
            .saturating_sub(1);
        (first.min(last_row), last.max(first).min(last_row))
    }

    #[cfg(test)]
    pub fn rows_in_view_for_test(&self) -> (usize, usize) {
        self.rows_in_view()
    }

    /// Move the viewport without moving the cursor.
    ///
    /// The wheel scrolls what you are looking at; the arrows move what is
    /// chosen. Conflating them means a glance down the page silently changes
    /// what `o` would expand and what `r` would re-condense.
    ///
    /// The one thing scrolling may do to the cursor is carry it: a cursor left
    /// off-screen is a cursor you cannot see acting on, so it rides along at
    /// whichever edge it went out of.
    pub fn scroll_by(&mut self, d: i64) {
        let h = self.viewport.max(1);
        let floor = self.total_lines.saturating_sub(h.min(self.total_lines));
        self.scroll = if d < 0 {
            self.scroll.saturating_sub(d.unsigned_abs() as usize)
        } else {
            self.scroll.saturating_add(d.unsigned_abs() as usize)
        }
        .min(floor);
        // At the bottom is following; anywhere above it is reading.
        self.follow = self.scroll >= floor;
        self.sync_view();
        self.dirty = true;
    }

    /// Put the cursor on a visible row before acting on it.
    ///
    /// The wheel leaves the cursor where it was — that is the whole point of
    /// decoupling them. But an action has to apply to a row you can see, and
    /// there are two ways to get there: drag the view back to the cursor, or
    /// bring the cursor to the view. Dragging the view back throws away the
    /// place you deliberately scrolled to, so the cursor moves instead. After
    /// scrolling, "down" means down from what you are looking at, which is
    /// what you meant by it.
    ///
    /// It also means nothing is pinned to a screen edge while you scroll: the
    /// highlight simply is not drawn until you reach for it.
    fn anchor_cursor(&mut self) {
        if self.row_starts.is_empty() || self.view.is_empty() {
            return;
        }
        let (first, last) = self.rows_in_view();
        self.selected = self.selected.clamp(first, last);
    }

    pub fn move_by(&mut self, d: i64) {
        if self.view.is_empty() {
            return;
        }
        self.anchor_cursor();
        // Clamped rather than wrapped: running off either end stays put.
        self.selected = if d < 0 {
            self.selected.saturating_sub(d.unsigned_abs() as usize)
        } else {
            (self.selected + d.unsigned_abs() as usize).min(self.view.len() - 1)
        };
        // Stepping onto the last message resumes following; stepping off it
        // means you want to read.
        self.follow = self.selected + 1 == self.view.len();
        self.reveal = true;
        self.sync_view();
        self.dirty = true;
    }

    pub fn go_bottom(&mut self) {
        self.selected = self.view.len().saturating_sub(1);
        self.follow = true;
        self.sync_view();
        self.mark_read();
        self.dirty = true;
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
        self.follow = false;
        self.scroll = 0;
        self.reveal = true;
        self.sync_view();
        self.dirty = true;
    }
}

/// How many times the whole conversation has been measured, and how many rows
/// the last frame actually built. Both are what "drawing is flat" means, and
/// both are counts rather than clocks, so the assertion says the same thing on
/// a loaded machine as on an idle one.
///
/// Thread-local because the suite runs in parallel and every other test that
/// draws a frame would otherwise be counted too.
#[cfg(test)]
thread_local! {
    pub static LAYOUTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub static ROWS_BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Spawn a bounded pool that turns jobs into gists.
fn spawn_pool(
    client: Arc<Client>,
    mut jobs: mpsc::Receiver<Job>,
    done: mpsc::Sender<Done>,
    n: usize,
) {
    let limit = Arc::new(tokio::sync::Semaphore::new(n.max(1)));
    tokio::spawn(async move {
        while let Some(job) = jobs.recv().await {
            let Ok(permit) = limit.clone().acquire_owned().await else {
                break;
            };
            let c = client.clone();
            let tx = done.clone();
            tokio::spawn(async move {
                let r = c
                    .condense(&job.kind, &job.text, job.bullets)
                    .await
                    .map_err(|e| e.to_string());
                let (result, used) = match r {
                    Ok((g, u)) => (Ok(g), u),
                    Err(e) => (Err(e), crate::llm::Usage::default()),
                };
                let _ = tx
                    .send(Done {
                        session: job.session,
                        uid: job.uid,
                        hash: job.hash,
                        result,
                        used,
                    })
                    .await;
                drop(permit);
            });
        }
    });
}

// ── small formatting helpers shared with the renderer ───────────────────

pub fn tilde(p: &str) -> String {
    let home = directories::UserDirs::new()
        .map(|d| d.home_dir().to_string_lossy().to_string())
        .unwrap_or_default();
    match p.strip_prefix(&home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => p.to_string(),
    }
}

pub fn clock(ts: DateTime<Utc>, h24: bool) -> String {
    let t = ts.with_timezone(&Local);
    t.format(if h24 { "%H:%M" } else { "%l:%M" })
        .to_string()
        .trim()
        .to_string()
}

pub fn ago(ts: DateTime<Utc>) -> String {
    let s = (Utc::now() - ts).num_seconds().max(0);
    match s {
        0..=45 => "just now".into(),
        46..=5400 => format!("{}m ago", (s + 30) / 60),
        5401..=86_400 => format!("{}h ago", (s + 1800) / 3600),
        _ => format!("{}d ago", s / 86_400),
    }
}

pub fn bytes(n: u64) -> String {
    const U: [&str; 4] = ["B", "K", "M", "G"];
    // A byte count large enough to lose precision here is larger than any
    // archive, and this only ever renders "2M".
    #[allow(clippy::cast_precision_loss)]
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 3 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n}{}", U[0])
    } else {
        format!("{v:.0}{}", U[i])
    }
}
