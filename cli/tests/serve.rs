//! The real binary over pipes, with a temporary HOME and a generated history file.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const US: char = '\u{1f}';
const BIN: &str = env!("CARGO_BIN_EXE_fuxx-completes");

fn command(home: &std::path::Path) -> Command {
    let mut c = Command::new(BIN);
    c.env_clear().env("HOME", home);
    c
}

struct Core {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
}

impl Core {
    fn start(home: &std::path::Path) -> Core {
        let mut child = command(home)
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout: ChildStdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Core {
            child,
            stdin,
            lines,
        }
    }

    fn send(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    fn recv(&self, within: Duration) -> Option<String> {
        self.lines.recv_timeout(within).ok()
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn hello(path: &str) -> String {
    format!("hello{US}1{US}zsh{US}5.9{US}zsh{US}{path}{US}autosuggestions")
}
fn suggest(id: u64, buffer: &str) -> String {
    format!("suggest{US}{id}{US}async{US}0{US}/{US}{buffer}")
}

#[test]
fn binary_answers_from_a_zsh_history_file() {
    let home = tempfile::tempdir().unwrap();
    let hist = home.path().join(".zsh_history");
    std::fs::write(&hist, "git status\ngit push origin main\n").unwrap();
    let mut core = Core::start(home.path());
    core.send(&hello(hist.to_str().unwrap()));
    assert_eq!(
        core.recv(Duration::from_secs(5)).as_deref(),
        Some(format!("ready{US}1").as_str())
    );
    // The history may load off the request path, so ask again until it has.
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut id = 1;
    let mut last = String::new();
    while Instant::now() < deadline {
        core.send(&suggest(id, "git p"));
        last = core.recv(Duration::from_secs(2)).expect("a reply");
        if last == format!("suggestion{US}{id}{US}ush origin main") {
            return;
        }
        id += 1;
        thread::sleep(Duration::from_millis(20));
    }
    panic!("never got the suggestion; last reply {last:?}");
}

#[test]
fn binary_echoes_ids() {
    let home = tempfile::tempdir().unwrap();
    let mut core = Core::start(home.path());
    core.send(&suggest(41, "x"));
    core.send(&suggest(42, "x"));
    assert_eq!(
        core.recv(Duration::from_secs(5)).as_deref(),
        Some(format!("suggestion{US}41{US}").as_str())
    );
    assert_eq!(
        core.recv(Duration::from_secs(5)).as_deref(),
        Some(format!("suggestion{US}42{US}").as_str())
    );
}

#[test]
fn binary_exits_zero_on_eof() {
    let home = tempfile::tempdir().unwrap();
    let mut core = Core::start(home.path());
    drop(core.stdin.take());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = core.child.try_wait().unwrap() {
            assert!(status.success(), "{status:?}");
            return;
        }
        assert!(Instant::now() < deadline, "did not exit on EOF");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn binary_without_serve_exits_2() {
    let home = tempfile::tempdir().unwrap();
    for args in [&[][..], &["report"][..], &["serve", "extra"][..]] {
        let out = command(home.path())
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "args {args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("usage"));
    }
}

#[test]
fn binary_writes_nothing_to_stderr_while_serving() {
    let home = tempfile::tempdir().unwrap();
    let mut core = Core::start(home.path());
    core.send(&hello("/nonexistent/hist"));
    core.send("garbage\u{1f}line");
    core.send(&suggest(1, "git"));
    core.send("bye");
    drop(core.stdin.take());
    let status = core.child.wait().unwrap();
    assert!(status.success());
    let mut err = String::new();
    core.child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    assert_eq!(err, "");
}
