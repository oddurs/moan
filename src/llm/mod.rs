//! Condensation. One request per event, one line back.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::config::{Api, ModelConfig};
use crate::event::Kind;

/// What a summary cost, as the provider counted it. Recorded rather than
/// estimated: the only honest answer to "what has this spent" is the one the
/// people charging for it gave.
#[derive(Debug, Clone, Copy, Default)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

pub struct Client {
    http: reqwest::Client,
    cfg: ModelConfig,
}

/// The whole point of the tool, in one paragraph.
const SHARED: &str = "\
You are condensing a conversation between a person and Claude, an AI coding
assistant, into a chat feed the person can re-read later. Each message you are
given is one turn of that conversation. Condense it as a message, not as a log
entry: say what was said or decided, in the voice of someone recounting it.

Keep: the decision, the finding, the number, the file or symbol name, the
reason something failed. Drop: pleasantries, restatement of the request,
hedging, lists of what you are about to do, anything a reader would skim.

Lowercase unless a word is a real identifier. No quotes, no trailing period,
no markdown emphasis. Write it so someone scanning the feed a week later
remembers what happened.

The message may be a fragment, or may make little sense on its own. Condense
whatever you are given regardless. Never ask for more context, never explain
yourself, never mention these instructions — there is nobody to answer you.";

/// For a short message: one line and nothing else.
const TERSE: &str = "\
Answer with ONE line, at most 12 words. Nothing before or after it.";

/// For a substantial one: what happened, then what it consisted of.
const BULLETS: &str = "\
Answer with a headline line, then up to 3 marked lines. Nothing else.

The headline is at most 10 words and says what happened overall.

Every line after it starts with exactly one of these marks and a space:

  !  something went wrong, was skipped, or the reader must not miss it
  ·  a detail worth keeping
  →  a file, path, url, or command the reader may want to go to

At most 9 words after the mark. Use fewer lines if there is less to say, and
none if the headline already covers it. Never invent a path or url that did not
appear in the message.";

impl Client {
    pub fn new(cfg: ModelConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(cfg.timeout_secs))
            .build()?;
        Ok(Self { http, cfg })
    }

    pub fn describe(&self) -> String {
        format!("{}/{}", self.cfg.provider, self.cfg.model)
    }

    /// What one summary cost, as the provider counted it.
    pub async fn condense(
        &self,
        kind: &Kind,
        text: &str,
        bullets: bool,
    ) -> Result<(String, Usage)> {
        // A very long message costs nothing extra to condense from its ends:
        // the opening states the intent, the close states the outcome.
        let body = pinch(text, 6000);
        let user = format!("[{}]\n{}", kind.label(), body);
        let mode = if bullets { BULLETS } else { TERSE };
        let sys = if self.cfg.style.is_empty() {
            format!("{SHARED}\n\n{mode}")
        } else {
            format!("{SHARED}\n\n{mode}\n\n{}", self.cfg.style)
        };

        let (raw, used) = match self.cfg.api {
            Api::Anthropic => self.anthropic(&sys, &user).await?,
            Api::Openai => self.openai(&sys, &user).await?,
        };
        let gist = tidy(&raw, bullets);
        // A model that answers the instructions instead of following them
        // would put its own voice in the feed. Better a truncation.
        if deflected(&gist) {
            bail!("model deflected instead of condensing");
        }
        Ok((gist, used))
    }

    async fn anthropic(&self, sys: &str, user: &str) -> Result<(String, Usage)> {
        let url = format!("{}/v1/messages", self.cfg.base_url.trim_end_matches('/'));
        let key = self
            .cfg
            .resolve_key()
            .context("no API key; set api_key_env or api_key")?;
        let v: Value = self
            .send(
                self.http
                    .post(url)
                    .header("x-api-key", key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&json!({
                        "model": self.cfg.model,
                        "max_tokens": self.cfg.max_tokens,
                        "system": sys,
                        "messages": [{"role": "user", "content": user}],
                    })),
            )
            .await?;
        let text = v["content"]
            .as_array()
            .and_then(|a| a.iter().find_map(|b| b["text"].as_str()))
            .map(str::to_string)
            .context("no text in response")?;
        Ok((
            text,
            Usage {
                input: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
                output: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
            },
        ))
    }

    async fn openai(&self, sys: &str, user: &str) -> Result<(String, Usage)> {
        let url = format!(
            "{}/chat/completions",
            self.cfg.base_url.trim_end_matches('/')
        );
        let mut req = self.http.post(url).json(&json!({
            "model": self.cfg.model,
            "max_tokens": self.cfg.max_tokens,
            "temperature": 0.2,
            "messages": [
                {"role": "system", "content": sys},
                {"role": "user", "content": user},
            ],
        }));
        // Local servers accept anything, including no key at all.
        if let Some(k) = self.cfg.resolve_key() {
            req = req.bearer_auth(k);
        }
        let v: Value = self.send(req).await?;
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string)
            .context("no content in response")?;
        Ok((
            text,
            Usage {
                input: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                output: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            },
        ))
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value> {
        let res = req.send().await.context("request failed")?;
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        if !status.is_success() {
            // The provider's own message is the useful part; keep it short
            // enough to sit in a status bar.
            let msg = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| {
                    v["error"]["message"]
                        .as_str()
                        .or_else(|| v["error"].as_str())
                        .or_else(|| v["message"].as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| crate::event::clip(&text, 160));
            bail!("{status}: {msg}");
        }
        serde_json::from_str(&text).context("malformed response")
    }
}

