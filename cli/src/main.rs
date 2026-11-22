use fuxx_completes_core::context::{data_dir_from_env, DataSource};
use fuxx_completes_core::protocol::Request;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

const USAGE: &str = "usage: fuxx-completes serve | forget | doctor [--report] | why [--cwd DIR] [--histfile FILE] [--] LINE";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args[..] {
        ["serve"] => {
            // A write error (the adapter went away) ends the core quietly; nothing is printed
            // while serving.
            let _ = fuxx_completes::serve::serve(std::io::stdout().lock());
            ExitCode::SUCCESS
        }
        ["forget"] => forget(),
        ["doctor"] => doctor(None, false),
        ["doctor", "--report"] => doctor(None, true),
        ["doctor", "--shell-facts", facts] => doctor(Some(facts), false),
        ["doctor", "--shell-facts", facts, "--report"] => doctor(Some(facts), true),
        ["why", ref rest @ ..] => match why_args(rest) {
            Some((cwd, histfile, line)) => why(cwd, histfile, &line),
            None => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

/// `[--cwd DIR] [--histfile FILE] [--] LINE…`: the folder (default: this process's), the history
/// file, and the line (the remaining words joined by spaces). `None` when the line is missing or
/// an option has no value.
fn why_args(args: &[&str]) -> Option<(PathBuf, Option<PathBuf>, String)> {
    let mut cwd = None;
    let mut histfile = None;
    let mut i = 0;
    while let Some(&a) = args.get(i) {
        match a {
            "--cwd" => cwd = Some(PathBuf::from(args.get(i + 1)?)),
            "--histfile" => histfile = Some(PathBuf::from(args.get(i + 1)?)),
            "--" => {
                i += 1;
                break;
            }
            _ => break,
        }
        i += 2;
    }
    let line = args.get(i..)?.join(" ");
    if line.is_empty() {
        return None;
    }
    let cwd = match cwd {
        Some(c) => c,
        None => std::env::current_dir().ok()?,
    };
    Some((cwd, histfile.filter(|h| !h.as_os_str().is_empty()), line))
}

/// `why` (`docs/design.md` §7): loads like `serve` (history, context, projects), writes nothing.
fn why(cwd: PathBuf, histfile: Option<PathBuf>, line: &str) -> ExitCode {
    let mut session = fuxx_completes::session::Session::for_why(
        data_dir_from_env(),
        std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from),
    );
    session.handle(Request::Hello {
        protocol: 1,
        shell: "zsh".into(),
        shell_version: String::new(),
        history_format: "zsh".into(),
        history_path: histfile,
        libraries: vec!["autosuggestions".into()],
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    print!("{}", session.explain(line, &cwd, deadline));
    ExitCode::SUCCESS
}

/// Deletes the context files and the secret (`docs/design.md` §5.7). fuxx's plugin folder is
/// kept (fuxx owns it); our own folder goes too when nothing else is in it.
fn forget() -> ExitCode {
    let Some(data) = data_dir_from_env() else {
        eprintln!("fuxx-completes: no data folder (HOME is not set)");
        return ExitCode::from(1);
    };
    let root = &data.root;
    let remove_root = data.source != DataSource::PluginData;
    match fuxx_completes_core::context::store::forget(root, remove_root) {
        Ok(true) => println!("Removed the context files in {}.", root.display()),
        Ok(false) => println!("Nothing to remove in {}.", root.display()),
        Err(e) => {
            eprintln!(
                "fuxx-completes: could not remove the context files in {}: {e}",
                root.display()
            );
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

fn doctor(facts: Option<&str>, posting: bool) -> ExitCode {
    let data = data_dir_from_env();
    let version = env!("CARGO_PKG_VERSION");
    let text = if posting {
        fuxx_completes::doctor::report_for_posting(facts, data.as_ref(), version)
    } else {
        fuxx_completes::doctor::report(facts, data.as_ref(), version)
    };
    print!("{text}");
    ExitCode::SUCCESS
}
