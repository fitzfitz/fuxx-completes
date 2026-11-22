# fuxx-completes: the suggestion list — design

**Status:** written 2026-11-22 from a conversation with the user; the builder check's one open
question answered by the user (the list on an empty line: yes, with its own setting).
**Approved by the user 2026-11-22.** Not planned, not built: M1c is planned after the user's
check of M1b. "Design §n" means `docs/design.md`; "fuxx" is the user's terminal app
(github.com/fitzfitz/fuxx-cli), which installs fuxx-completes as a plugin.

## 1. Intent and why

The user's words: "can this tools have multiple suggestions? like in fuxx cli, in terminal tab
inside chatbox, below the input command field, I want to have a list of suggestions (max 5), I
want fuxx-completes handles the list. I want to be able to configure the plugin inside setting.
when I want to type something and dont know what to type for 3 seconds, the list of suggestion
appeared, either I can type 1 and directly typed it in chat or I can select with arrow down or up.
to remove the suggestions, user use esc."

Today the ghost text shows **one** suggestion. When the user pauses because they don't know what
to type, one guess isn't enough; a short ranked list lets them pick. fuxx-completes owns the list
(what is in it and how it is drawn), so it works the same in fuxx and in any other terminal.

**Success:** at a pause, up to 5 good commands appear under the line; picking one takes a
keystroke; the list never gets in the way of typing, running a command, the ghost text, zsh's own
Tab completion or ⌃R.

## 2. Goals, non-goals, anti-goals

