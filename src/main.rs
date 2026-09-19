//! moan — a compressed, live transcript of your coding-agent sessions.

mod config;
mod db;
mod event;
mod llm;
mod secrets;
mod source;
mod store;
mod ui;

/// Prose at or under this many characters already is its gist.
pub const VERBATIM_LIMIT: usize = 96;
/// How much of a long message to show while the model is still thinking.
pub const PENDING_CLIP: usize = 90;
/// A prompt this long or shorter is shown verbatim. Higher than the limit for
/// assistant prose: your own words carry information a paraphrase loses.
pub const PROMPT_VERBATIM_LIMIT: usize = 200;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// What `conversations` used to be called. Kept working so nobody's fingers
/// have to relearn it.
const OLD_LIST_NAME: &str = "sessions"; // vocabulary-exempt: the old name, on purpose

use config::Config;
use store::Store;
use ui::app::{ago, bytes, tilde, App};

#[derive(Parser)]
#[command(
    name = "moan",
    version,
    about = "A compressed, live record of what your coding agents did"
)]
struct Cli {
    /// Open a specific conversation instead of the most recent one.
    #[arg(long, short)]
    session: Option<String>,

    /// Prefer the newest session for this directory. Defaults to the cwd.
    #[arg(long, short)]
    project: Option<String>,

