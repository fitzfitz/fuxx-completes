//! Lines the core never offers as they are (`docs/design.md` §5.4, M1d): lines that look like
//! they hold a secret are never suggested; dangerous commands are guarded until their dangerous
//! part is typed.

use crate::cd_target::{command_words, CmdWord};

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

/// How many bytes of `line` must be typed before it may be shown, when it is a dangerous
/// command (spec M1d §4.2): the command word and the dangerous flag or subcommand. `None`: not
/// dangerous.
pub fn danger_reach(line: &str) -> Option<usize> {
    let words = command_words(line);
    let words = strip_prefix_words(&words);
    let (cmd, args) = words.split_first()?;
    DANGERS.iter().find_map(|d| {
        let named = match d.command.strip_suffix('*') {
            Some(stem) => cmd.value.starts_with(stem),
            None => cmd.value == d.command,
        };
        if !named {
            return None;
        }
        // The subcommand words, as one run anywhere after the command (`git -C dir push`).
        let after = if d.sub.is_empty() {
            0
        } else {
            (0..args.len()).find(|&i| {
                args.len() - i >= d.sub.len()
                    && d.sub.iter().zip(&args[i..]).all(|(s, a)| a.value == *s)
            })? + d.sub.len()
        };
        let last = if after == 0 { cmd } else { &args[after - 1] };
        let rest = &args[after..];
        match d.trigger {
            Trigger::Word => Some(last.end),
            Trigger::Flag { short, long, reach } => rest
                .iter()
                .take_while(|a| a.value != "--")
                .find(|a| {
                    let v = a.value.as_str();
                    long.iter()
                        .any(|l| v == *l || v.strip_prefix(l).is_some_and(|r| r.starts_with('=')))
                        || (v.len() > 1
                            && v.starts_with('-')
                            && !v.starts_with("--")
                            && v[1..].chars().any(|c| short.contains(c)))
                })
                .map(|a| (a.start + reach).min(a.end)),
            Trigger::Args(seq) => (0..rest.len()).find_map(|i| {
                (rest.len() - i >= seq.len()
                    && seq.iter().zip(&rest[i..]).all(|(s, a)| a.value == *s))
                .then(|| rest[i].end)
            }),
        }
    })
}

/// What makes a listed command dangerous.
#[derive(Clone, Copy)]
enum Trigger {
    /// The command (and its subcommand words) alone; typed in full.
    Word,
    /// A short-option cluster holding one of `short`, or one of the `long` options (also with
    /// `=value`), before a `--`; `reach` characters of it must be typed.
    Flag {
        short: &'static str,
        long: &'static [&'static str],
        reach: usize,
    },
    /// These arguments in a row; typed up to the end of the first.
    Args(&'static [&'static str]),
}

struct Danger {
    /// The command word (`mkfs*`: any word starting with `mkfs`).
    command: &'static str,
    sub: &'static [&'static str],
    trigger: Trigger,
}

