//! The latency budget of `docs/design.md` §3: a suggestion answer under 10 ms at p95 on a
//! 100,000-line history, here with 50,000 context records over 200 folders as well.
//! Run in release: `cargo test --release -p fuxx-completes --test latency -- --ignored --nocapture`.

use fuxx_completes::session::Session;
use fuxx_completes_core::context::record::Record;
use fuxx_completes_core::context::secret::Secret;
use fuxx_completes_core::context::store::{append, own_file_id};
use fuxx_completes_core::context::{DataDir, DataSource};
use fuxx_completes_core::protocol::{Mode, Request};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A small deterministic generator (no extra crate).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const TOOLS: [&str; 50] = [
    "git",
    "cargo",
    "npm",
    "make",
    "docker",
    "kubectl",
    "ls",
    "cd",
    "cat",
    "grep",
    "rg",
    "fd",
    "vim",
    "code",
    "open",
    "brew",
    "python3",
    "pip",
    "uv",
    "node",
    "yarn",
    "pnpm",
    "go",
    "rustup",
    "ssh",
    "scp",
    "curl",
    "wget",
    "tar",
    "zip",
    "unzip",
    "rm",
    "cp",
    "mv",
    "mkdir",
    "touch",
    "echo",
    "export",
    "source",
    "history",
    "jq",
    "sed",
    "awk",
    "find",
    "xargs",
    "tmux",
    "gh",
    "terraform",
    "aws",
    "just",
];
const SUBS: [&str; 10] = [
    "status", "build", "test", "run", "push", "pull", "log", "add", "install", "deploy",
];

/// About 5,000 distinct commands: 50 tools x 10 subcommands x 10 arguments.
fn distinct(rng: &mut Lcg) -> Vec<String> {
    let mut out = Vec::new();
    for t in TOOLS {
        for s in SUBS {
            for a in 0..10 {
                let arg = match rng.below(3) {
                    0 => format!("--flag-{a}"),
                    1 => format!("src/module_{a}/file.rs"),
                    _ => format!("target-{a}"),
                };
                out.push(format!("{t} {s} {arg}"));
            }
        }
    }
    out
}

#[test]
#[ignore = "a release-mode benchmark; run it with --release -- --ignored"]
fn suggest_p95_under_10_ms_on_100k_history() {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let mut rng = Lcg(0x5eed);
    let commands = distinct(&mut rng);

    let lines: Vec<&str> = (0..100_000)
        .map(|_| commands[rng.below(commands.len())].as_str())
        .collect();
    let hist = base.join(".zsh_history");
    std::fs::write(&hist, lines.join("\n") + "\n").unwrap();

    let root = base.join("data");
    let secret = Secret::load_or_create(&root).unwrap();
    let folders: Vec<PathBuf> = (0..200)
        .map(|i| PathBuf::from(format!("/w/proj{i}")))
        .collect();
    let records: Vec<Record> = (0..50_000)
        .map(|i| Record {
            key: secret.key_of(lines[rng.below(lines.len())]),
            folder: folders[rng.below(folders.len())].clone(),
            status: if rng.below(10) == 0 { 1 } else { 0 },
            time: i,
        })
        .collect();
    append(&root, &own_file_id(&secret, &hist), &records).unwrap();

    let data = DataDir {
        root,
        source: DataSource::Xdg,
    };
    let mut session = Session::new(Some(data), None);
    let started = Instant::now();
    session.handle(Request::Hello {
        protocol: 1,
        shell: "zsh".into(),
        shell_version: "5.9".into(),
        history_format: "zsh".into(),
        history_path: Some(hist.clone()),
        libraries: vec!["autosuggestions".into()],
    });
    let suggest = |id: u64, buffer: String, cwd: PathBuf| Request::Suggest {
        id,
        mode: Mode::Async,
        cursor: buffer.chars().count(),
        cwd,
        buffer,
        cdpath: Some(Vec::new()),
    };
    while !session.history_loaded() {
        session.handle(suggest(0, "g".into(), PathBuf::from("/")));
        std::thread::sleep(Duration::from_millis(1));
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "history never loaded"
        );
    }
    println!("hello to loaded: {:?}", started.elapsed());

    let mut times = Vec::with_capacity(2000);
    for id in 1..=2000u64 {
        let line = lines[rng.below(lines.len())];
        let prefix: String = line.chars().take(1 + rng.below(6)).collect();
        let cwd = folders[rng.below(folders.len())].clone();
        let req = suggest(id, prefix, cwd);
        let t = Instant::now();
        session.handle(req);
        times.push(t.elapsed());
    }
    times.sort();
    let p50 = times[times.len() / 2];
    let p95 = times[times.len() * 95 / 100];
    let max = times[times.len() - 1];
    println!("suggest over 2000 requests: p50 {p50:?}, p95 {p95:?}, max {max:?}");
    assert!(p95 < Duration::from_millis(10), "p95 {p95:?}");
}

