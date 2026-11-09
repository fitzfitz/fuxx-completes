# fish adapter

Not built yet. Planned for M2 and M7. Design: `docs/design.md` §6.

What fish allows, honestly:

- **fish already has autosuggestions** and no public hook to supply our own ghost text. So
  fuxx-autosuggestions **stays off in fish**. The adapter may still report commands that ran, so
  ranking stays consistent across shells.
- **Menu (M7):** fish's `complete -c <command> -a '(…)'` can ask the `fuxx-completes` binary for
  argument values, shown in fish's own pager on Tab. No as-you-type menu in fish.
- fish's own completions keep priority unless the user turns ours on.
