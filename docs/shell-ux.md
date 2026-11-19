<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Shell UX: listing aliases and the ed workflow (K183)

Status: the aliases are OBSERVED on the release login. The `ed` smoke path is BLOCKED: the
current `ed` target wrapper reads the raw `DEBUG_READ` ring, while release console input is
owned by `consd`, so the editor never receives the script (see K183 report).

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

**Known blocker (K183).** The target wrapper collects commands with `raven_rt::debug_read`, the
raw serial ring. In the release/shell images `consd` owns that ring and serves the login console,
so the editor sees no input, idles out and exits; the queued script then reaches the shell as
commands. Making `ed` read the inherited console/stdin handle (or a `-c script` file) is a
separate editor integration change outside K183's Owns; the commands below are the intended
workflow and the shape of the `shell-alias` case, not a passing gate yet.

Once that I/O is fixed, `ed` reads a newline-separated script and executes it after one idle
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
content, so a missing alias or a broken `ed` write fails the gate. It currently fails at the
`ed` step for the reason above; the alias and `l`/`ll` steps pass.

## Scope

No oh-my-bash, Reedline backend/hint change, eza/Atuin, network fetch, boot-log styling,
coloured `--help` or second editor binary is part of K183.

## Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| interactive short aliases | user/site shell configuration; no system-wide `l`/`ll` requirement | NetBSD `etc/profile` (BSD-3-Clause) sets environment only, no aliases | distro `/etc/profile.d` aliases are user convenience, not shell semantics | first-principles two-line alias block in `etc/brush/brushrc`; no donor code copied |
| line-editor smoke path | `ed` is the historical line editor | NetBSD `bin/ed` (BSD-3-Clause) is the behavioural reference; the Rust editor already exists from P4t | GNU ed is reference only | use the shipped first-party `/bin/ed`; K183 only retries console interrupts, no editor code copied |
