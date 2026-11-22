# fuxx-completes-core

The shared engine of [fuxx-completes](https://github.com/fitzfitz/fuxx-completes): it reads your
shell history (zsh, bash, fish), ranks what you ran before by folder, exit status and how often,
knows a project's own commands, and answers a small line protocol. It knows nothing about any
shell's runtime; the shell adapters only draw and forward keys.

Used by [`fuxx-autosuggestions`](https://crates.io/crates/fuxx-autosuggestions) and the
[`fuxx-completes`](https://crates.io/crates/fuxx-completes) binary. macOS first; pre-1.0, so the
API and the wire format may change in a minor version.

Everything stays on your machine: no network code, no telemetry. The design is in
[`docs/design.md`](https://github.com/fitzfitz/fuxx-completes/blob/main/docs/design.md).

Licensed under either of MIT or Apache-2.0, at your option.
