# bash adapter

Not built yet. Planned for M2 (suggestions, host channel) and M7 (menu). Design:
`docs/design.md` §3 and §6.

What bash allows, honestly:

- **Plain readline has no way to show ghost text** as you type: no `POSTDISPLAY` equivalent and no
  per-keystroke hook. We won't draw grey text behind readline's back.
- **What works:** `bind -x` runs a function on a key and can read and set `READLINE_LINE` and
  `READLINE_POINT`. So a key can insert the best suggestion, and a key can show a menu.
- **A bounded wait, on explicit keys only.** Those keys ask the core synchronously and wait at most
  50 ms; if the answer is late they do nothing. Keys that only type are never bound and never
  wait. The menu blocks the line only while it is open.
- **Plain readline needs bash 4.0 or later.** macOS ships bash 3.2, which lacks `READLINE_LINE`;
  use Homebrew's bash, or ble.sh.
- **ble.sh** runs on bash 3.0+, replaces readline and has its own grey auto-complete. Whether we
  can feed it our suggestions is an open question for M2; if not, we stay off when ble.sh is
  loaded.
- **History:** a `PROMPT_COMMAND` function reports a command only when `history 1` shows a new
  entry, so `HISTCONTROL` and `HISTIGNORE` are respected.
- **Host channel (M2):** with `FUXX_COMPLETES_HOST=1`, the buffer goes to the terminal over OSC
  7701 at the explicit keys (readline has no buffer-change hook), and `ESC [ 7701 ~` applies what
  the terminal accepted.
- Stays off in Warp, whose own editor replaces readline.
- bash's own `complete` setup and Tab are left alone.
