# fish adapter

`fuxx-autosuggestions.fish` (M2) reports the commands you run in fish; the menu is M7. Design:
`docs/design.md` §5.5, §5.7, §5.8 and §6. Needs fish 3.0 or later (tested on 4.9.3; 3.0.2 and
3.7.1 don't build on the Mac used for testing, so the 3.x range is untested so far).

## Install

Copy or link `fuxx-autosuggestions.fish` into `~/.config/fish/conf.d/`, or `source` it from
`config.fish`, with the `fuxx-completes` binary on `PATH` (or named in `FUXX_COMPLETES_BIN`).
It prints nothing. `fuxx-completes doctor` (a fish function it defines when you have none of that
name) shows `state off (fish has its own autosuggestions)` and whether reporting is on.

What fish allows, honestly:

- **fish already has autosuggestions** and no public hook to supply our own ghost text. So
  fuxx-autosuggestions **stays off in fish**.
- **Reporting commands:** fish has no coproc and can't keep an extra file descriptor, so there is
  no long-lived core process. From `fish_postexec` the adapter starts a short-lived
  `fuxx-completes report` in the background (disowned, so the prompt never waits). It appends one
  record to the context file, so your other shells and a host terminal such as fuxx get the
  context of commands run in fish. Not reported: commands starting with a space, anything in
  private mode (`fish --private`), anything while `fish_history` is set empty, lines over 1000
  characters, and, on fish 4.0+, whatever `fish_should_add_to_history` rejects (when you define
  it, it alone decides, as in fish). The folder reported is where the command started.
- **An honest limit of reporting:** `report` loads no history, so it can't prune by it. A command
  you delete from fish's history keeps its records (a keyed hash, folder, status and time; never
  the text) until the file passes its 4 MiB cap and the oldest records go (the user's decision,
  2026-11-23).
- **Menu (M7):** fish's `complete -c <command> -a '(…)'` can ask the `fuxx-completes` binary for
  argument values, shown in fish's own pager on Tab. No as-you-type menu in fish.
- **No priority in fish:** every `complete -c` definition for a command is merged, and fish
  autoloads its own. So we register only for commands fish has no completions for. Forcing ours on
  for a command fish covers merges both lists.
- **Host channel:** fish has no buffer-change hook, so it can't pass the buffer to a host terminal
  as you type. Commands still reach the host through the context file.
- Stays off in Warp, whose own editor replaces fish's.
