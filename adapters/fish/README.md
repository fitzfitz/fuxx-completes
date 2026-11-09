# fish adapter

Not built yet. Planned for M2 and M7. Design: `docs/design.md` §5.8 and §6. Needs fish 3.0 or later.

What fish allows, honestly:

- **fish already has autosuggestions** and no public hook to supply our own ghost text. So
  fuxx-autosuggestions **stays off in fish**.
- **Reporting commands:** fish has no coproc and can't keep an extra file descriptor, so there is
  no long-lived core process. From `fish_postexec` the adapter starts a short-lived
  `fuxx-completes report` in the background (disowned, so the prompt never waits). It appends one
  record to the context file, so your other shells and a host terminal such as fuxx get the
  context of commands run in fish. Not reported: commands starting with a space, anything in
  private mode (`fish --private`), and, on fish 4.0+, whatever `fish_should_add_to_history`
  rejects.
- **Menu (M7):** fish's `complete -c <command> -a '(…)'` can ask the `fuxx-completes` binary for
  argument values, shown in fish's own pager on Tab. No as-you-type menu in fish.
- **No priority in fish:** every `complete -c` definition for a command is merged, and fish
  autoloads its own. So we register only for commands fish has no completions for. Forcing ours on
  for a command fish covers merges both lists.
- **Host channel:** fish has no buffer-change hook, so it can't pass the buffer to a host terminal
  as you type. Commands still reach the host through the context file.
- Stays off in Warp, whose own editor replaces fish's.
