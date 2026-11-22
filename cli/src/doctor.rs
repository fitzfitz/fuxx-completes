//! `fuxx-completes doctor` (`docs/design.md` §7): what is loaded and why, one fact per line.

use fuxx_completes_core::context::secret::Secret;
use fuxx_completes_core::context::store::{own_file_id, CONTEXT_DIR};
use fuxx_completes_core::context::{DataDir, DataSource, Key};
use std::fmt::Write;
use std::path::Path;

/// The report. `facts` are the `name=value` lines the zsh adapter's `fuxx-completes` function
/// passes with `--shell-facts` (`None` outside such a shell); `data` is the data folder.
pub fn report(facts: Option<&str>, data: Option<&DataDir>, version: &str) -> String {
    let mut out = format!("fuxx-completes {version}\n");
    let mut line = |label: &str, value: &str| {
        let _ = writeln!(out, "{label:<13}{value}");
    };
    let facts: Vec<(&str, &str)> = facts
        .unwrap_or("")
        .lines()
        .filter_map(|l| l.split_once('='))
        .collect();
    let get = |name: &str| facts.iter().find(|(k, _)| *k == name).map(|(_, v)| *v);

    if facts.is_empty() {
        line(
            "shell",
            "run this inside zsh or bash with fuxx-autosuggestions loaded for shell facts",
        );
    } else {
        line(
            "shell",
            &format!(
                "{} {}",
                get("shell").unwrap_or("?"),
                get("version").unwrap_or("?")
            ),
        );
        let state = match (get("state").unwrap_or(""), get("why").unwrap_or("")) {
            ("" | "starting", _) => "starting (no prompt yet)".to_string(),
            (s, "") => s.to_string(),
            (s, why) => format!("{s} ({why})"),
        };
        line("state", &state);
        if get("force") == Some("1") {
            line("force", "on (FUXX_AUTOSUGGESTIONS_FORCE=1)");
        }
        if let Some(file) = get("file").filter(|f| !f.is_empty()) {
            line("copy", file);
        }
        for (_, second) in facts.iter().filter(|(k, _)| *k == "second") {
            line("copy", &format!("second copy stayed off: {second}"));
        }
        for (_, key) in facts.iter().filter(|(k, _)| *k == "key") {
            let shown = match key.rsplit_once(' ') {
                Some((k, outcome)) => match outcome.strip_prefix("kept:") {
                    Some(widget) => format!("{k} kept ({widget})"),
                    None => format!("{k} {outcome}"),
                },
                None => key.to_string(),
            };
            line("keys", &shown);
        }
    }
    // The suggestion list's settings (M1c): `setting=<name> <value> <source>`.
    for name in ["list", "list_empty", "list_delay", "list_size"] {
        let Some(v) = facts
            .iter()
            .find(|(k, v)| *k == "setting" && v.split(' ').next() == Some(name))
            .map(|(_, v)| *v)
        else {
            continue;
        };
        let mut parts = v.split(' ').skip(1);
        let (value, source) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let label = match name {
            "list" => "list",
            "list_empty" => "list empty",
            "list_delay" => "list delay",
            _ => "list size",
        };
        let source = match source {
            "fuxx" => "fuxx Settings",
            other => other,
        };
        let value = if name == "list_delay" {
            format!("{value} s")
        } else {
            value.to_string()
        };
        line(label, &format!("{value} ({source})"));
    }
    line("host mode", "not built yet (M2)");
    match data {
        None => line("data", "none (no HOME)"),
        Some(d) => {
            let from = match d.source {
                DataSource::PluginData => "FUXX_PLUGIN_DATA",
                DataSource::Xdg => "XDG_DATA_HOME",
                DataSource::Home => "HOME",
            };
            line("data", &format!("{} (from {from})", d.root.display()));
            let secret = Secret::load(&d.root);
            line(
                "context",
                &context_line(
                    &d.root,
                    secret.as_ref().ok().and_then(Option::as_ref),
                    get("histfile"),
                ),
            );
            line(
                "secret",
                &match secret {
                    Ok(Some(_)) => "present".to_string(),
                    Ok(None) => "none yet".to_string(),
                    Err(e) => format!("unreadable ({e})"),
                },
            );
        }
    }
    if get("shglob") == Some("on") {
        line(
            "warning",
            "shglob is on: zsh's add-zle-hook-widget prints \"No such widget\" at each prompt (design §16)",
        );
    }
    out
}

