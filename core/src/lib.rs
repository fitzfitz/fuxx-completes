//! # fuxx-completes-core
//!
//! The shared engine behind `fuxx-autosuggestions` and `fuxx-autocomplete`. See
//! `docs/design.md`.
//!
//! Implemented so far (M0): the wire format 1 protocol types (`protocol`), the zsh history parser
//! (`zsh_history`), the history index with ranking for a prefix (`history`) and re-reading the
//! history file when it changes (`history_file`). Since 2026-11-13, whether a `cd` target is
//! certainly missing (`cd_target`), so such a match is passed over, and whether it goes down into
//! the current folder, so such a match ranks above one that works from anywhere.
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

pub mod cd_target;
pub mod history;
pub mod history_file;
pub mod protocol;
pub mod zsh_history;
