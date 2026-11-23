# fuxx-completes

The `fuxx-completes` binary: grey ghost-text suggestions for zsh and bash, from your own history
and your project's commands. The shell adapters start it and draw what it answers; in fish, which
has its own suggestions, the adapter only reports your commands so they count in your other
shells.

```
cargo install fuxx-completes
```

The binary alone does nothing visible: add the adapter for your shell. The adapters, Homebrew
and plugin-manager install lines are in the repo's
[README](https://github.com/fuxx-dev/fuxx-completes#install). `fuxx-completes doctor` shows
what is running and why. macOS first; pre-1.0.

Everything stays on your machine: no network code, no telemetry. The design is in
[`docs/design.md`](https://github.com/fuxx-dev/fuxx-completes/blob/main/docs/design.md).

Licensed under either of MIT or Apache-2.0, at your option.
