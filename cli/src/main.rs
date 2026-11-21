use fuxx_completes_core::context::{data_dir_from_env, DataSource};
use std::process::ExitCode;

const USAGE: &str = "usage: fuxx-completes serve | forget | doctor";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args[..] {
        ["serve"] => {
            // A write error (the adapter went away) ends the core quietly; nothing is printed
            // while serving.
            let _ = fuxx_completes::serve::serve(std::io::stdin().lock(), std::io::stdout().lock());
            ExitCode::SUCCESS
        }
        ["forget"] => forget(),
        ["doctor"] => doctor(None),
        ["doctor", "--shell-facts", facts] => doctor(Some(facts)),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
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

fn doctor(facts: Option<&str>) -> ExitCode {
    let data = data_dir_from_env();
    print!(
        "{}",
        fuxx_completes::doctor::report(facts, data.as_ref(), env!("CARGO_PKG_VERSION"))
    );
    ExitCode::SUCCESS
}
