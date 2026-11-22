# fuxx-autosuggestions

Grey ghost-text suggestions for the shell prompt, from your past commands: the library behind
the [fuxx-completes](https://github.com/fitzfitz/fuxx-completes) plugins for zsh and bash.
It turns what you have typed into the one suggestion to show (and a short list when you pause),
on top of [`fuxx-completes-core`](https://crates.io/crates/fuxx-completes-core).

To use the suggestions in your shell, install the
[`fuxx-completes`](https://crates.io/crates/fuxx-completes) binary and a shell adapter; see the
repo's README. macOS first; pre-1.0, so the API may change in a minor version.

Everything stays on your machine: no network code, no telemetry. The design is in
[`docs/design.md`](https://github.com/fitzfitz/fuxx-completes/blob/main/docs/design.md).

Licensed under either of MIT or Apache-2.0, at your option.
