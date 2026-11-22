<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Donor port test register (K450)

Reducers for the tree-wide donor-file test mapping:

| File | Purpose |
|---|---|
| `run.py` | Enumerates donor-marked files, maps each to a test, regenerates the register and doc, and gates staleness |
| `register.tsv` | Generated register: one row per donor file with status, basis, test evidence, owner and proposed ticket |
| `../../../docs/releases/0.1.0-port-register.md` | Generated summary, counts per area and every NONE row |

## Commands

```sh
timeout 120 python3 tests/release/port-register/run.py --write   # regenerate
timeout 120 python3 tests/release/port-register/run.py --check   # reducer gate
```

`--check` exits non-zero when a donor-marked file has no register row, when a row is stale, or
when the recorded status/evidence or the generated doc differs from a fresh regeneration.

## Method

The script unions `PROVENANCE.toml` imports under `kernel/`, `libc/`, `crates/` and `userland/`
with source files whose first 20 lines carry a donor marker (`RavenBSD: transpiled/adapted/
ported/derived from`, The Regents, NetBSD/FreeBSD/OpenBSD/DragonFly/Illumos/Sun copyright, or a
BSD-2/BSD-4/CDDL SPDX header).

The test index is every `*_ut.rs`, every file below a `tests/` directory under those four areas,
and the repository `tests/` tree (Rust, shell, TOML, C and header fixtures). Status:

- **COVERED** - a test file names the donor file's full path, path suffix, basename or module
  stem. A Rust source file needs a same-crate test, or a test carrying its full path/suffix.
- **INDIRECT** - no direct name match, but the donor file's crate has tests that exercise it.
- **NONE** - no direct or crate-level test; the row carries an owner lane and a proposed ticket.

This is a static file-to-test mapping, not line or branch coverage. It does not prove the named
test runs in a gate; kernel unit tests are currently not host-buildable (K416).

## Exclusions

`addons/`, `donors/`, and the unbuilt vendor worktrees `userland/brush/`, `userland/uutils/` and
`toolchain/` are outside the register. The first two are the ticket's exclusions; the other three
are directory-group `PROVENANCE.toml` imports of vendored trees, not transpiled Raven files.
