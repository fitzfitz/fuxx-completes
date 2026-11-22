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