/// M1b: the same 100,000-line history, now in a project folder (package.json with 200 scripts,
/// a Makefile with 200 targets, Cargo.toml). The first request in the cold folder answers from
/// history while detection runs; once warm, answers that use project candidates stay under 10 ms.
#[test]
#[ignore = "a release-mode benchmark; run it with --release -- --ignored"]
fn project_suggest_p95_under_10_ms_warm() {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let mut rng = Lcg(0x5eed);
    let commands = distinct(&mut rng);
    let lines: Vec<&str> = (0..100_000)
        .map(|_| commands[rng.below(commands.len())].as_str())
        .collect();
    let hist = base.join(".zsh_history");
    std::fs::write(&hist, lines.join("\n") + "\n").unwrap();

    let proj = base.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join(".git"), "").unwrap();
    let scripts: Vec<String> = (0..200).map(|i| format!("\"s{i}\":\"x\"")).collect();
    std::fs::write(
        proj.join("package.json"),
        format!("{{\"scripts\":{{{}}}}}", scripts.join(",")),
    )
    .unwrap();
    let targets: String = (0..200).map(|i| format!("t{i}:\n\techo\n")).collect();
    std::fs::write(proj.join("Makefile"), targets).unwrap();
    std::fs::write(proj.join("Cargo.toml"), "[package]\nname = \"p\"\n").unwrap();

    let mut session = Session::new(None, None);
    session.handle(Request::Hello {
        protocol: 1,
        shell: "zsh".into(),
        shell_version: "5.9".into(),
        history_format: "zsh".into(),
        history_path: Some(hist.clone()),
        libraries: vec!["autosuggestions".into()],
    });
    let suggest = |id: u64, buffer: &str| Request::Suggest {
        id,
        mode: Mode::Async,
        cursor: buffer.chars().count(),
        cwd: proj.clone(),
        buffer: buffer.into(),
        cdpath: Some(Vec::new()),
    };
    let started = Instant::now();
    // Load the history from another folder, so the project folder is still cold below.
    let elsewhere = |buffer: &str| Request::Suggest {
        id: 0,
        mode: Mode::Async,
        cursor: buffer.chars().count(),
        cwd: base.clone(),
        buffer: buffer.into(),
        cdpath: Some(Vec::new()),
    };
    while !session.history_loaded() {
        session.handle(elsewhere("zz"));
        std::thread::sleep(Duration::from_millis(1));
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "history never loaded"
        );
    }
    let t = Instant::now();
    session.handle(suggest(1, "npm ru"));
    println!(
        "first request in the cold project folder: {:?}",
        t.elapsed()
    );
    let warm = Instant::now();
    loop {
        // Warm: the first `npm run` project command (nothing ran here, so no step 1).
        let r = session.handle(suggest(2, "npm ru"));
        if format!("{r:?}").contains("Some(\"n s0\")") {
            break;
        }
        assert!(warm.elapsed() < Duration::from_secs(5), "never warm: {r:?}");
        std::thread::sleep(Duration::from_millis(1));
    }
    println!("cold to warm: {:?}", warm.elapsed());

    let prefixes: Vec<String> = (0..200)
        .flat_map(|i| [format!("npm run s{i}"), format!("make t{i}")])
        .chain(["cargo b".to_string(), "c".to_string(), "g".to_string()])
        .collect();
    let mut times = Vec::with_capacity(2000);
    for id in 0..2000u64 {
        let p = &prefixes[rng.below(prefixes.len())];
        let cut: String = p.chars().take(1 + rng.below(p.len())).collect();
        let t = Instant::now();
        session.handle(suggest(10 + id, &cut));
        times.push(t.elapsed());
    }
    times.sort();
    let p50 = times[times.len() / 2];
    let p95 = times[times.len() * 95 / 100];
    let max = times[times.len() - 1];
    println!("warm project folder, 2000 requests: p50 {p50:?}, p95 {p95:?}, max {max:?}");
    assert!(p95 < Duration::from_millis(10), "p95 {p95:?}");
}

