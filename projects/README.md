# Project mappings

What each project type means to fuxx-completes (design §5.4, M1b). `core/build.rs` compiles these
files into static tables; bad data fails the build. Files are read in name order, entries in file
order. Nothing here is ever run in a project: readers only pull names out of small files.

```toml
[[project]]
id = "pnpm"
markers = ["pnpm-lock.yaml"]     # any of these in the folder (a name, or "*.ext")
requires = ["package.json"]      # all of these present
requires_any = []                # when not empty, one of these present
contains = { file = "…", text = "…" }   # optional: a file must contain a text
beats = ["npm"]                  # drop these types when both match in one folder
beats_below = ["npm"]            # in nearer folders on the walk (workspace packages), take
                                 # these types' place: a pnpm repo's packages get pnpm commands
commands = ["pnpm install"]      # fixed commands, everyday ones first
[[project.from]]
reader = "json_keys"             # json_keys, toml_keys, make_targets, just_recipes, yaml_keys, file_names
files = ["package.json"]         # first that exists; for file_names a "*.ext" pattern
path = "scripts"                 # json/toml: "a.b" or "a[].k"; yaml: the top-level key
templates = ["pnpm {}", "pnpm run {}"]
```
