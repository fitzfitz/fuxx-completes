# specs

Command knowledge for fuxx-autocomplete: what each command's subcommands, options and arguments
are, and where live values come from. Nothing is here yet. The format is settled in milestone M4
(see `docs/design.md` §5.2 and §11).

## The format to come

Our own, declarative, one file per command. Each file describes:

- the command's name(s) and description;
- subcommands, recursively;
- options: names (`-v`, `--verbose`), description, argument, repeatable, mutually exclusive ones;
- arguments: optional or required, variadic, and their values: a fixed list, a template (`files`,
  `folders`) or a named generator (e.g. `git.branches`).

Specs never contain code. Whether files are JSON or TOML, and whether they are compiled into the
binary or loaded from disk, is decided in M4.

## Imported data

The first specs are converted from Fig's withfig/autocomplete (MIT). Imported files:

- are marked as imported, with the upstream commit they came from;
- keep upstream's license notice in [`../THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md);
- are ours to edit from then on.

The planned layout is a folder for imported specs and one for specs we wrote ourselves; M4 settles
the names.

## Generators

A spec may name a generator for live values. Generators run local programs directly (never through
a shell), with a timeout, and only from specs shipped here or in the user's own spec folder.