/// M1c: building the list of 5 (what `tick` does when the pause is over) on the 100,000-line
/// history, for an empty line and 1–6 typed characters.
#[test]
#[ignore = "a release-mode benchmark; run it with --release -- --ignored"]
fn list_p95_under_10_ms_on_100k_history() {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let mut rng = Lcg(0x5eed);
    let commands = distinct(&mut rng);
    let lines: Vec<&str> = (0..100_000)
        .map(|_| commands[rng.below(commands.len())].as_str())
        .collect();
    let hist = base.join(".zsh_history");
    std::fs::write(&hist, lines.join("\n") + "\n").unwrap();
    let mut session = Session::new(None, None);
    session.handle(Request::Hello {
        protocol: 1,
        shell: "zsh".into(),
        shell_version: "5.9".into(),
        history_format: "zsh".into(),
        history_path: Some(hist.clone()),
        libraries: vec!["autosuggestions".into()],
    });
    session.handle(Request::ListConfig {
        size: 5,
        delay: Duration::ZERO,
        empty: true,
    });
    let suggest = |id: u64, buffer: String| Request::Suggest {
        id,
        mode: Mode::Async,
        cursor: buffer.chars().count(),
        cwd: base.clone(),
        buffer,
        cdpath: Some(Vec::new()),
    };
    let started = Instant::now();
    while !session.history_loaded() {
        session.handle(suggest(0, "zz".into()));
        session.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(1));
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "history never loaded"
        );
    }
    let mut times = Vec::with_capacity(2000);
    for id in 1..=2000u64 {
        let line = lines[rng.below(lines.len())];
        let prefix: String = line.chars().take(rng.below(7)).collect();
        session.handle(suggest(id, prefix));
        let t = Instant::now();
        let list = session.tick(Instant::now() + Duration::from_secs(1));
        times.push(t.elapsed());
        assert!(list.is_some(), "no list for id {id}");
    }
    times.sort();
    let p50 = times[times.len() / 2];
    let p95 = times[times.len() * 95 / 100];
    let max = times[times.len() - 1];
    println!("list of 5 over 2000 requests: p50 {p50:?}, p95 {p95:?}, max {max:?}");
    assert!(p95 < Duration::from_millis(10), "p95 {p95:?}");
}

/// M1d: the worst case for the repo tier: the 200 folders are real folders of one repo, so every
/// request in a new folder merges the records of the 199 others (the 100,000-line history, 50,000
/// records); then the fix after a failure, worked out fresh each time.
#[test]
#[ignore = "a release-mode benchmark; run it with --release -- --ignored"]
fn repo_and_fix_p95_under_10_ms() {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let mut rng = Lcg(0x5eed);
    let commands = distinct(&mut rng);
    let lines: Vec<&str> = (0..100_000)
        .map(|_| commands[rng.below(commands.len())].as_str())
        .collect();
    let hist = base.join(".zsh_history");
    std::fs::write(&hist, lines.join("\n") + "\n").unwrap();
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let folders: Vec<PathBuf> = (0..200).map(|i| repo.join(format!("dir{i}"))).collect();
    for f in &folders {
        std::fs::create_dir_all(f).unwrap();
    }
    let root = base.join("data");
    let secret = Secret::load_or_create(&root).unwrap();
    let records: Vec<Record> = (0..50_000)
        .map(|i| Record {
            key: secret.key_of(lines[rng.below(lines.len())]),
            folder: folders[rng.below(folders.len())].clone(),
            status: if rng.below(10) == 0 { 1 } else { 0 },
            time: i,
        })
        .collect();
    append(&root, &own_file_id(&secret, &hist), &records).unwrap();
    let data = DataDir {
        root,
        source: DataSource::Xdg,
    };
    let mut session = Session::new(Some(data), None);
    session.handle(Request::Hello {
        protocol: 1,
        shell: "zsh".into(),
        shell_version: "5.9".into(),
        history_format: "zsh".into(),
        history_path: Some(hist.clone()),
        libraries: vec!["autosuggestions".into()],
    });
    let suggest = |id: u64, buffer: String, cwd: PathBuf| Request::Suggest {
        id,
        mode: Mode::Async,
        cursor: buffer.chars().count(),
        cwd,
        buffer,
        cdpath: Some(Vec::new()),
    };
    let started = Instant::now();
    while !session.history_loaded() {
        session.handle(suggest(0, "g".into(), PathBuf::from("/")));
        std::thread::sleep(Duration::from_millis(1));
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "history never loaded"
        );
    }
    let mut times = Vec::with_capacity(2000);
    for id in 1..=2000u64 {
        let line = lines[rng.below(lines.len())];
        let prefix: String = line.chars().take(1 + rng.below(6)).collect();
        let cwd = folders[rng.below(folders.len())].clone();
        let req = suggest(id, prefix, cwd);
        let t = Instant::now();
        session.handle(req);
        times.push(t.elapsed());
    }
    times.sort();
    let p50 = times[times.len() / 2];
    let p95 = times[times.len() * 95 / 100];
    let max = times[times.len() - 1];
    println!(
        "repo tier, a new folder each time, 2000 requests: p50 {p50:?}, p95 {p95:?}, max {max:?}"
    );
    assert!(p95 < Duration::from_millis(10), "p95 {p95:?}");

    let mut fixes = Vec::with_capacity(200);
    for id in 0..200u64 {
        let line = lines[rng.below(lines.len())];
        // A typo in the command word, a typo in the line, and a package manager.
        let failed = match id % 3 {
            0 => format!("x{line}"),
            1 => format!("{line}x"),
            _ => format!("yarn add p{id}"),
        };
        session.handle(Request::Prev {
            command: Some(failed),
            status: Some(127),
        });
        let cwd = folders[rng.below(folders.len())].clone();
        let t = Instant::now();
        session.handle(suggest(10_000 + id, String::new(), cwd));
        fixes.push(t.elapsed());
    }
    fixes.sort();
    let p50 = fixes[fixes.len() / 2];
    let p95 = fixes[fixes.len() * 95 / 100];
    let max = fixes[fixes.len() - 1];
    println!("fix after a failure, 200 requests: p50 {p50:?}, p95 {p95:?}, max {max:?}");
    assert!(p95 < Duration::from_millis(10), "p95 {p95:?}");
}

