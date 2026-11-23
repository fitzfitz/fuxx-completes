# Third-party notices

fuxx-completes is licensed MIT OR Apache-2.0 (see `LICENSE-MIT` and `LICENSE-APACHE`). The
binary links a few other crates, and some data in this repo comes from other projects, under
their own licenses, listed here.

## Rust crates in the binary

The `fuxx-completes` binary links these crates (versions from `Cargo.lock`; the rest of our
dependencies are used only at build or test time and are not in the binary). Each is used under
the Apache License, Version 2.0, whose text is `LICENSE-APACHE`.

| Crate | Version | Upstream | Its licence | Copyright line in its licence files |
| --- | --- | --- | --- | --- |
| blake3 | 1.8.7 | https://github.com/BLAKE3-team/BLAKE3 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | Copyright 2019 Jack O'Connor and Samuel Neves |
| arrayvec | 0.7.8 | https://github.com/bluss/arrayvec | MIT OR Apache-2.0 | Copyright (c) Ulrik Sverdrup "bluss" 2015-2023 |
| cfg-if | 1.0.5 | https://github.com/rust-lang/cfg-if | MIT OR Apache-2.0 | Copyright (c) 2014 Alex Crichton |
| constant_time_eq | 0.4.2 | https://github.com/cesarb/constant_time_eq | CC0-1.0 OR MIT-0 OR Apache-2.0 | (none given) |
| cpufeatures (x86_64 binary only) | 0.3.1 | https://github.com/RustCrypto/utils | MIT OR Apache-2.0 | Copyright (c) 2020-2026 The RustCrypto Project Developers |

## Imported command spec data

*Reserved. Nothing has been imported yet.*

When command specs are imported from Fig's withfig/autocomplete (MIT) into `specs/`, this section
gets:

- the upstream repository URL and the exact commit converted from;
- the list of imported files (or the folder they live in);
- upstream's copyright line and MIT permission notice, **copied exactly** from its LICENSE file at
  that commit.

The notice stays for as long as any imported data remains, even after we change it.