/// `doctor --report` (M1d §4.5): the report inside a ```text fence, with the OS, the machine and
/// the terminal added, under a line asking the user to check it first. Nothing is sent or run.
pub fn report_for_posting(facts: Option<&str>, data: Option<&DataDir>, version: &str) -> String {
    let mut out =
        String::from("Check this before you post it: it shows your data folder's path.\n```text\n");
    out.push_str(&report(facts, data, version));
    let _ = writeln!(out, "{:<13}{}", "os", os_version());
    let _ = writeln!(out, "{:<13}{}", "arch", std::env::consts::ARCH);
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let program = match (var("TERM_PROGRAM"), var("TERM_PROGRAM_VERSION")) {
        (Some(p), Some(v)) => Some(format!("{p} {v}")),
        (p, _) => p,
    };
    let terminal: Vec<String> = program.into_iter().chain(var("TERM")).collect();
    let terminal = if terminal.is_empty() {
        "unknown".to_string()
    } else {
        terminal.join(", ")
    };
    let _ = writeln!(out, "{:<13}{terminal}", "terminal");
    out.push_str("```\n");
    out
}

/// The OS and its version, read from files (macOS's SystemVersion.plist, Linux's os-release);
/// nothing is run.
fn os_version() -> String {
    let os = std::env::consts::OS;
    if os == "macos" {
        let plist = std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist")
            .unwrap_or_default();
        let version = plist
            .split("<key>ProductVersion</key>")
            .nth(1)
            .and_then(|r| r.split("<string>").nth(1))
            .and_then(|r| r.split("</string>").next())
            .map(str::trim);
        return match version {
            Some(v) => format!("macOS {v}"),
            None => "macOS".to_string(),
        };
    }
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    release
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_else(|| os.to_string())
}

fn kb(bytes: u64) -> String {
    format!("{:.1} KB", bytes as f64 / 1024.0)
}