    /// Show every session, not just this project's.
    #[arg(long, short)]
    any: bool,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// List known conversations, newest first.
    // The old name keeps working; it is spelled at the dispatch site so the
    // vocabulary check does not have to make an exception for it.
    #[command(alias = OLD_LIST_NAME)]
    Conversations {
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    /// Print the config file path and the resolved model.
    Config {
        /// Print the available provider presets.
        #[arg(long)]
        providers: bool,
    },
    /// Check that the configured model answers.
    Doctor,
    /// Show what a synced copy would hold, and send it if configured.
    Sync {
        /// Say what would leave, and send nothing.
        #[arg(long)]
        dry_run: bool,
        /// Take in a shared archive from another machine.
        #[arg(long, value_name = "FILE")]
        merge: Option<PathBuf>,
    },
    /// Apply the retention policy to the archive now.
    Prune {
        /// Ceiling in megabytes, overriding the configured one.
        #[arg(long)]
        max_mb: Option<u64>,
        /// Drop sessions older than this many days.
        #[arg(long)]
        days: Option<u64>,
        /// Keep at most this many conversations.
        #[arg(long)]
        sessions: Option<usize>,
    },
    /// Write a conversation to stdout as markdown.
    Export {
        /// Which conversation. Defaults to the most recent.
        session: Option<String>,
        /// Include assistant reasoning.
        #[arg(long)]
        thinking: bool,
        /// Include tool calls.
        #[arg(long)]
        tools: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::load()?;

    match cli.cmd {
        Some(Cmd::Conversations { limit }) => conversations(limit),
        Some(Cmd::Config { providers }) => show_config(&cfg, providers),
        Some(Cmd::Doctor) => doctor(&cfg).await,
        Some(Cmd::Sync { dry_run, merge }) => sync(cfg, dry_run, merge),
        Some(Cmd::Prune {
            max_mb,
            days,
            sessions,
        }) => prune(cfg, max_mb, days, sessions),
        Some(Cmd::Export {
            session,
            thinking,
            tools,
        }) => export(cfg, session, thinking, tools).await,
        None => tui(cli, cfg).await,
    }
}

async fn tui(cli: Cli, cfg: Config) -> Result<()> {
    let mut store = Store::open(&config::data_path()?, cfg.store.engine)?;
    // Keep the archive inside its budget without anybody having to remember.
    // Only does work when it is over, so the usual case costs one page read.
    let dropped = store.enforce(&cfg.retain).unwrap_or(0);
    // Somebody who has read something here before does not need the keys
    // naming at them.
    let fresh = store.stats().map(|(_, e, _)| e == 0).unwrap_or(true);
    let mut app = App::new(cfg, store)?;
    app.first_run = fresh;
    if dropped > 0 {
        app.status = format!("archive over budget — set aside {dropped} old conversation(s)");
    }
    app.refresh_sessions()?;

    if app.sessions.is_empty() {
        anyhow::bail!(
            "nothing to read yet.\n\nmoan reads what Claude Code writes as it works, from\n~/.claude/projects. Have a conversation with Claude Code,\nthen run moan again."
        );
    }

    // Reopening should be returning. An explicit conversation, or asking for
    // any, overrides it.
    if cli.session.is_none() && !cli.any && cli.project.is_none() && app.resume().unwrap_or(false) {
        return ui::run(&mut app).await;
    }

    let chosen = match &cli.session {
        Some(id) => app
            .sessions
            .iter()
            .find(|s| s.id.starts_with(id))
            .cloned()
            .with_context(|| {
                format!("no conversation matching {id}. `moan conversations` lists them")
            })?,
        None => {
            let dir = if cli.any {
                None
            } else {
                cli.project.clone().or_else(|| {
                    std::env::current_dir()
                        .ok()
                        .map(|d| d.to_string_lossy().to_string())
                })
            };
            app.pick_default(dir.as_deref())
                .context("no conversation to open")?
        }
    };
    app.open(chosen)?;
    ui::run(&mut app).await
}

fn conversations(limit: usize) -> Result<()> {
    let all = source::all_sessions()?;
    if all.is_empty() {
        println!(
            "nothing to read yet.\n\n\
             moan reads what Claude Code writes as it works, from\n\
             ~/.claude/projects. Have a conversation with Claude Code,\n\
             then run moan again."
        );
        return Ok(());
    }
    for s in all.iter().take(limit) {
        println!(
            "{:<10}  {:<44}  {:>10}  {:>6}  {}",
            &s.id[..s.id.len().min(8)],
            tilde(&s.title),
            ago(s.modified),
            bytes(s.bytes),
            s.source
        );
    }
    Ok(())
}

fn show_config(cfg: &Config, providers: bool) -> Result<()> {
    if providers {
        for p in config::ModelConfig::provider_names() {
            println!("{p}");
        }
        return Ok(());
    }
    println!("config  {}", config::config_path()?.display());
    println!("data    {}", config::data_path()?.display());
    println!("model   {} · {}", cfg.model.provider, cfg.model.model);
    println!("api     {:?} at {}", cfg.model.api, cfg.model.base_url);
    let key = if cfg.model.api_key_env.is_empty() {
        "none needed".to_string()
    } else if cfg.model.resolve_key().is_some() {
        format!("found in {}", cfg.model.key_origin())
    } else {
        format!("${} is NOT set", cfg.model.api_key_env)
    };
    println!("key     {key}");

    if let Ok(s) = Store::open(&config::data_path()?, cfg.store.engine) {
        if let Ok((se, ev, gi)) = s.stats() {
            println!("stored  {se} conversations · {ev} messages · {gi} summaries");
        }
        let used = s.size().unwrap_or(0);
        let disk = s.on_disk().unwrap_or(used);
        // The two differ where an engine cannot give freed pages back: the
        // space is reused for what comes next rather than returned, so the
        // file plateaus instead of shrinking.
        let room = if disk > used {
            format!("  ({} on disk)", ui::app::bytes(disk))
        } else {
            String::new()
        };
        println!(
            "engine  {:?} · {} used{room}",
            s.engine(),
            ui::app::bytes(used)
        );
        // What has actually been sent to a paid API, counted by the provider
        // rather than estimated here. A summary is bought once and cached
        // against its content, so this only grows when you read something new.
        let (n, tin, tout) = s.spent();
        if n > 0 {
            let touched = s.summarised().len();
            let all = s.stats().map_or(0, |(se, _, _)| se);
            // Older summaries were bought before this was recorded; saying so
            // beats printing a zero that looks like they were free.
            let toks = if tin + tout > 0 {
                format!(" · {tin} tokens in, {tout} out")
            } else {
                " · bought before token counting".to_string()
            };
            println!("spent   {n} summaries over {touched} of {all} conversations{toks}");
        } else {
            println!("spent   nothing — no message has been sent to a model");
        }
    }
    Ok(())
}

/// Build the archive that is allowed to leave, and say exactly what is in it.
///
/// Turso replicates a whole database, so what syncs is a second, smaller file
/// rather than a filtered view of the real one. Nothing is sent until
/// `remote_url` is set, and this always prints what it holds first.
fn sync(cfg: Config, dry_run: bool, merge: Option<PathBuf>) -> Result<()> {
    if let Some(from) = merge {
        let mut store = Store::open(&config::data_path()?, cfg.store.engine)?;
        let took = store.merge_from(&from, cfg.store.engine)?;
        println!(
            "took in        {} conversations · {} messages · {} summaries",
            took.sessions, took.events, took.gists
        );
        println!("nothing local was replaced");
        return Ok(());
    }
    let data = config::data_path()?;
    let dest = data.with_file_name("moan-shared.db");
    let store = Store::open(&data, cfg.store.engine)?;
    let held = store.export_subset(&dest, cfg.sync.raw, cfg.store.engine)?;

    println!("shareable copy  {}", dest.display());
    println!(
        "holds           {} conversations · {} messages · {} summaries · {}",
        held.sessions,
        held.events,
        held.gists,
        ui::app::bytes(held.bytes)
    );
    println!(
        "original text   {}",
        if cfg.sync.raw {
            format!(
                "included — {} of it, every command the agent ran",
                ui::app::bytes(held.raw_bytes)
            )
        } else {
            "left behind (set sync.raw = true to include it)".into()
        }
    );

    if cfg.sync.remote_url.is_empty() {
        println!("remote          not set — nothing has left this machine");
        return Ok(());
    }
    let token = std::env::var(&cfg.sync.auth_token_env).unwrap_or_default();
    if token.is_empty() {
        anyhow::bail!(
            "${} is not set. It is the token for {}; export it, or clear sync.remote_url to keep everything local",
            cfg.sync.auth_token_env,
            cfg.sync.remote_url
        );
    }
    if dry_run {
        println!(
            "remote          {} (dry run, sending nothing)",
            cfg.sync.remote_url
        );
        return Ok(());
    }
    anyhow::bail!(
        "this build does not push to a remote: the shareable copy at {} is ready, and attaching it to {} is the step that remains",
        dest.display(),
        cfg.sync.remote_url
    )
}

fn prune(
    cfg: Config,
    max_mb: Option<u64>,
    days: Option<u64>,
    sessions: Option<usize>,
) -> Result<()> {
    let r = config::RetainConfig {
        max_mb: max_mb.unwrap_or(cfg.retain.max_mb),
        days: days.unwrap_or(cfg.retain.days),
        sessions: sessions.unwrap_or(cfg.retain.sessions),
    };
    let mut store = Store::open(&config::data_path()?, cfg.store.engine)?;
    let before = store.size()?;
    let dropped = store.enforce(&r)?;
    store.checkpoint();
    let after = store.size()?;
    println!(
        "set aside {dropped} conversation(s) · {} → {}",
        ui::app::bytes(before),
        ui::app::bytes(after)
    );
    Ok(())
}

async fn doctor(cfg: &Config) -> Result<()> {
    println!("model   {} · {}", cfg.model.provider, cfg.model.model);
    if cfg.model.missing_key() {
        anyhow::bail!(
            "${} is not set, so there is nothing to summarise with.\n\n\
             Export it, or put the key in a file and point model.api_key_file at it.\n\
             `moan config` shows where that is. A local server needs no key at all:\n\
             set provider = \"ollama\" or \"lmstudio\".",
            cfg.model.api_key_env
        );
    }
    let c = llm::Client::new(cfg.model.clone())?;
    let sample = "I looked at the parser and the reason the offsets drift is that \
        a partially written line is counted before it is complete, so the next \
        poll re-reads it and the same event lands twice. Fixed by only advancing \
        past the last newline.";
    let t = std::time::Instant::now();
    match c.condense(&event::Kind::Say, sample, false).await {
        Ok(g) => {
            println!("ok      {:?}", t.elapsed());
            println!("summary {}", g.0);
            println!("cost    {} tokens in, {} out", g.1.input, g.1.output);
            Ok(())
        }
        Err(e) => anyhow::bail!(
            "{e}\n\nThe model is {} at {}. `moan config --providers` lists the presets.",
            cfg.model.model,
            cfg.model.base_url
        ),
    }
}

/// Dump a session as markdown, condensing anything not already condensed.
async fn export(
    mut cfg: Config,
    session: Option<String>,
    thinking: bool,
    tools: bool,
) -> Result<()> {
    cfg.ui.show_thinking = thinking;
    cfg.ui.show_tools = tools;
    let all = source::all_sessions()?;
    let s = match &session {
        Some(id) => all
            .iter()
            .find(|s| s.id.starts_with(id))
            .cloned()
            .with_context(|| {
                format!("no conversation matching {id}. `moan conversations` lists them")
            })?,
        None => all
            .first()
            .cloned()
            .context("nothing to read yet. Have a conversation with a coding agent first")?,
    };

    let store = Store::open(&config::data_path()?, cfg.store.engine)?;
    let mut app = App::new(cfg, store)?;
    app.sources = source::all();
    // An export is the whole conversation, so nothing is out of range.
    app.condense_all = true;
    app.open(s.clone())?;

    // Give the pool a chance to drain before printing.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while app.pending > 0 && std::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_secs(5), app.done.recv()).await {
            Ok(Some(d)) => app.absorb(d),
            _ => break,
        }
    }

