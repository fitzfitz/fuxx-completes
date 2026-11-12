//! The `serve` loop: one request per line in, replies out, each flushed at once.

use crate::session::Session;
use fuxx_completes_core::protocol::{decode_request, encode_response, Request};
use std::io::{BufRead, ErrorKind, Write};

/// Reads LF-terminated lines until EOF or `bye`; writes and flushes each reply at once.
/// Lines that don't decode (including invalid UTF-8) are skipped. A closed output pipe ends
/// the loop quietly with `Ok`.
pub fn serve(mut input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    let mut session = Session::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        let reply = match decode_request(&line) {
            Ok(Request::Bye) => return Ok(()),
            Ok(req) => session.handle(req),
            Err(err) => session.handle_error(err),
        };
        if let Some(reply) = reply {
            let sent = output
                .write_all(&encode_response(&reply))
                .and_then(|()| output.flush());
            match sent {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::BrokenPipe => return Ok(()),
                Err(e) => return Err(e),
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
        serve(Cursor::new(input.to_vec()), &mut out).unwrap();
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
