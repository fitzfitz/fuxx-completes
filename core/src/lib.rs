//! # fuxx-completes-core
//!
//! The shared engine behind `fuxx-autosuggestions` and `fuxx-autocomplete`. See
//! `docs/design.md`.
//!
//! What goes here:
//!
//! - **History:** reading past commands (read-only) and the commands an adapter reports
//!   during a session, and ranking them for a typed prefix.
//! - **Command line parsing:** finding the current command and word in a typed line.
//! - **Command knowledge:** loading our own command specs (see `specs/`), including data
//!   converted from Fig's MIT-licensed specs.
//! - **Generators:** live values such as git branch names or files, run off the typing path
//!   with a timeout and a cache.
//! - **The adapter protocol:** the request and response types that shell adapters, other
//!   apps and fuxx use.
//!
//! The core knows nothing about any shell's runtime: it never calls, embeds or depends on
//! zsh, bash or fish. It never sends history or command lines off the machine.

pub mod history;
pub mod history_file;
pub mod protocol;
pub mod zsh_history;
