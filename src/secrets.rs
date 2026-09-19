//! Spotting a credential before it is sent anywhere.
//!
//! A transcript is full of things nobody meant to write down: a key echoed by
//! a shell, a token pasted into a prompt, a private key read by mistake. Most
//! of a conversation never leaves the machine — tool calls are summarised by
//! rule and short messages stand as they are — but prose long enough to want
//! summarising is sent to a model, and that is the one path out.
//!
//! So this refuses rather than redacts. Scrubbing a secret out of arbitrary
//! text is not a problem anybody has solved, and a redaction that misses is
//! worse than not sending at all: it looks safe. A message that holds one is
//! summarised locally instead, and says so.

/// What was found, in words fit to show somebody.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found(pub &'static str);

/// Key shapes distinctive enough to name.
const SHAPES: &[(&str, &str)] = &[
    ("sk-ant-", "an Anthropic key"),
    ("sk-or-v1-", "an OpenRouter key"),
    ("sk-proj-", "an OpenAI key"),
    ("github_pat_", "a GitHub token"),
    ("ghp_", "a GitHub token"),
    ("gho_", "a GitHub token"),
    ("ghs_", "a GitHub token"),
    ("ghu_", "a GitHub token"),
    ("glpat-", "a GitLab token"),
    ("xoxb-", "a Slack token"),
    ("xoxp-", "a Slack token"),
    ("xapp-", "a Slack token"),
    ("AKIA", "an AWS access key"),
    ("ASIA", "an AWS session key"),
    ("AIza", "a Google API key"),
    ("dop_v1_", "a DigitalOcean token"),
    ("shpat_", "a Shopify token"),
    ("-----BEGIN", "a private key"),
];

/// Words that introduce a secret, when what follows looks like one.
const NAMES: &[&str] = &[
    "api_key",
    "apikey",
    "api-key",
    "secret",
    "password",
    "passwd",
    "token",
    "auth_token",
    "access_key",
    "private_key",
    "credential",
];

/// Below this, a random-looking run is more likely an id than a key.
const MIN_LEN: usize = 20;

/// Does this text hold something that must not be sent?
pub fn found_in(text: &str) -> Option<Found> {
    for (shape, what) in SHAPES {
        if let Some(at) = text.find(shape) {
            // A bare mention of the shape is not a key. Something has to
            // follow it that looks like one.
            let rest = &text[at + shape.len()..];
            if *shape == "-----BEGIN" {
                if rest.contains("PRIVATE KEY") {
                    return Some(Found(what));
                }
                continue;
            }
            if secretish(rest) >= MIN_LEN {
                return Some(Found(what));
            }
        }
    }
    named(text)
}

/// How many characters of key-shaped text start here.
fn secretish(s: &str) -> usize {
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .count()
}

/// `API_KEY=...`, `token: "..."` — a name that introduces a secret, followed
/// by something long enough to be one.
fn named(text: &str) -> Option<Found> {
    let lower = text.to_lowercase();
    for name in NAMES {
        let mut from = 0;
        while let Some(at) = lower[from..].find(name) {
            let i = from + at;
            from = i + name.len();
            // Past the closing quote of a JSON name, the separator, and the
            // opening quote of the value: `"token": "..."` as readily as
            // `token=...`.
            let rest = text[from..]
                .trim_start()
                .trim_start_matches(['"', '\'', '`'])
                .trim_start();
            let Some(rest) = rest.strip_prefix(['=', ':']) else {
                continue;
            };
            let rest = rest
                .trim_start()
                .trim_start_matches(['"', '\'', '`'])
                .trim_start();
            let run = secretish(rest);
            if run < MIN_LEN {
                continue;
            }
            // A placeholder is not a secret, and neither is an environment
            // variable being read rather than written.
            let value = &rest[..run];
            if value.starts_with('$')
                || value.chars().all(|c| c == 'x' || c == 'X')
                || value.to_lowercase().contains("your")
                || value.to_lowercase().contains("example")
                || value.to_lowercase().contains("redacted")
            {
                continue;
            }
            if mixed(value) {
                return Some(Found("something that looks like a credential"));
            }
        }
    }
    None
}

/// Keys mix cases or digits. Prose does not, over twenty characters.
fn mixed(s: &str) -> bool {
    let digits = s.chars().filter(char::is_ascii_digit).count();
    let upper = s.chars().filter(|c| c.is_ascii_uppercase()).count();
    let lower = s.chars().filter(|c| c.is_ascii_lowercase()).count();
    (digits > 0 && (upper > 0 || lower > 0)) || (upper > 2 && lower > 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_shapes_are_recognised() {
        for (text, what) in [
            (
                "found it: sk-or-v1-82d7aa11bb22cc33dd44ee55ff66",
                "an OpenRouter key",
            ),
            (
                "export ANTHROPIC_API_KEY=sk-ant-api03-AbCdEf0123456789xyz",
                "an Anthropic key",
            ),
            ("ghp_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8", "a GitHub token"),
            ("AKIAIOSFODNN7EXAMPLEKEY123", "an AWS access key"),
            ("-----BEGIN OPENSSH PRIVATE KEY-----", "a private key"),
        ] {
            assert_eq!(found_in(text).map(|f| f.0), Some(what), "{text}");
        }
    }

    #[test]
    fn a_name_and_a_long_random_value_counts() {
        assert!(found_in(r#"{"token": "aB3dE5fG7hJ9kL1mN3pQ5rS7"}"#).is_some());
        assert!(found_in("password=Tr0ub4dor3AndSomeMoreHere").is_some());
    }

    #[test]
    fn talking_about_keys_is_not_a_key() {
        // The commonest false positive, and this whole conversation is made
        // of it: writing about the shape of a key without one present.
        for text in [
            "keys that start with sk-or-v1- come from OpenRouter",
            "moan looks for $ANTHROPIC_API_KEY in the environment",
            "set api_key = \"your-key-here\" in the config",
            "api_key = $OPENROUTER_API_KEY",
            "token: xxxxxxxxxxxxxxxxxxxxxxxx",
            "the password field was empty",
            "grep -o 'sk-or-v1-[A-Za-z0-9]*' ~/.config/namesync/env",
        ] {
            assert_eq!(found_in(text), None, "{text:?} is talk, not a key");
        }
    }

    #[test]
    fn ordinary_prose_is_left_alone() {
        for text in [
            "the parser now stops at the last newline, so a torn line is re-read",
            "PR 228 fixes the script column, and the tests would have caught it",
            "a1b2c3d4 is the commit",
        ] {
            assert_eq!(found_in(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_short_run_is_an_identifier_not_a_secret() {
        assert_eq!(found_in("token=abc123"), None);
        assert_eq!(found_in("sk-ant-short"), None);
    }
}
