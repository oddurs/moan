//! Configuration. One TOML file, one model, sensible presets.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub model: ModelConfig,
    pub ui: UiConfig,
    pub retain: RetainConfig,
    pub store: StoreConfig,
    pub sync: SyncConfig,
}

/// Where the archive lives.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreConfig {
    /// `turso` (pure Rust, can sync between machines) or `sqlite`.
    pub engine: crate::db::Engine,
}

/// Keeping the same conversations on more than one machine.
///
/// Off until `remote_url` is set. Turso replicates a whole database — there is
/// no way to hold a column back — so what syncs is a second, smaller archive
/// built from the first, and what goes into it is decided here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// A Turso database URL. Empty means nothing ever leaves the machine.
    pub remote_url: String,
    /// Environment variable holding the auth token.
    pub auth_token_env: String,
    /// Send the original text of every message as well as the condensations.
    ///
    /// Off, and deliberately. `events.raw` holds every shell command the agent
    /// ran, in full — this project's own transcript contains a `grep` for an
    /// API key pattern and the matched prefix in its output. Turso can encrypt
    /// what it stores, but it sends the key to the server in a header, so that
    /// protects the data from third parties rather than from the service.
    pub raw: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            remote_url: String::new(),
            auth_token_env: "TURSO_AUTH_TOKEN".into(),
            raw: false,
        }
    }
}

/// How much history to keep.
///
/// Size is the control that matters, because sessions differ enormously: on
/// one real machine the newest twenty came to 321MB while the newest hundred
/// came to 504MB, so a count says almost nothing about disk.
///
/// Condensations are never pruned. They are what cost money to produce and
/// they are tiny — tens of kilobytes against hundreds of megabytes of text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetainConfig {
    /// Ceiling for the archive. 0 to keep everything.
    pub max_mb: u64,
    /// Drop sessions older than this many days. 0 to ignore age.
    pub days: u64,
    /// Keep at most this many sessions. 0 for no limit.
    pub sessions: usize,
}

impl Default for RetainConfig {
    fn default() -> Self {
        // 256MB holds a week or two of heavy use and will never be reached by
        // anybody else. A tool for reading logs should not be the largest
        // thing in a config directory.
        Self {
            max_mb: 256,
            days: 0,
            sessions: 0,
        }
    }
}

/// The two wire formats worth supporting. Everything else speaks one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Api {
    /// `POST /chat/completions` — OpenAI, OpenRouter, Ollama, LM Studio,
    /// llama.cpp, vLLM, Groq, Together, DeepSeek, and most local servers.
    Openai,
    /// `POST /v1/messages` — Anthropic.
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    /// A preset name, or "custom" to use `base_url` and `api` as given.
    pub provider: String,
    pub model: String,
    pub api: Api,
    pub base_url: String,
    /// Environment variable holding the key. Preferred over `api_key`.
    pub api_key_env: String,
    /// Inline key. Only for keyless local servers or throwaway setups.
    pub api_key: String,
    /// A file holding the key, so a secret already on disk is referenced
    /// rather than copied. Accepts `export NAME=value`, `NAME=value`, or a
    /// file whose only contents are the key.
    pub api_key_file: String,
    /// Cap on a gist. Small on purpose — a long gist is a failed gist.
    pub max_tokens: u32,
    /// How many condensations to have in flight at once.
    pub concurrency: usize,
    /// Seconds before a condensation request is abandoned.
    pub timeout_secs: u64,
    /// Extra system guidance appended to the built-in condensation prompt.
    pub style: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Show assistant reasoning in the feed.
    pub show_thinking: bool,
    /// Show tool calls in the feed. Off by default: the feed is a
    /// conversation, and tool traffic buries it.
    pub show_tools: bool,
    /// Poll interval for transcript changes, milliseconds.
    pub poll_ms: u64,
    /// 12- or 24-hour clock in the gutter.
    pub clock_24h: bool,
    /// `auto`, `dark` or `light`. Auto reads `COLORFGBG` and assumes dark when
    /// the terminal does not say.
    pub theme: crate::ui::theme::Mode,
}

