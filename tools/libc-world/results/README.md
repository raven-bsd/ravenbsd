# libc-world results

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

`libc-world report` writes `world.json`, `world.md`, `missing-symbols.tsv`, `headers.tsv` and
`libs.tsv` here under the target architecture (`x86_64`, `aarch64`, `riscv64`). `libc-world build`
writes the per-package `<name>.json`, `undef/<name>.undef` and `headers/<name>.txt` files.

Nothing here is source; the files are deterministic by design (no timestamps, no host paths, no
user names). A committed `results/<arch>` tree is the evidence a review reads; regenerate it with
the current sysroot and the owner's tarballs, never by hand.