const fn flag(short: &'static str, long: &'static [&'static str], reach: usize) -> Trigger {
    Trigger::Flag { short, long, reach }
}

/// The dangerous list (spec M1d §4.2; **AI's call** on the exact entries).
const DANGERS: [Danger; 18] = [
    Danger {
        command: "rm",
        sub: &[],
        trigger: flag("rRf", &["--recursive", "--force"], 1),
    },
    Danger {
        command: "git",
        sub: &["push"],
        trigger: flag("f", &["--force", "--force-with-lease"], 1),
    },
    Danger {
        command: "git",
        sub: &["reset"],
        trigger: flag("", &["--hard"], 3),
    },
    Danger {
        command: "git",
        sub: &["clean"],
        trigger: flag("f", &["--force"], 1),
    },
    Danger {
        command: "git",
        sub: &["checkout"],
        trigger: Trigger::Args(&["."]),
    },
    Danger {
        command: "git",
        sub: &["restore"],
        trigger: Trigger::Args(&["."]),
    },
    Danger {
        command: "kubectl",
        sub: &["delete"],
        trigger: Trigger::Word,
    },
    Danger {
        command: "terraform",
        sub: &["apply"],
        trigger: Trigger::Word,
    },
    Danger {
        command: "terraform",
        sub: &["destroy"],
        trigger: Trigger::Word,
    },
    Danger {
        command: "helm",
        sub: &["uninstall"],
        trigger: Trigger::Word,
    },
    Danger {
        command: "docker",
        sub: &["system", "prune"],
        trigger: Trigger::Word,
    },
    Danger {
        command: "docker",
        sub: &["volume", "rm"],
        trigger: Trigger::Word,
    },
    Danger {
        command: "dd",
        sub: &[],
        trigger: Trigger::Word,
    },
    Danger {
        command: "mkfs*",
        sub: &[],
        trigger: Trigger::Word,
    },
    Danger {
        command: "chmod",
        sub: &[],
        trigger: flag("R", &["--recursive"], 1),
    },
    Danger {
        command: "chown",
        sub: &[],
        trigger: flag("R", &["--recursive"], 1),
    },
    Danger {
        command: "shutdown",
        sub: &[],
        trigger: Trigger::Word,
    },
    Danger {
        command: "reboot",
        sub: &[],
        trigger: Trigger::Word,
    },
];

/// sudo's and doas's options that take a value.
const PREFIX_VALUE_OPTIONS: [&str; 10] =
    ["-u", "-g", "-C", "-h", "-p", "-r", "-t", "-T", "-D", "-U"];

/// The words from the command word on: `NAME=value`, `sudo`/`doas` (with their options) and
/// `builtin`/`command` before it are skipped.
fn strip_prefix_words(words: &[CmdWord]) -> &[CmdWord] {
    let mut i = 0;
    while let Some(w) = words.get(i) {
        if w.assign || w.value == "builtin" || w.value == "command" {
            i += 1;
        } else if w.value == "sudo" || w.value == "doas" {
            i += 1;
            while let Some(o) = words.get(i).filter(|o| o.value.starts_with('-')) {
                i += 1;
                if o.value == "--" {
                    break;
                }
                if PREFIX_VALUE_OPTIONS.contains(&o.value.as_str()) {
                    i += 1;
                }
            }
        } else {
            break;
        }
    }
    &words[i.min(words.len())..]
}

/// True when `line`'s first simple command is on the dangerous list.
pub fn is_dangerous(line: &str) -> bool {
    danger_reach(line).is_some()
}

/// How many hidden lines one answer passes over before it gives up (final review: a history
/// full of them must not slow an answer past the budget).
pub const MAX_HIDDEN: usize = 1000;

/// True when `line` must not be offered for `buffer`: it looks like it holds a secret, or it is
/// dangerous and `buffer` hasn't reached its dangerous part yet.
pub fn hidden_for(line: &str, buffer: &str) -> bool {
    looks_secret(line) || danger_reach(line).is_some_and(|reach| buffer.len() < reach)
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

/// Name segments (split at `_`, `-` counted as `_`) that make a name secret, alone or as a pair.
const SECRET_WORDS: [&str; 5] = ["TOKEN", "SECRET", "PASSWORD", "PASSWD", "APIKEY"];
const SECRET_PAIRS: [(&str, &str); 3] = [("API", "KEY"), ("PRIVATE", "KEY"), ("ACCESS", "KEY")];
/// A last segment that names something about a secret, not the secret (`--password-file`,
/// `--secret-id`, `--password-stdin`, `--token-limit`).
const NOT_THE_SECRET: [&str; 8] = [
    "FILE", "PATH", "DIR", "STDIN", "ID", "NAME", "LIMIT", "TYPE",
];

/// A name holding a secret word as a whole segment (`HF_TOKEN`, `--api-key`), not as part of
/// one (`--max-tokens`, `TOKENIZERS_PARALLELISM`), and not a `NO_…` switch.
fn secret_name(name: &str) -> bool {
    let n = name
        .trim_start_matches('-')
        .to_ascii_uppercase()
        .replace('-', "_");
    let parts: Vec<&str> = n.split('_').filter(|p| !p.is_empty()).collect();
    let (Some(first), Some(last)) = (parts.first(), parts.last()) else {
        return false;
    };
    let holds = parts.iter().any(|p| SECRET_WORDS.contains(p))
        || parts
            .windows(2)
            .any(|w| SECRET_PAIRS.contains(&(w[0], w[1])));
    holds && *first != "NO" && !NOT_THE_SECRET.contains(last)
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
            // `--from-literal=password=x`: a `name=value` inside the value counts too.
            Some((name, value)) => {
                (secret_name(name) && literal(value))
                    || value
                        .split_once('=')
                        .is_some_and(|(n, v)| secret_name(n) && literal(v))
            }
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
            // Final review: names that only contain a secret word.
            "cargo run -- --max-tokens 4096",
            "llm --max-tokens 100 hi",
            "rg --token-limit 4",
            "aws secretsmanager get-secret-value --secret-id prod/db",
            "docker login --password-stdin ghcr.io",
            "docker login --username AWS --password-stdin 1234.dkr.ecr.us-east-1.amazonaws.com",
            "pg_dump --no-password mydb",
            "TOKENIZERS_PARALLELISM=false python x.py",
        ] {
            assert!(!looks_secret(l), "{l}");
        }
    }

    /// Final review: a secret name as a whole segment still counts, and a `name=value` inside a
    /// value too.
    #[test]
    fn secret_names_are_whole_segments() {
        for l in [
            "HF_TOKEN=hf_x make",
            "SECRET_KEY_BASE=abc rails s",
            "export OPENAI_API_KEY=abc",
            "tool --apikey=xyz",
            "kubectl create secret generic db --from-literal=password=hunter2",
        ] {
            assert!(looks_secret(l), "{l}");
        }
    }

    #[test]
    fn the_list_is_dangerous() {
        for l in [
            "rm -rf build",
            "rm -f x",
            "rm --recursive x",
            "rm build -Rf",
            "git push --force-with-lease",
            "git push origin main -f",
            "git reset --hard HEAD~1",
            "git clean -fdx",
            "git checkout -- .",
            "git checkout .",
            "git checkout main -- .",
            "git restore .",
            "kubectl delete pod x",
            "terraform apply",
            "terraform destroy",
            "helm uninstall r",
            "docker system prune -a",
            "docker volume rm v",
            "dd if=a of=b",
            "mkfs.ext4 /dev/x",
            "chmod -R 777 .",
            "chown -R me .",
            "shutdown -h now",
            "reboot",
        ] {
            assert!(is_dangerous(l), "{l}");
        }
    }

    #[test]
    fn safe_neighbours() {
        for l in [
            "rm file",
            "git push",
            "git push -u origin x",
            "git reset --soft HEAD~1",
            "git clean -n",
            "git checkout -b x",
            "git checkout -- src/a.rs",
            "git restore src/a.rs",
            "kubectl get pods",
            "terraform plan",
            "chmod +x a",
            "ddgr x",
            "echo rm -rf x",
            "cd x && rm -rf b",
        ] {
            assert!(!is_dangerous(l), "{l}");
        }
    }

    #[test]
    fn reach_is_the_dangerous_part() {
        assert_eq!(danger_reach("rm -rf build"), Some("rm -".len()));
        assert_eq!(danger_reach("git push --force"), Some("git push -".len()));
        assert_eq!(
            danger_reach("git reset --hard"),
            Some("git reset --h".len())
        );
        assert_eq!(
            danger_reach("kubectl delete pod x"),
            Some("kubectl delete".len())
        );
        assert_eq!(
            danger_reach("git checkout -- ."),
            Some("git checkout -- .".len())
        );
        assert_eq!(danger_reach("git checkout ."), Some("git checkout .".len()));
        assert_eq!(danger_reach("git restore ."), Some("git restore .".len()));
        assert_eq!(danger_reach("reboot"), Some(6));
        assert_eq!(danger_reach("mkfs.ext4 /dev/x"), Some("mkfs.ext4".len()));
    }

    /// Review Focus 2: prefixes and odd forms.
    #[test]
    fn prefixes_and_forms() {
        let l = "sudo -u root rm -rf x";
        assert_eq!(danger_reach(l), Some("sudo -u root rm -".len()));
        let l = "FOO=1 command rm -fr x";
        assert_eq!(danger_reach(l), Some("FOO=1 command rm -".len()));
        assert!(!is_dangerous("rm -- -rf"));
        assert!(is_dangerous("\\rm -rf x"));
        assert_eq!(
            danger_reach("git -C dir push -f"),
            Some("git -C dir push -".len())
        );
        assert!(is_dangerous("doas reboot"));
    }

    #[test]
    fn hidden_until_the_dangerous_part_is_typed() {
        let l = "git push --force";
        for b in ["", "g", "git push", "git push "] {
            assert!(hidden_for(l, b), "{b:?}");
        }
        assert!(!hidden_for(l, "git push -"));
        assert!(!hidden_for("git pull", ""));
        assert!(hidden_for("TOKEN=abc x", "TOKEN=abc x"));
    }
}