    println!("# {}\n", tilde(&s.title));
    println!(
        "_{} · {} messages_\n",
        s.modified.format("%Y-%m-%d %H:%M"),
        app.view.len()
    );
    // The same conversation the feed shows: merged turns, tools and reasoning
    // included only if they are switched on.
    for i in 0..app.view.len() {
        let Some(e) = app.at(i) else { continue };
        let mut lines = e.gist.lines();
        let head = lines.next().unwrap_or("");
        let out = e
            .outcome
            .as_deref()
            .map(|o| format!("  — {o}"))
            .unwrap_or_default();
        let who = match e.kind {
            event::Kind::Say => app.agent_of(&e.session),
            _ => e.kind.label(),
        };
        println!(
            "- `{}` **{}** {}{}",
            ui::app::clock(e.ts, true),
            who,
            head,
            out
        );
        for b in lines {
            println!("    {}", b.trim());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Every command and flag the README names must exist, and every key of
    /// the config must be documented. Three descriptions of one tool drift
    /// apart, and the generated config file is a fourth.
    #[test]
    fn the_readme_and_the_program_agree() {
        let readme = include_str!("../README.md");
        let cmd = Cli::command();
        let names: Vec<String> = cmd
            .get_subcommands()
            .map(|c| c.get_name().to_string())
            .collect();

        // Anything the README shows as a command must be one.
        // Only inside fenced blocks: prose says "moan fuses a turn", which is
        // a sentence rather than a command.
        let mut missing = Vec::new();
        let mut fenced = false;
        for line in readme.lines() {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if !fenced {
                continue;
            }
            // At the start of a line, so the mockup's header row — which
            // begins with a space and then the name — is not read as one.
            let Some(rest) = line.strip_prefix("moan ") else {
                continue;
            };
            let Some(word) = rest.split_whitespace().next() else {
                continue;
            };
            if !word.chars().all(|c| c.is_ascii_lowercase()) {
                continue;
            }
            if !names.iter().any(|n| n == word) {
                missing.push(word.to_string());
            }
        }
        assert!(
            missing.is_empty(),
            "the README names commands that do not exist: {missing:?}"
        );

        // And every command must be mentioned, or nobody will find it.
        let undocumented: Vec<&String> = names
            .iter()
            .filter(|n| *n != "help" && !readme.contains(&format!("moan {n}")))
            .collect();
        assert!(
            undocumented.is_empty(),
            "commands nobody is told about: {undocumented:?}"
        );
    }

    #[test]
    fn every_setting_is_written_down() {
        // The file moan writes for somebody who has none is the reference, so
        // it has to hold every key rather than the ones that were easy.
        let written = toml::to_string_pretty(&Config::default()).unwrap();
        let readme = include_str!("../README.md");
        let mut hidden = Vec::new();
        for line in written.lines() {
            let Some((key, _)) = line.split_once(" = ") else {
                continue;
            };
            let key = key.trim();
            if !readme.contains(key) {
                hidden.push(key.to_string());
            }
        }
        assert!(
            hidden.is_empty(),
            "settings the README never mentions: {hidden:?}"
        );
    }
}
