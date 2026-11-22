//! Lines the core never offers as they are (`docs/design.md` §5.4, M1d): lines that look like
//! they hold a secret are never suggested; dangerous commands are guarded until their dangerous
//! part is typed.

/// True when `line` looks like it holds a secret (spec M1d §4.1): a known token shape, an
/// `Authorization:`/`Bearer` value, a URL with a password, a secret-named value, or one long
/// random-looking word. Such a line is dropped, never masked.
pub fn looks_secret(line: &str) -> bool {
    line.contains("-----BEGIN")
        || has_token_shape(line)
        || has_auth_value(line)
        || has_url_password(line)
        || has_secret_name(line)
        || has_random_word(line)
}

/// Known token prefixes (at a word start), the characters their tail may hold, and the
/// shortest tail that counts (AKIA/ASIA: exactly 16).
/// A token prefix, the characters its tail may hold, and the shortest tail that counts.
type TokenShape = (&'static str, fn(u8) -> bool, usize);

const TOKENS: [TokenShape; 13] = [
    ("ghp_", alnum, 30),
    ("gho_", alnum, 30),
    ("ghu_", alnum, 30),
    ("ghs_", alnum, 30),
    ("ghr_", alnum, 30),
    ("github_pat_", word, 20),
    ("glpat-", word_dash, 20),
    ("sk-", word_dash, 20),
    ("xoxa-", alnum_dash, 10),
    ("xoxb-", alnum_dash, 10),
    ("xoxp-", alnum_dash, 10),
    ("xoxs-", alnum_dash, 10),
    ("AIza", word_dash, 35),
];

fn alnum(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}
fn word(b: u8) -> bool {
    alnum(b) || b == b'_'
}
fn word_dash(b: u8) -> bool {
    word(b) || b == b'-'
}
fn alnum_dash(b: u8) -> bool {
    alnum(b) || b == b'-'
}
fn upper_digit(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit()
}

/// How many bytes from `start` satisfy `ok`.
fn run_len(bytes: &[u8], start: usize, ok: impl Fn(u8) -> bool) -> usize {
    bytes[start.min(bytes.len())..]
        .iter()
        .take_while(|&&b| ok(b))
        .count()
}

fn has_token_shape(line: &str) -> bool {
    let b = line.as_bytes();
    (0..b.len())
        .filter(|&i| i == 0 || !alnum(b[i - 1]))
        .any(|i| {
            let rest = &b[i..];
            TOKENS.iter().any(|(p, ok, min)| {
                rest.starts_with(p.as_bytes()) && run_len(b, i + p.len(), ok) >= *min
            }) || ["AKIA", "ASIA", "npm_"].iter().any(|p| {
                if !rest.starts_with(p.as_bytes()) {
                    return false;
                }
                let start = i + p.len();
                if *p == "npm_" {
                    return run_len(b, start, alnum) >= 36;
                }
                run_len(b, start, upper_digit) == 16 && run_len(b, start, alnum) == 16
            })
        })
}

/// The run of value characters at `start`: up to a space or a quote.
fn value_at(line: &str, start: usize) -> &str {
    let rest = &line[start.min(line.len())..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '\'' || c == '"')
        .unwrap_or(rest.len());
    &rest[..end]
}

fn literal_secret_value(v: &str) -> bool {
    v.len() >= 16 && !v.starts_with('$')
}

/// `Authorization:` (and an optional scheme word) or `Bearer `, then 16+ characters that aren't
/// a `$` variable.
fn has_auth_value(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let mut found = false;
    for (i, _) in lower.match_indices("authorization:") {
        let mut at = i + "authorization:".len();
        at += run_len(lower.as_bytes(), at, |b| b == b' ');
        let scheme = run_len(lower.as_bytes(), at, |b| b.is_ascii_alphabetic());
        if scheme > 0 && lower.as_bytes().get(at + scheme) == Some(&b' ') {
            at += scheme + 1;
        }
        found |= literal_secret_value(value_at(line, at));
    }
    for (i, _) in lower.match_indices("bearer ") {
        found |= literal_secret_value(value_at(line, i + "bearer ".len()));
    }
    found
}

/// `scheme://user:password@host`, unless the password is a `$` variable.
fn has_url_password(line: &str) -> bool {
    line.match_indices("://").any(|(i, _)| {
        let authority = value_at(line, i + 3);
        let authority = &authority[..authority.find('/').unwrap_or(authority.len())];
        let Some((userinfo, _)) = authority.rsplit_once('@') else {
            return false;
        };
        userinfo
            .split_once(':')
            .is_some_and(|(_, pass)| !pass.is_empty() && !pass.starts_with('$'))
    })
}

const SECRET_NAMES: [&str; 8] = [
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "API_KEY",
    "APIKEY",
    "PRIVATE_KEY",
    "ACCESS_KEY",
];

fn secret_name(name: &str) -> bool {
    let n = name
        .trim_start_matches('-')
        .to_ascii_uppercase()
        .replace('-', "_");
    !n.is_empty()
        && SECRET_NAMES.iter().any(|s| n.contains(s))
        && !["FILE", "PATH", "DIR"].iter().any(|s| n.ends_with(s))
}

/// A literal value: not empty, and (inside one layer of quotes) not a `$` expansion or a
/// backtick command.
fn literal(value: &str) -> bool {
    let v = value.trim_start_matches(['\'', '"']);
    !v.is_empty() && !v.starts_with(['$', '`', '\'', '"'])
}

/// `NAME=value`, `--name=value` or `--name value` with a secret-looking name and a literal value.
fn has_secret_name(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    words
        .iter()
        .enumerate()
        .any(|(i, w)| match w.split_once('=') {
            Some((name, value)) => secret_name(name) && literal(value),
            None => {
                w.starts_with('-')
                    && secret_name(w)
                    && words
                        .get(i + 1)
                        .is_some_and(|v| literal(v) && !v.starts_with('-'))
            }
        })
}

/// One word of 32+ base64/hex characters that looks random: not all hex, a letter and a digit,
/// few separators, and ≥ 3.5 bits of entropy per character.
fn has_random_word(line: &str) -> bool {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || "+/=_-".contains(c)))
        .any(|w| {
            let b = w.as_bytes();
            let seps = b.iter().filter(|c| b"-_/".contains(c)).count();
            b.len() >= 32
                && !b.iter().all(|c| c.is_ascii_hexdigit() || *c == b'-')
                && b.iter().any(u8::is_ascii_alphabetic)
                && b.iter().any(u8::is_ascii_digit)
                && seps <= b.len() / 16
                && entropy(b) >= 3.5
        })
}

