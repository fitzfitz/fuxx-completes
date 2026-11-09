# AGENTS.md — Operating Instructions for fuxx-completes

This file is the system prompt for any AI agent working in this repo. Read it fully before doing
**anything**, including asking clarifying questions or exploring files.

fuxx-completes holds two libraries, **fuxx-autosuggestions** (grey ghost text from history) and
**fuxx-autocomplete** (an as-you-type menu), on one Rust core, with thin zsh, bash and fish
adapters. **[`docs/design.md`](docs/design.md) is the source of truth.** This file enforces it.
When they disagree, the design wins; flag it.

---

## The Hard Rules (non-negotiable)

1. **Superpowers workflow, always.** Every task runs through the Superpowers agentic workflow.
   Invoke the relevant skill *before* acting. No task is exempt. (See below.)

2. **Stay on the current milestone. Never work ahead.** Do exactly the one next task from the
   current milestone in [`docs/roadmap.md`](docs/roadmap.md). One milestone at a time; verify before
   moving on. Autosuggestions come before autocomplete.

3. **Nothing outside the written plan.** Do not add features, dependencies, architecture or "nice
   to haves" that `docs/design.md` doesn't have. If the user asks for something out of scope,
   **stop, say it's outside the plan, cite the section** (usually §2 Goals/non-goals or §4
   Decided), and ask whether the design should change first.

4. **Update the docs with every task.** A task isn't done until the docs reflect it: flip the
   milestone's status in `docs/roadmap.md`, add a Progress log entry there, and change
   `docs/design.md` if the design moved. The docs are part of the deliverable.

5. **Never rely on zsh.** The core knows nothing about any shell's runtime: it never calls, embeds
   or depends on zsh, bash or fish, and never uses a shell's completion system for knowledge.
   (Parsing a history file's format is fine; it's just a file.) Adapters stay thin: they draw,
   forward keys and report commands. No ranking, parsing or command knowledge in shell code.

6. **Never block or slow the user's typing.** A keystroke never waits on the core. Requests are
   async, stale answers are dropped, a slow or dead core means no suggestion, never a slow prompt
   or an error at the prompt. Respect the latency budget in `docs/design.md` §3. Adapters print
   nothing at shell startup.

7. **History and command lines never leave the machine.** No network code, no telemetry, no
   uploads. History files are read, never written or copied. Logs carry no command lines by default.

8. **Imported spec data keeps its license notice.** Anything converted from Fig's
   withfig/autocomplete (MIT) keeps upstream's notice in `THIRD_PARTY_NOTICES.md`, copied exactly,
   with the upstream commit.

---

## Superpowers workflow (how to run every task)

Announce "Using [skill] to [purpose]" and follow the skill exactly.
- **Creative / feature / design work** → `superpowers:brainstorming` **first**, before code.
- **Executing a milestone** → `superpowers:writing-plans` / `superpowers:subagent-driven-development`.
- **Any feature or fix** → `superpowers:test-driven-development`. The core is very testable; shell
  drawing often isn't fully, so verify that manually in a real terminal and say so.
- **Any bug/surprise** → `superpowers:systematic-debugging` before proposing a fix.
- **Before claiming done/fixed/passing** → `superpowers:verification-before-completion`. Evidence
  (real command output, or a user-confirmed observation of the terminal) before assertions.

If even a 1% chance a skill applies, use it.

## Library docs — always use Context7

Before adding or using any crate, consult **Context7** (`resolve-library-id` → `query-docs`) for
current APIs and versions; fall back to crates.io or the source repo if Context7 lacks it. **Never
trust a remembered version or API.** The same goes for shell mechanisms (ZLE widgets, readline,
fish's `complete`): check the real docs, and the prior art (zsh-autosuggestions,
zsh-autocomplete), before relying on a behaviour.

## Session lifecycle

- **Startup:** read the latest Progress log entry in `docs/roadmap.md`; note the current milestone;
  identify the one next task.
- **During:** frame the task, do it, explain non-obvious Rust or shell code as you go, verify on the
  real machine.
- **Shutdown:** update the roadmap (status and Progress log) and the design if it changed; commit
  only when the user asks (branch first if on `main`).

## Guardrails specific to this project

- **Scope is the plain shell prompt.** Not inside AI agents such as Claude Code. Not history sync,
  not ⌃R replacement, not AI suggestions, not Windows. See `docs/design.md` §2.
- **Coexist, don't fight.** If zsh-autosuggestions, zsh-autocomplete, fzf, atuin, ble.sh or fish's
  own autosuggestions already do a job, detect it and stay off (`docs/design.md` §9). Never break
  zsh's Tab, ⌃R or prompt themes such as powerlevel10k.
- **Use hook helpers, don't overwrite.** In zsh, `add-zsh-hook` and `add-zle-hook-widget`; only add
  and remove our own `region_highlight` entries.
- **Generators run programs directly** (argument lists, never `sh -c`), with a timeout, only from
  specs we ship or the user's own spec folder.
- **Tests never touch the user's real shell rc files or history.** Use a temporary HOME (and
  `ZDOTDIR`, `XDG_*` as needed) and generated history files, every time.
- **Be honest about shell limits.** bash and fish can't do everything zsh can. Say so in docs; don't
  paper over it.

## What you must NOT do

- Skip the Superpowers workflow "to save time."
- Work ahead of the current milestone, or start autocomplete before autosuggestions ships.
- Introduce anything not in the written plan without the user changing the plan first.
- Put smarts in an adapter, or make the core depend on a shell.
- Add anything that sends data off the machine.
- Mark work done without updating `docs/roadmap.md`.
- Claim terminal drawing works without a user-confirmed observation.
- Commit, push or add a remote unless the user asks.
