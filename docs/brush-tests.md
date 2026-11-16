<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# brush host tests (K26f)

Status: host suite green on the K26f base. The interactive login remains P9a; this document
describes the host-only gate only.

## Exact command

The brush sources are a nested Cargo workspace under `userland/brush`. The root workspace does
not resolve `-p brush-shell`, so run nextest from the nested directory:

```sh
cd userland/brush
cargo nextest run -p brush-shell
```

On the K26f base this runs 3707 tests: **3707 passed, 109 skipped, 0 failed**. Nextest is the
mandatory runner for Rust tests; do not substitute `cargo test`.

The host oracle is GNU bash **5.1.16**. Compat cases that assert bash 5.2+ behaviour carry
`min_oracle_version: "5.2"` and are skipped on this host; they run unchanged on a 5.2/5.3
oracle. The gates cover `complete -p` quoting/order, negative `history -d` offsets,
`printf %Q`, symbolic `umask` modes, `unset 'assoc[*]'`, here-docs in command substitution,
`patsub_replacement`, and prompt `\#` numbering. The existing 5.3 gates for `fullquote`,
alternate-form `printf`, and other 5.3 changes are unchanged.

The two `--init-file` / `--rcfile` compat cases drop `--norc`, so the oracle also reads the
host's `/etc/bash.bashrc`. A Debian/Ubuntu system rc prints the sudo hint; the cases set
`home_dir: "."` and create `.hushlogin` so the oracle output is the `testrc` marker only. The
assertion is unchanged.

`Signals::Ctrl+C ends a loop running a program` remains a documented known failure and is
skipped on Linux: the pty harness compares only the process exit status there, so the interactive
transcript assertion cannot run reliably. The target/manual signal behaviour is unchanged by
this ticket.

## Warning-free build

The nested workspace declares the custom target value once in
`userland/brush/.cargo/config.toml`:

```toml
[build]
rustflags = ["--check-cfg=cfg(target_os,values(\"ravenbsd\"))"]
```

That removes the `unexpected_cfgs` warnings from the vendored `socket2`, `mio`, `uzers` and
patched `libc` without `-A warnings`. The vendored `nix` crate already carries a crate-level
`#![allow(unexpected_cfgs)]`; the host check reports no `unexpected_cfgs` diagnostics. The only
remaining cargo note is rustix's `hints.mostly-unused` profile hint, which is not a cfg warning.

`cargo check -p brush-shell` from `userland/brush` is the quick warning check.

## Target smoke

The target smoke is separate from the host suite:

```sh
cargo xtask build brush --arch x86_64
cargo xtask qtest --case tests/brush/shell-brush.toml --arch x86_64
```

`tests/brush/shell-brush.toml` boots `shell-test` (rsh owns the console), checks
`/bin/brush --version`, runs the bash-compat smoke, and asserts a clean init exit. The target
interactive login is still P9a; `tests/qtest/cases/modern-shell-login.toml` records that
dependency.

`cargo xtask libc-pack --arch x86_64` is required before the target build, as for the other
target userland.

## What this gate does not cover

* The interactive brush login and the `modern-shell` profile (P9a).
* Target external-command spawn (`posix_spawnp`) and pipelines (P9a/PG-35).
* A Pi 4/Pi 5 board result; all target smoke evidence is QEMU x86_64.
* The 109 skipped tests: known failures, bash 5.2+ oracle gates, and platform exclusions. They
  remain in the suite and run where their prerequisite holds.
