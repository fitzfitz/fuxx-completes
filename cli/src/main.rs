use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args != ["serve"] {
        eprintln!("usage: fuxx-completes serve");
        return ExitCode::from(2);
    }
    // A write error (the adapter went away) ends the core quietly; nothing is printed while serving.
    let _ = fuxx_completes::serve::serve(std::io::stdin().lock(), std::io::stdout().lock());
    ExitCode::SUCCESS
}