/// M2: bash's explicit keys wait for a `sync` answer at most 50 ms (design §3). On a 100,000-entry
/// timestamped bash history, 2,000 `sync` requests through the real serve loop (pipes in and out,
/// timed from the write to the reply line) stay under 10 ms at p95 and 50 ms at most.
#[test]
#[ignore = "a release-mode benchmark; run it with --release -- --ignored"]
fn sync_requests_on_a_bash_history() {
    use std::io::{BufRead, BufReader, Write};

    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let mut rng = Lcg(0xba5);
    let commands = distinct(&mut rng);
    let lines: Vec<&str> = (0..100_000)
        .map(|_| commands[rng.below(commands.len())].as_str())
        .collect();
    let mut text = String::new();
    for (i, l) in lines.iter().enumerate() {
        text.push_str(&format!("#{}\n{l}\n", 1_700_000_000 + i));
    }
    let hist = base.join(".bash_history");
    std::fs::write(&hist, text).unwrap();

    let (in_r, mut in_w) = std::io::pipe().unwrap();
    let (out_r, out_w) = std::io::pipe().unwrap();
    let server = std::thread::spawn(move || {
        fuxx_completes::serve::serve_with(Session::new(None, None), BufReader::new(in_r), out_w)
    });
    let mut replies = BufReader::new(out_r);
    let us = '\u{1f}';
    let mut ask = |line: String| -> (Duration, String) {
        let t = Instant::now();
        in_w.write_all(line.as_bytes()).unwrap();
        in_w.flush().unwrap();
        let mut reply = String::new();
        replies.read_line(&mut reply).unwrap();
        (t.elapsed(), reply)
    };
    let started = Instant::now();
    let (_, ready) = ask(format!(
        "hello{us}1{us}bash{us}5.3.0(1)-release{us}bash{us}{}{us}autosuggestions\n",
        hist.display()
    ));
    assert!(ready.starts_with("ready"), "{ready:?}");
    let newest = lines[lines.len() - 1];
    loop {
        let (_, r) = ask(format!(
            "suggest{us}0{us}sync{us}2{us}/{us}{}\n",
            &newest[..2]
        ));
        if !r.trim_end().ends_with(us) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "history never loaded"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    println!("hello to loaded: {:?}", started.elapsed());

    let mut times = Vec::with_capacity(2000);
    for id in 1..=2000u64 {
        let line = lines[rng.below(lines.len())];
        let prefix: String = line.chars().take(1 + rng.below(6)).collect();
        let n = prefix.chars().count();
        let (t, reply) = ask(format!("suggest{us}{id}{us}sync{us}{n}{us}/{us}{prefix}\n"));
        assert!(
            reply.starts_with(&format!("suggestion{us}{id}{us}")),
            "{reply:?}"
        );
        times.push(t);
    }
    times.sort();
    let p50 = times[times.len() / 2];
    let p95 = times[times.len() * 95 / 100];
    let max = times[times.len() - 1];
    println!(
        "bash sync suggest over 2000 requests (round trip): p50 {p50:?}, p95 {p95:?}, max {max:?}"
    );
    drop(in_w);
    server.join().unwrap().unwrap();
    assert!(p95 < Duration::from_millis(10), "p95 {p95:?}");
    assert!(max < Duration::from_millis(50), "max {max:?}");
}
