<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Shell UX: listing aliases and the ed workflow (K183)

Status: OBSERVED on the `shell-test` image; the aliases are interactive-only and `ed` writes and
reads back a real file.

## Listing aliases

`etc/brush/brushrc` defines two aliases when the shell is interactive:

```sh
alias l='ls -CF'
alias ll='ls -lah'
```

`l` gives the compact classified listing (`/` for directories, `*` for executables); `ll` gives
the long listing with hidden files and human-readable sizes.

The aliases exist only in interactive brush sessions. Non-interactive `brush -c` never loads
`/etc/brush/brushrc`, and the rc also guards on `$-`:

```text
$ brush -c 'alias l; echo no=$?'
alias: l: not found
no=1
```

A script that relied on the base `PATH` keeps working; the aliases cannot leak into it.

The K177 bobtheshell prompt is unchanged by this ticket. `RAVEN_PROMPT=plain`, `NO_COLOR` and
`TERM=dumb` still select the same prompt modes as before.

## ed

`ed` is the 0.1.0 text editor. It is the POSIX line editor from `userland/bin/ed`, built into
`/bin/ed` for the shell images. A full-screen editor (`vi`, `nano`, `emacs`) is **not** in this
ticket and not in 0.1.0.

`ed` reads a newline-separated script from the serial console and executes it after one idle
second. Append, write and quit:

```text
$ ed
a
raven ed works
.
w /tmp/raven-ed.txt
q
```

After the idle pause the editor writes the file and returns to the shell:

```text
$ cat /tmp/raven-ed.txt
raven ed works
```

The `qtest` case `tests/qtest/cases/shell-alias.toml` runs these commands and asserts the file
content, so a missing alias or a broken `ed` write fails the gate.

## Scope

No oh-my-bash, Reedline backend/hint change, eza/Atuin, network fetch, boot-log styling,
coloured `--help` or second editor binary is part of K183.

## Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| interactive short aliases | user/site shell configuration; no system-wide `l`/`ll` requirement | NetBSD `etc/profile` (BSD-3-Clause) sets environment only, no aliases | distro `/etc/profile.d` aliases are user convenience, not shell semantics | first-principles two-line alias block in `etc/brush/brushrc`; no donor code copied |
| line-editor smoke path | `ed` is the historical line editor | NetBSD `bin/ed` (BSD-3-Clause) is the behavioural reference; the Rust editor already exists from P4t | GNU ed is reference only | use the shipped first-party `/bin/ed`; no editor code added or copied |
