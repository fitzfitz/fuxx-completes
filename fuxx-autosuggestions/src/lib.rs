//! # fuxx-autosuggestions
//!
//! Grey ghost text that completes the command you are typing from your past commands.
//! Nothing is implemented yet; this crate is a placeholder. See `docs/design.md`.
//!
//! What goes here:
//!
//! - Turning the core's ranked history into **one** suggestion for the current line.
//! - Where a partial accept stops (the next word boundary) and what a full accept inserts.
//!
//! History reading and ranking live in `fuxx-completes-core`. Drawing the ghost text lives in
//! the adapters (`adapters/`) and in fuxx.
