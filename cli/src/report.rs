//! `fuxx-completes report` (design §5.5, §5.7): one record for a command a shell that can't keep
//! a `serve` process (fish) ran. It loads no history: the record's key needs only the command's
//! text and the secret. Never prints; a busy lock or any error drops the record silently.

use fuxx_completes_core::context::record::Record;
use fuxx_completes_core::context::secret::Secret;
use fuxx_completes_core::context::store::{append, own_file_id, trim_to_cap, AppendError};
use fuxx_completes_core::context::DataDir;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// What `report` was asked.
#[derive(Debug, PartialEq, Eq)]
pub struct ReportArgs {
    pub cwd: PathBuf,
    pub status: i32,
    pub time: Option<i64>,
    pub histfile: PathBuf,
    pub command: String,
}

/// `--cwd DIR --status N [--time SECS] --histfile FILE [--] COMMAND…` (the remaining words joined
/// by spaces). `None` for a missing or bad value or an empty command.
pub fn parse(args: &[&str]) -> Option<ReportArgs> {
    let (mut cwd, mut status, mut time, mut histfile) = (None, None, None, None);
    let mut i = 0;
    while let Some(&a) = args.get(i) {
        let value = || args.get(i + 1).copied();
        match a {
            "--cwd" => cwd = Some(PathBuf::from(value()?)),
            "--status" => status = Some(value()?.parse::<i32>().ok()?),
            "--time" => time = Some(value()?.parse::<i64>().ok()?),
            "--histfile" => histfile = Some(PathBuf::from(value()?)),
            "--" => {
                i += 1;
                break;
            }
            _ => break,
        }
        i += 2;
    }
    let command = args.get(i..)?.join(" ");
    let histfile = histfile.filter(|h| !h.as_os_str().is_empty())?;
    if command.is_empty() {
        return None;
    }
    Some(ReportArgs {
        cwd: cwd?,
        status: status?,
        time,
        histfile,
        command,
    })
}

/// How long a busy lock (a compaction elsewhere) is retried before the record is dropped.
const LOCK_WAIT: Duration = Duration::from_millis(100);

/// Appends the record to this history file's own context file and trims the file past its cap.
pub fn report(data: Option<DataDir>, args: ReportArgs) {
    let Some(data) = data else { return };
    let Ok(secret) = Secret::load_or_create(&data.root) else {
        return;
    };
    let id = own_file_id(&secret, &args.histfile);
    let time = args.time.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    });
    let record = Record {
        key: secret.key_of(&args.command),
        folder: args.cwd,
        status: args.status,
        time,
    };
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        match append(&data.root, &id, std::slice::from_ref(&record)) {
            Ok(()) => break,
            Err(AppendError::Busy) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return,
        }
    }
    let _ = trim_to_cap(&data.root, &id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_form() {
        let a = parse(&[
            "--cwd",
            "/x",
            "--status",
            "1",
            "--time",
            "5",
            "--histfile",
            "/h",
            "--",
            "make",
            "test",
        ])
        .unwrap();
        assert_eq!(
            a,
            ReportArgs {
                cwd: "/x".into(),
                status: 1,
                time: Some(5),
                histfile: "/h".into(),
                command: "make test".into()
            }
        );
        assert!(parse(&["--cwd", "/x", "--status", "1", "--histfile", "", "--", "ls"]).is_none());
        assert!(parse(&["--cwd", "/x", "--status", "-1", "--histfile", "/h", "ls"]).is_some());
    }
}