impl Default for ModelConfig {
    fn default() -> Self {
        let mut m = Self {
            provider: "anthropic".into(),
            model: String::new(),
            api: Api::Anthropic,
            base_url: String::new(),
            api_key_env: String::new(),
            api_key: String::new(),
            api_key_file: String::new(),
            max_tokens: 96,
            concurrency: 4,
            timeout_secs: 30,
            style: String::new(),
        };
        m.apply_preset();
        m
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_thinking: false,
            show_tools: false,
            poll_ms: 400,
            clock_24h: true,
            theme: crate::ui::theme::Mode::Auto,
        }
    }
}

/// `(provider, api, base_url, key_env, default_model)`
const PRESETS: &[(&str, Api, &str, &str, &str)] = &[
    (
        "anthropic",
        Api::Anthropic,
        "https://api.anthropic.com",
        "ANTHROPIC_API_KEY",
        "claude-haiku-4-5-20251001",
    ),
    (
        "openai",
        Api::Openai,
        "https://api.openai.com/v1",
        "OPENAI_API_KEY",
        "gpt-4o-mini",
    ),
    (
        "openrouter",
        Api::Openai,
        "https://openrouter.ai/api/v1",
        "OPENROUTER_API_KEY",
        "anthropic/claude-haiku-4.5",
    ),
    (
        "groq",
        Api::Openai,
        "https://api.groq.com/openai/v1",
        "GROQ_API_KEY",
        "llama-3.3-70b-versatile",
    ),
    (
        "together",
        Api::Openai,
        "https://api.together.xyz/v1",
        "TOGETHER_API_KEY",
        "meta-llama/Llama-3.3-70B-Instruct-Turbo",
    ),
    (
        "deepseek",
        Api::Openai,
        "https://api.deepseek.com",
        "DEEPSEEK_API_KEY",
        "deepseek-chat",
    ),
    (
        "gateway",
        Api::Openai,
        "https://ai-gateway.vercel.sh/v1",
        "AI_GATEWAY_API_KEY",
        "anthropic/claude-haiku-4.5",
    ),
    (
        "ollama",
        Api::Openai,
        "http://localhost:11434/v1",
        "",
        "qwen2.5:7b-instruct",
    ),
    (
        "lmstudio",
        Api::Openai,
        "http://localhost:1234/v1",
        "",
        "local-model",
    ),
    (
        "llamacpp",
        Api::Openai,
        "http://localhost:8080/v1",
        "",
        "local-model",
    ),
];

impl ModelConfig {
    /// Fill blanks from the named preset. Explicit values always win.
    pub fn apply_preset(&mut self) {
        let Some(&(_, api, url, env, model)) = PRESETS
            .iter()
            .find(|(n, ..)| *n == self.provider.to_lowercase())
        else {
            return;
        };
        self.api = api;
        if self.base_url.is_empty() {
            self.base_url = url.into();
        }
        if self.api_key_env.is_empty() {
            self.api_key_env = env.into();
        }
        if self.model.is_empty() {
            self.model = model.into();
        }
    }

    /// The key, from the config, a referenced file, or the environment.
    pub fn resolve_key(&self) -> Option<String> {
        if !self.api_key.is_empty() {
            return Some(self.api_key.clone());
        }
        if !self.api_key_file.is_empty() {
            if let Some(k) = key_from_file(&expand(&self.api_key_file), &self.api_key_env) {
                return Some(k);
            }
        }
        if self.api_key_env.is_empty() {
            return None;
        }
        std::env::var(&self.api_key_env)
            .ok()
            .filter(|s| !s.is_empty())
    }

    /// True when the provider needs a key we cannot find.
    pub fn missing_key(&self) -> bool {
        !self.api_key_env.is_empty() && self.resolve_key().is_none()
    }

    /// Where the key came from, for `moan config` to report.
    pub fn key_origin(&self) -> &'static str {
        if !self.api_key.is_empty() {
            "the config file"
        } else if !self.api_key_file.is_empty()
            && key_from_file(&expand(&self.api_key_file), &self.api_key_env).is_some()
        {
            "api_key_file"
        } else if self.resolve_key().is_some() {
            "the environment"
        } else {
            "nowhere"
        }
    }

    pub fn provider_names() -> Vec<&'static str> {
        PRESETS.iter().map(|(n, ..)| *n).collect()
    }
}