/// Shannon entropy in bits per byte.
fn entropy(b: &[u8]) -> f64 {
    let mut counts = [0u32; 256];
    for &c in b {
        counts[c as usize] += 1;
    }
    let n = b.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = f64::from(c) / n;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_shapes_are_secrets() {
        let lines = [
            format!("gh auth login --with-token ghp_{}", "aB3d".repeat(9)),
            format!("echo github_pat_11ABC{}", "x9_Y".repeat(6)),
            format!("curl -H 'PRIVATE-TOKEN: glpat-{}'", "Ab3-".repeat(5)),
            format!("export OPENAI_API_KEY=sk-proj-{}", "Qw9e".repeat(6)),
            format!("claude --key sk-ant-api03-{}", "Zx8_".repeat(6)),
            "slack-post xoxb-123-456-abcdef".to_string(),
            "aws configure set aws_access_key_id AKIAIOSFODNN7EXAMPLE".to_string(),
            format!("maps --key AIza{}", "Sy9_k".repeat(7)),
            format!("npm config set //r/:_authToken npm_{}", "aB3d".repeat(9)),
            "echo '-----BEGIN OPENSSH PRIVATE KEY-----'".to_string(),
        ];
        for l in &lines {
            assert!(looks_secret(l), "{l}");
        }
    }

    #[test]
    fn headers_and_urls() {
        assert!(looks_secret(
            "curl -H 'Authorization: Bearer abcdef0123456789xyz' https://api"
        ));
        assert!(looks_secret(
            "curl -H \"Authorization: token abcdef0123456789xyz\" x"
        ));
        assert!(looks_secret("http GET x Bearer abcdef0123456789xyz"));
        assert!(!looks_secret(
            "curl -H \"Authorization: Bearer $GITHUB_TOKEN\" https://api"
        ));
        assert!(looks_secret("git clone https://me:hunter2pass@host/r"));
        assert!(!looks_secret("git clone https://me:$PASS@host/r"));
        assert!(!looks_secret("curl https://host/a:b@c"));
        assert!(!looks_secret("git clone git@github.com:me/r.git"));
    }

    #[test]
    fn named_values() {
        for l in [
            "TOKEN=abc make",
            "mysql --password hunter2",
            "tool --api-key=xyz",
            "DB_PASSWD='x' run",
            "export GITHUB_TOKEN=0123abcd",
            "SECRET_KEY_BASE=abc rails s",
        ] {
            assert!(looks_secret(l), "{l}");
        }
        for l in [
            "TOKEN=$X make",
            "export TOKEN=$(pass x)",
            "TOKEN= make",
            "tool --password-file p",
            "tool --token-path p",
            "PASSWORD_STORE_DIR=~/x pass",
            "git commit -m \"fix token refresh\"",
        ] {
            assert!(!looks_secret(l), "{l}");
        }
    }

    #[test]
    fn high_entropy_words() {
        assert!(looks_secret(
            "aws-x --opt wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
        ));
        assert!(looks_secret(
            "deploy --with Zx8Qm2Lp9Rt4Vb7Nc1Hd6Jf3Kg5Ws0YeTuAiOoPq"
        ));
    }

    /// Review Focus 1: everyday lines a filter could mistake for secrets.
    #[test]
    fn everyday_lines_are_not_secrets() {
        for l in [
            "cargo test test_suggestion_list_follows_the_three_steps",
            "git checkout feature/JIRA-1234-add-the-thing-for-users",
            "docker rm 550e8400-e29b-41d4-a716-446655440000",
            "git show 3f2a9c1d8e7b6a5f4e3d2c1b0a9f8e7d6c5b4a39",
            "docker pull x@sha256:0b5c3e8f2a1d4c6b9e7f0a3d5c8b2e4f6a9c1d3e5f7b0a2c4e6d8f1a3b5c7e9d",
            "cd ~/Projects/some-really-long-folder-name/and/more",
            "git push",
            "rm file",
            "task-runner sk-x",
            "echo héllo ✓ sk-é ghp_é Bearer é Authorization: é https://é/é",
            "npm_config_registry=x npm i",
            "ls /usr/local/lib/node_modules/typescript/lib",
            "open https://github.com/fitzfitz/fuxx-completes/pull/12",
        ] {
            assert!(!looks_secret(l), "{l}");
        }
    }
}
