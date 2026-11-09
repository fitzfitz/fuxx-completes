//! # fuxx-autocomplete
//!
//! A menu while you type: commands, subcommands, flags, arguments, branches and files.
//! Nothing is implemented yet; this crate is a placeholder. See `docs/design.md`.
//!
//! What goes here:
//!
//! - Building the menu's items for the current word from the core's command knowledge,
//!   generators and the file system.
//! - Ordering and filtering those items, and what inserting one does to the line.
//!
//! Specs, generators and parsing live in `fuxx-completes-core`. Drawing the menu lives in the
//! adapters (`adapters/`) and in fuxx.
