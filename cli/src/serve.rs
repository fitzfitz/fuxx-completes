//! The `serve` loop: one request per line in, replies out, each flushed at once. Lines are read
//! on a thread, so the loop can also wake when a pushed list is due (M1c) without any input.

use crate::session::Session;
use fuxx_completes_core::protocol::{decode_request, encode_response, Request, Response};
use std::io::{BufRead, ErrorKind, Write};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Instant;

/// `serve_with` on a session set up from this process's environment, reading standard input.
pub fn serve(output: impl Write) -> std::io::Result<()> {
    serve_with(
        Session::from_env(),
        std::io::BufReader::new(std::io::stdin()),
        output,
    )
}

/// Reads LF-terminated lines until EOF or `bye`; writes and flushes each reply at once, and a
/// pushed list when its pause is over. Lines that don't decode (including invalid UTF-8) are
/// skipped. A closed output pipe, or a read error, ends the loop quietly with `Ok`.
pub fn serve_with(
    mut session: Session,
    mut input: impl BufRead + Send + 'static,
    mut output: impl Write,
) -> std::io::Result<()> {
    let (tx, rx) = channel::<Vec<u8>>();
    std::thread::spawn(move || loop {
        let mut line = Vec::new();
        match input.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                if line.last() == Some(&b'\n') {
                    line.pop();
                }
                if tx.send(line).is_err() {
                    return;
                }
            }
        }
    });
    let mut send = |reply: Response| -> std::io::Result<bool> {
        match output
            .write_all(&encode_response(&reply))
            .and_then(|()| output.flush())
        {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(false),
            Err(e) => Err(e),
        }
    };
    loop {
        let line = match session.next_deadline() {
            Some(due) => match rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(list) = session.tick(Instant::now()) {
                        if !send(list)? {
                            return Ok(());
                        }
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            },
            None => match rx.recv() {
                Ok(line) => line,
                Err(_) => return Ok(()),
            },
        };
        let reply = match decode_request(&line) {
            Ok(Request::Bye) => return Ok(()),
            Ok(req) => session.handle(req),
            Err(err) => session.handle_error(err),
        };
        if let Some(reply) = reply {
            if !send(reply)? {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const US: char = '\u{1f}';

    fn hello() -> String {
        format!("hello{US}1{US}zsh{US}5.9{US}zsh{US}{US}autosuggestions\n")
    }
    fn suggest(id: u64) -> String {
        format!("suggest{US}{id}{US}async{US}0{US}/{US}git\n")
    }
    fn run(input: &[u8]) -> String {
        let mut out = Vec::new();
        serve_with(
            Session::new(None, None),
            Cursor::new(input.to_vec()),
            &mut out,
        )
        .unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn serves_lines_until_eof() {
        let out = run(format!("{}{}", hello(), suggest(1)).as_bytes());
        assert_eq!(out, format!("ready{US}1\nsuggestion{US}1{US}\n"));
    }

    #[test]
    fn bye_stops_reading() {
        let out = run(format!("{}bye\n{}", hello(), suggest(1)).as_bytes());
        assert_eq!(out, format!("ready{US}1\n"));
    }

    #[test]
    fn garbage_line_is_skipped_and_serving_continues() {
        let mut input = b"\xff\xff\n".to_vec();
        input.extend(suggest(3).as_bytes());
        let out = run(&input);
        assert_eq!(out, format!("suggestion{US}3{US}\n"));
    }
}