/// How many context files there are, their size, and this shell's own file.
fn context_line(root: &Path, secret: Option<&Secret>, histfile: Option<&str>) -> String {
    let dir = root.join(CONTEXT_DIR);
    let files: Vec<(String, u64)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            Key::from_hex(&name)?;
            Some((name, e.metadata().ok()?.len()))
        })
        .collect();
    if files.is_empty() {
        return "none yet".to_string();
    }
    let total: u64 = files.iter().map(|(_, n)| n).sum();
    let own = match (secret, histfile.filter(|h| !h.is_empty())) {
        (Some(s), Some(h)) => {
            let id = own_file_id(s, Path::new(h));
            match files.iter().find(|(n, _)| *n == id) {
                Some((_, n)) => format!("this shell's file {} of 4 MiB", kb(*n)),
                None => "this shell's file none yet".to_string(),
            }
        }
        _ => "this shell's file unknown".to_string(),
    };
    let s = if files.len() == 1 { "" } else { "s" };
    format!("{} file{s}, {}; {own}", files.len(), kb(total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuxx_completes_core::context::record::Record;
    use fuxx_completes_core::context::secret::Secret;
    use fuxx_completes_core::context::store::{append, own_file_id};
    use fuxx_completes_core::context::{DataDir, DataSource, Key};
    use std::path::{Path, PathBuf};

    fn data(root: &Path) -> DataDir {
        DataDir {
            root: root.to_path_buf(),
            source: DataSource::Xdg,
        }
    }

    fn has(report: &str, line: &str) -> bool {
        report.lines().any(|l| l == line)
    }

    const ON: &str = "shell=zsh\nversion=5.9\nstate=on\nwhy=\nfile=/p/a.plugin.zsh\ndata=\n\
                      key=emacs ^[[F bound\nkey=emacs ^[[4~ kept:beginning-of-line\n\
                      shglob=off\nhistfile=/h/.zsh_history\nforce=0\n";

    #[test]
    fn report_without_facts() {
        let dir = tempfile::tempdir().unwrap();
        let r = report(None, Some(&data(&dir.path().join("d"))), "0.0.5");
        assert_eq!(r.lines().next(), Some("fuxx-completes 0.0.5"));
        assert!(
            has(
                &r,
                "shell        run this inside zsh or bash with fuxx-autosuggestions loaded for shell facts"
            ),
            "{r}"
        );
        assert!(has(&r, "host mode    not built yet (M2)"), "{r}");
        let d = format!(
            "data         {} (from XDG_DATA_HOME)",
            dir.path().join("d").display()
        );
        assert!(has(&r, &d), "{r}");
        assert!(has(&r, "context      none yet"), "{r}");
        assert!(has(&r, "secret       none yet"), "{r}");
        assert!(!r.contains("warning"), "{r}");
        let r = report(None, None, "0.0.5");
        assert!(has(&r, "data         none (no HOME)"), "{r}");
    }

    #[test]
    fn report_with_bash_facts() {
        let facts = "shell=bash\nversion=5.3.0(1)-release\nstate=on\nwhy=\n\
                     key=emacs \\e[C bound\nhistfile=/h/.bash_history\n";
        let r = report(Some(facts), None, "1");
        assert!(has(&r, "shell        bash 5.3.0(1)-release"), "{r}");
        assert!(has(&r, "state        on"), "{r}");
        assert!(has(&r, "keys         emacs \\e[C bound"), "{r}");
    }

    #[test]
    fn report_with_facts_on() {
        let r = report(Some(ON), None, "1.2.3");
        assert!(has(&r, "shell        zsh 5.9"), "{r}");
        assert!(has(&r, "state        on"), "{r}");
        assert!(has(&r, "copy         /p/a.plugin.zsh"), "{r}");
        assert!(has(&r, "keys         emacs ^[[F bound"), "{r}");
        assert!(
            has(&r, "keys         emacs ^[[4~ kept (beginning-of-line)"),
            "{r}"
        );
        // Lines come in the documented order.
        let pos = |p: &str| r.find(p).unwrap();
        assert!(pos("shell ") < pos("state ") && pos("state ") < pos("copy "));
        assert!(pos("copy ") < pos("keys ") && pos("keys ") < pos("host mode"));
    }

    #[test]
    fn report_with_conflict_and_second_copy() {
        let facts = "shell=zsh\nversion=5.8.1\nstate=off\nwhy=conflict:zsh-autosuggestions\n\
                     file=/p/a.zsh\nsecond=/x/b.zsh\nsecond=/y/c.zsh\nforce=0\n";
        let r = report(Some(facts), None, "1");
        assert!(
            has(&r, "state        off (conflict:zsh-autosuggestions)"),
            "{r}"
        );
        assert!(
            has(&r, "copy         second copy stayed off: /x/b.zsh"),
            "{r}"
        );
        assert!(
            has(&r, "copy         second copy stayed off: /y/c.zsh"),
            "{r}"
        );
        let r = report(Some("state=\nforce=1\n"), None, "1");
        assert!(has(&r, "state        starting (no prompt yet)"), "{r}");
        assert!(
            has(&r, "force        on (FUXX_AUTOSUGGESTIONS_FORCE=1)"),
            "{r}"
        );
    }

    #[test]
    fn report_warns_on_shglob() {
        let r = report(Some("shell=zsh\nshglob=on\n"), None, "1");
        assert!(has(
            &r,
            "warning      shglob is on: zsh's add-zle-hook-widget prints \"No such widget\" at each prompt (design §16)"
        ), "{r}");
    }

    #[test]
    fn report_counts_context_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("d");
        let secret = Secret::load_or_create(&root).unwrap();
        let rec = Record {
            key: Key(1),
            folder: PathBuf::from("/p"),
            status: 0,
            time: 1,
        };
        let own = own_file_id(&secret, Path::new("/h/.zsh_history"));
        let other = own_file_id(&secret, Path::new("/h/.bash_history"));
        append(&root, &own, &vec![rec.clone(); 40]).unwrap();
        append(&root, &other, &vec![rec; 10]).unwrap();
        let len = |id: &str| {
            std::fs::metadata(root.join("context").join(id))
                .unwrap()
                .len()
        };
        let kb = |n: u64| format!("{:.1} KB", n as f64 / 1024.0);
        let r = report(Some(ON), Some(&data(&root)), "1");
        let want = format!(
            "context      2 files, {}; this shell's file {} of 4 MiB",
            kb(len(&own) + len(&other)),
            kb(len(&own))
        );
        assert!(has(&r, &want), "{want}\n{r}");
        assert!(has(&r, "secret       present"), "{r}");
    }

    #[test]
    fn report_lists_the_list_settings() {
        let facts = "shell=zsh\nversion=5.9\nstate=on\nsetting=list on fuxx\n\
                     setting=list_empty on shell\nsetting=list_delay 3 default\n\
                     setting=list_size 5 default\n";
        let r = report(Some(facts), None, "1");
        for want in [
            "list         on (fuxx Settings)",
            "list empty   on (shell)",
            "list delay   3 s (default)",
            "list size    5 (default)",
        ] {
            assert!(has(&r, want), "{want:?} not in\n{r}");
        }
        assert!(r.find("list         on").unwrap() < r.find("host mode").unwrap());
    }
}