/// Pull a key out of a shell-style env file.
///
/// Prefers the assignment named by `want`; falls back to the first assignment,
/// then to a file that is nothing but the key.
fn key_from_file(path: &str, want: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut first = None;
    for line in text.lines() {
        let line = line.trim().trim_start_matches("export ").trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.split_once('=') {
            Some((name, value)) => {
                let value = value.trim().trim_matches(['"', '\'']).to_string();
                if value.is_empty() {
                    continue;
                }
                if name.trim() == want {
                    return Some(value);
                }
                first.get_or_insert(value);
            }
            // A file containing only the key.
            None if first.is_none() && !line.contains(char::is_whitespace) => {
                first = Some(line.to_string());
            }
            None => {}
        }
    }
    first
}

fn expand(path: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => directories::UserDirs::new()
            .map(|d| d.home_dir().join(rest).to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string()),
        None => path.to_string(),
    }
}

pub fn config_path() -> Result<PathBuf> {
    let d = directories::ProjectDirs::from("", "", "moan")
        .context("could not work out where to keep the config — is HOME set?")?;
    Ok(d.config_dir().join("config.toml"))
}

pub fn data_path() -> Result<PathBuf> {
    let d = directories::ProjectDirs::from("", "", "moan")
        .context("could not work out where to keep the archive — is HOME set?")?;
    Ok(d.data_dir().join("moan.db"))
}

impl Config {
    pub fn load() -> Result<Self> {
        let p = config_path()?;
        if !p.exists() {
            let c = Config::default();
            c.write(&p)?;
            return Ok(c);
        }
        let text =
            std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        let mut c: Config = toml::from_str(&text).with_context(|| {
            format!(
                "{} is not valid TOML. Fix the line it names, or delete the file \
                     and moan will write a fresh one",
                p.display()
            )
        })?;
        c.model.apply_preset();
        Ok(c)
    }

    pub fn write(&self, p: &PathBuf) -> Result<()> {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = format!("{HEADER}{}", toml::to_string_pretty(self)?);
        std::fs::write(p, body)?;
        Ok(())
    }
}

const HEADER: &str = "\
# moan — compressed transcripts of coding-agent sessions
#
# provider: anthropic | openai | openrouter | groq | together | deepseek
#           | gateway | ollama | lmstudio | llamacpp | custom
#
# For anything else, set provider = \"custom\" and give api (\"openai\" or
# \"anthropic\"), base_url, and model yourself. Local servers need no key.

";

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, body: &str) -> String {
        let p = std::env::temp_dir().join(format!("moan-key-{name}"));
        std::fs::write(&p, body).unwrap();
        p.to_string_lossy().to_string()
    }

    #[test]
    fn reads_a_shell_env_file() {
        let p = write(
            "env",
            "# a comment\n\nexport OTHER_KEY=nope\nexport OPENROUTER_API_KEY=sk-or-v1-abc\n",
        );
        assert_eq!(
            key_from_file(&p, "OPENROUTER_API_KEY").as_deref(),
            Some("sk-or-v1-abc")
        );
    }

    #[test]
    fn reads_a_file_that_is_only_the_key() {
        let p = write("bare", "sk-or-v1-bare\n");
        assert_eq!(
            key_from_file(&p, "ANYTHING").as_deref(),
            Some("sk-or-v1-bare")
        );
    }

    #[test]
    fn falls_back_to_the_first_assignment() {
        let p = write("first", "API_KEY=\"quoted-value\"\n");
        assert_eq!(
            key_from_file(&p, "SOMETHING_ELSE").as_deref(),
            Some("quoted-value")
        );
    }

    #[test]
    fn a_missing_file_is_not_a_key() {
        assert!(key_from_file("/nonexistent/moan/key", "K").is_none());
    }

    #[test]
    fn a_file_key_beats_the_environment() {
        let p = write("wins", "export MOAN_TEST_KEY=from-file\n");
        let mut c = ModelConfig {
            provider: "custom".into(),
            api_key_env: "MOAN_TEST_KEY".into(),
            api_key_file: p,
            ..Default::default()
        };
        c.apply_preset();
        assert_eq!(c.resolve_key().as_deref(), Some("from-file"));
        assert!(!c.missing_key());
        assert_eq!(c.key_origin(), "api_key_file");
    }
}