**Goals**
- After an idle pause, a numbered list of up to 5 suggestions under the command line.
- Pick with ⌥1–⌥5 (the user's choice of the options offered), or ↑/↓ then Enter or Tab. Esc closes
  it. A pick is **put into the line, never run** (the user agreed: "go ahead" to Enter inserting).
- The ghost text and the list are one feature: item 1 is the ghost text; the highlighted item
  becomes the ghost text.
- **On an empty line too** (the user, 2026-11-22: "yes agreed", with its own setting): after the
  pause at a blank prompt, the list shows what the user is likely to run here.
- Configurable: on/off, on an empty line or not, the pause, how many items. In fuxx, through Settings → Plugins; elsewhere,
  through shell variables.

**Non-goals**
- fuxx drawing the list in its own UI (design §8 "later"; it needs the host channel, M2). The zsh
  plugin draws; fuxx's shell-tab chat box already shows zsh's rows under the prompt.
- Menus of flags and arguments (that is fuxx-autocomplete, M4–M8). The list holds whole commands
  from the same sources as the ghost text.
- bash and fish (they get this when their adapters do, M2 or later; bash can't draw as you type).

**Anti-goals**
- The list appearing while the user is typing, or delaying a keystroke.
- A key the user relies on changing meaning when the list isn't showing (digits, ⌥-characters,
  ↑/↓ history, Enter, Tab).
- Enter running something other than what the user typed.
- Fighting zsh's completion list, ⌃R, or another plugin's display under the line.

## 3. Approaches

1. **The zsh plugin draws the list under the line — picked.** The core returns the top N; the
   plugin draws them below the line inside ZLE (as the ghost text is drawn) and owns the keys while
   the list is open. Works in every terminal; inside fuxx's shell tab it appears in the chat box
   because fuxx's box shows the rows under the prompt (fuxx note 88). The user said "go ahead" to
   this shape.
2. fuxx draws the list in its own chat-box UI, fed by the core over the host channel. Prettier and
   fuxx-only; needs M2's host channel and a fuxx drawing API that fuxx's plugin format 1 lacks
   (design §8). Kept as the later upgrade.
3. Reuse zsh's completion-list display (`zle -M` / compadd). Free drawing, but it is zsh's
   completion machinery (design §4: "we don't ever rely on zsh") and it collides with Tab's lists.

## 4. How it works

### 4.1 The core
- A `suggest` can ask for a list: the core's reply for the request also carries up to N ranked
  items (the same order as the ghost text: this folder's history, project commands, the rest;
  M1/M1b), each the full command line. Item 1 equals the ghost text.
- **The pause is timed by the core** (AI's call): after answering a `suggest`, if no newer request
  arrives within the configured delay, the core pushes a `list` message for that request's id. The
  zsh plugin already reads the core's answers asynchronously (`zle -F`), so no timer runs in the
  shell, nothing waits, and a newer keystroke makes the pushed list stale (dropped by id).
- **An empty line** (the user's decision): the plugin also asks at a fresh prompt and when the
  line becomes empty; the core answers with no ghost text but, after the pause, a list in this
  order: what usually follows the command just run (`prev`), this folder's history (M1b step 1,
  with an empty prefix), the project's commands, then the newest history from elsewhere. Ghost
  text appears on an empty line only for a highlighted item.
- The `cd` rules apply to list items as to the ghost text (no `cd` to a missing folder).
- Duplicates are dropped; items longer than the 1,000-character cap are skipped.
- Wire format: a new optional `suggest` field (list size, 0 = no list) and a new `list` message
  (id, then the items). An M1b core ignores the field; an older adapter never asks for a list.

### 4.2 Drawing (zsh)
```
~/proj ❯ npm r█un dev                       ← ghost text = item 1 (grey)
  1  npm run dev
  2  npm run build
  3  npm run test
  4  npm run lint
  5  npm run storybook                      ← dim; the highlighted row in reverse
```
- Drawn in `POSTDISPLAY` after the ghost text, styled through our own `region_highlight` entries
  (the existing memo/exact-string rules, design §6). Item numbers are shown so ⌥n is discoverable.
- Too narrow a terminal: rows are cut to the width with `…`. Fewer rows than items: fewer items.
- While zsh shows its own completion list (after Tab) or a ⌃R search, no list is drawn.

### 4.3 Keys while the list is showing
| Key | Does |
| --- | --- |
| ↓ / ↑ | ↓ enters the list and moves down; ↑ moves up inside it (no wrap); the ghost text follows the highlight. **With nothing highlighted, ↑ is history as always** (it closes the list), so the most common key after a pause keeps its meaning (AI's call, builder check) |
| ⌥1–⌥5 | put that item into the line (cursor at the end); the list closes |
| Enter | an item highlighted: put it into the line, list closes, **nothing runs**; none highlighted: run the line as normal |
| Tab | an item highlighted: put it into the line; none: today's Tab (design §6) |
| → / End | accept the ghost text as today (the highlighted item, if any) |
| Esc | close the list and hide the ghost text (as Esc does today) |
| any typing or editing | the list closes; the ghost text updates as today; after the next pause a new list |

- **No item is highlighted when the list opens** (AI's call), so Enter keeps running what was
  typed until the user moves into the list.
- **What restarts the pause** (AI's call, builder check): any key, not only edits. The core times
  the pause from the last request; after a key that doesn't edit (a cursor move, Esc) the plugin
  asks again with the same buffer, so the list never pops up while the user moves around the line,
  and a list that arrives after any key was pressed is dropped.
- **⌥n in practice** (AI's call, from recon of fuxx's key encoder, `src/keys.rs`: Option with a
  character key sends the character macOS composed): ⌥1–⌥5 arrive as `¡ ™ £ ¢ ∞` on a US layout,
  or as Esc then the digit where the terminal sends Option as Meta. While the list is open the
  plugin accepts both; when it is closed they type as normal. Other keyboard layouts compose other
  characters; there the picks are ↑/↓ and Enter (README says so).
- The keys live in a keymap of our own, switched to only while the list shows (copied from the
  user's current keymap, so everything else keeps working), so no binding outside the list changes.
  vi mode: in `viins` as in emacs; `vicmd` closes the list.

### 4.4 Settings
- **Shell variables** (any terminal): `FUXX_AUTOSUGGESTIONS_LIST` (on/off),
  `…_LIST_EMPTY` (on/off: also on an empty line), `…_LIST_DELAY` (seconds), `…_LIST_SIZE` (1–5).
- **In fuxx, Settings → Plugins → fuxx-completes:** fuxx's plugin format gains declared settings
  (AI's call on the shape): lines in `fuxx-plugin.conf` such as
  `setting.list = bool on  Show a suggestion list when you pause`,
  `setting.list_empty = bool on  Show the list on an empty line too`,
  `setting.list_delay = number 3 Seconds of pause before the list shows`,
  `setting.list_size = number 5 Items in the list (1–5)`.
  fuxx draws a control for each, stores the values in its config, and exports them as
  `FUXX_PLUGIN_SETTING_<NAME>` while it sources the plugin (as it does `FUXX_PLUGIN_DATA`); the
  plugin copies them at load. A change applies to new tabs (AI's call: no live reload).
- Defaults: on, on an empty line too, **3 seconds** (the user's number), 5 items.
- **Which wins inside fuxx** (AI's call, builder check): fuxx exports only the settings the user
  changed in Settings; such a value wins over a shell variable from `.zshrc` (the user just chose
  it in fuxx); otherwise the shell variable, otherwise the default. `doctor` shows each value and
  where it came from.

## 5. Constraints
- A keystroke never waits (AGENTS.md rule 6): the list arrives asynchronously; drawing it is one
  `zle -R` like the ghost text.
- The core's answer budget (10 ms p95) covers the list: the walk collects N items instead of one.
- Nothing new leaves the machine or is written to disk.
- zsh 5.3+ as today; tested on 5.3.1, 5.8.1, 5.9.

## 6. Where it lives and milestones

M1c is planned and built after the user's check of M1b (it builds on M1b's three-step walk); the
fuxx milestone follows fuxx's own rules (its Scope note changes first) and is planned there.
- **fuxx-completes:** a new milestone **M1c, suggestion list**, after M1b and before M2 (AI's
  call: it builds on M1/M1b's ranking and is zsh-only like them). Core: top-N and the pushed list;
  protocol fields; zsh drawing, keys and settings variables; `doctor` shows the list settings.
- **fuxx:** a small milestone for **declared plugin settings** in Settings → Plugins (fuxx's
  Scope note today excludes plugin settings beyond On/Off; the user changes that). It doesn't wait
  for M1c and M1c doesn't wait for it (the shell variables work without it).

## 7. Left to the builder
How the walk collects N items; the `list` message's exact fields; drawing details (indent, colour
of numbers, the cut marker); the keymap mechanics; how fuxx's Settings renders a declared setting.

## 8. Changes to make once approved
- `docs/design.md`: §2 goals (the list), §4 decided (the user's choices), §5.5 wire (field and
  `list`), §6 zsh (drawing and keys), §7 settings, §8 fuxx settings, §15 + roadmap (M1c).
- fuxx's vault: Scope (declared plugin settings in), a milestone note for plugin settings, the
  roadmap.

## 9. The look (added 2026-11-22, approved by the user)

The user, after v0.0.9 in fuxx: "the list appears not really good … add like title above like
'Suggestions:' … better in UI and UX"; chose fuxx-completes over fuxx-cli, picked look **A** of
three browser mockups ("A would be good"), and approved this design ("Approved and deploy it
directly").

```
❯ uv r(un whomee run)              ← ghost text, unchanged
  Suggestions                       ← dim title
  1  uv run whomee run              ← number in blue, command at normal brightness
▸ 2  uv run pytest -q               ← selected: ▸ and a soft grey background (bg=8)
  ⌥1–5 pick · ↑↓ move · esc close   ← dim hint
```
Colours are the terminal theme's. In a short window the hint goes first, then the title, before
any item; the title and hint are cut with … like the rows. Keys unchanged. Look-only, in the zsh
plugin. Approaches considered: B (compact, ⌥ labels on rows) and C (a box-drawing frame: a box in
fuxx's box, breaks when narrow).