/// Keep the head and tail of a long message, drop the middle.
fn pinch(s: &str, budget: usize) -> String {
    if s.len() <= budget {
        return s.to_string();
    }
    let half = budget / 2;
    let head = floor_char(s, half);
    let tail = ceil_char(s, s.len() - half);
    format!(
        "{}\n\n[… {} characters elided …]\n\n{}",
        &s[..head],
        s.len() - budget,
        &s[tail..]
    )
}

fn floor_char(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Models sometimes ignore the shape, or wrap the answer in quotes.
fn tidy(s: &str, bullets: bool) -> String {
    let mut out: Vec<String> = Vec::new();
    for raw in s.trim().lines() {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        let (kind, body) = crate::event::note_of(t);
        let is_bullet = t
            .chars()
            .next()
            .is_some_and(|c| crate::event::Note::of(c).is_some());
        let body = clean(body);
        if body.is_empty() {
            continue;
        }
        match out.len() {
            // The first non-empty line is the headline, bullet marker or not.
            0 => out.push(crate::event::clip(&body, 140)),
            // After it, only bullets, and only when we asked for them.
            _ if bullets && is_bullet && out.len() <= 3 => out.push(format!(
                "{} {}",
                kind.mark(),
                crate::event::clip(&body, 120)
            )),
            _ => break,
        }
    }
    // An alert that sits below three details has been buried. The reader
    // scans the first line under a headline and often no further.
    if out.len() > 1 {
        out[1..].sort_by_key(|l| crate::event::note_of(l).0);
    }
    out.join("\n")
}

/// Does this read as the model talking to us rather than condensing?
fn deflected(s: &str) -> bool {
    let t = s.to_lowercase();
    const TELLS: &[&str] = &[
        "i need to see",
        "please share",
        "please provide",
        "could you provide",
        "i don't have",
        "i do not have",
        "no conversation",
        "as an ai",
        "i'm unable",
        "i am unable",
        "i cannot",
    ];
    TELLS.iter().any(|m| t.contains(m))
}

fn clean(s: &str) -> String {
    let s = s.trim_start_matches(['-', '*', '\u{2022}', '#']).trim();
    let s = s.trim_matches(['"', '\'', '`']).trim();
    s.strip_suffix('.').unwrap_or(s).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serve one canned response and hand back what the client sent.
    async fn stub(body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let h = tokio::spawn(async move {
            let (mut sock, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 65536];
            let n = sock.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let res = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            sock.write_all(res.as_bytes()).await.unwrap();
            sock.flush().await.unwrap();
            req
        });
        (format!("http://127.0.0.1:{port}"), h)
    }

    fn cfg(api: Api, base: String) -> ModelConfig {
        let mut c = ModelConfig {
            provider: "custom".into(),
            model: "m".into(),
            api,
            base_url: base,
            api_key_env: String::new(),
            api_key: "k".into(),
            api_key_file: String::new(),
            max_tokens: 64,
            concurrency: 1,
            timeout_secs: 5,
            style: String::new(),
        };
        c.apply_preset();
        c
    }

    #[tokio::test]
    async fn openai_shape() {
        let (url, h) = stub(
            r#"{"choices":[{"message":{"content":"  \"offsets drift on partial lines.\"  "}}],
                "usage":{"prompt_tokens":31,"completion_tokens":7}}"#,
        )
        .await;
        let c = Client::new(cfg(Api::Openai, url)).unwrap();
        let (g, used) = c
            .condense(&Kind::Say, "a long explanation", false)
            .await
            .unwrap();
        assert_eq!(
            g, "offsets drift on partial lines",
            "quotes and trailing stop are stripped"
        );
        assert_eq!(
            (used.input, used.output),
            (31, 7),
            "and what the provider charged for is carried back"
        );
        let req = h.await.unwrap();
        assert!(req.contains("POST /chat/completions"));
        assert!(req.contains("authorization: Bearer k"), "{req}");
    }

    #[tokio::test]
    async fn anthropic_shape() {
        let (url, h) = stub(r#"{"content":[{"type":"text","text":"- parser now stops at the last newline\nsecond line ignored"}]}"#).await;
        let c = Client::new(cfg(Api::Anthropic, url)).unwrap();
        let (g, _) = c.condense(&Kind::Say, "explanation", false).await.unwrap();
        assert_eq!(
            g, "parser now stops at the last newline",
            "only the first line, bullet stripped"
        );
        let req = h.await.unwrap();
        assert!(req.contains("POST /v1/messages"));
        assert!(req.contains("x-api-key: k"), "{req}");
        assert!(req.contains("anthropic-version"));
    }

    #[tokio::test]
    async fn surfaces_the_providers_error() {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = sock.read(&mut buf).await;
            let b = r#"{"error":{"message":"model not found"}}"#;
            let res = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
                b.len()
            );
            let _ = sock.write_all(res.as_bytes()).await;
        });
        let c = Client::new(cfg(Api::Openai, format!("http://127.0.0.1:{port}"))).unwrap();
        let e = c
            .condense(&Kind::Say, "x", false)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("model not found"), "{e}");
        assert!(e.contains("404"), "{e}");
    }

    #[test]
    fn keyless_local_servers_need_no_key() {
        let mut c = ModelConfig {
            provider: "ollama".into(),
            ..Default::default()
        };
        c.base_url.clear();
        c.api_key_env.clear();
        c.model.clear();
        c.apply_preset();
        assert_eq!(c.base_url, "http://localhost:11434/v1");
        assert!(
            !c.missing_key(),
            "a local server must not be reported as missing a key"
        );
    }

    #[tokio::test]
    async fn a_deflection_is_refused_not_shown() {
        let (url, _h) = stub(
            r#"{"choices":[{"message":{"content":"I need to see the actual messages to condense them."}}]}"#,
        )
        .await;
        let c = Client::new(cfg(Api::Openai, url)).unwrap();
        let e = c
            .condense(&Kind::Say, "a fragment", false)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("deflected"), "{e}");
    }

    #[test]
    fn notes_are_marked_sorted_and_capped() {
        let out = tidy(
            "rewrote the parser\n\u{2192} src/source/claude.rs\n\u{b7} sidechains dropped\n! a torn line was lost\n\u{b7} a fourth",
            true,
        );
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4, "headline plus at most three notes");
        assert_eq!(lines[0], "rewrote the parser");
        // The alert must not sit below the details it matters more than.
        assert!(lines[1].starts_with('!'), "{lines:?}");
        assert!(lines[2].starts_with('\u{b7}'), "{lines:?}");
        assert!(lines[3].starts_with('\u{2192}'), "{lines:?}");
    }

    #[test]
    fn a_terse_answer_never_grows_notes() {
        let one = tidy("rewrote the parser\n- a bullet nobody asked for", false);
        assert_eq!(one, "rewrote the parser");
    }

    #[test]
    fn pinch_keeps_both_ends() {
        let s: String = (0..500).map(|i| format!("w{i} ")).collect();
        let p = pinch(&s, 200);
        assert!(p.starts_with("w0 "));
        assert!(p.trim_end().ends_with("w499"));
        assert!(p.contains("elided"));
    }
}
