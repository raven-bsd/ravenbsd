<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Shell UX: listing aliases (K183)

Status: OBSERVED on the release login. `ed` is shipped, but its console/stdin I/O is K186 and
is not part of K183.

## Listing aliases

`etc/profile` defines two aliases when the shell is interactive:

```sh
alias l='ls -CF'
alias ll='ls -lah'
```

`l` gives the compact classified listing (`/` for directories, `*` for executables); `ll` gives
the long listing with hidden files and human-readable sizes.

The aliases exist only in interactive brush login sessions. Non-interactive `brush -c` does not
read `/etc/profile`, and the block also guards on `$-`:

```text
$ brush -c 'alias l; echo no=$?'
alias: l: not found
no=1
```

A script that relied on the base `PATH` keeps working; the aliases cannot leak into it.

The K177 bobtheshell prompt is unchanged by this ticket. `RAVEN_PROMPT=plain`, `NO_COLOR` and
`TERM=dumb` still select the same prompt modes as before.

## ed

`ed` is the 0.1.0 text editor. It is the POSIX line editor from `userland/bin/ed`, already
shipped in the tree. A full-screen editor (`vi`, `nano`, `emacs`) is **not** in this ticket and
not in 0.1.0.

K183 does not change or prove `ed`. The editor's console/stdin I/O is owned by **K186**; until
that lands, this ticket makes no claim that an interactive `ed` session works from the release
shell. The `shell-alias` qtest covers the aliases only.

## Scope

No oh-my-bash, Reedline backend/hint change, eza/Atuin, network fetch, boot-log styling,
coloured `--help` or second editor binary is part of K183.

## Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| interactive short aliases | user/site shell configuration; no system-wide `l`/`ll` requirement | NetBSD `etc/profile` (BSD-3-Clause) sets environment only, no aliases | distro `/etc/profile.d` aliases are user convenience, not shell semantics | first-principles two-line alias block in `etc/profile`; no donor code copied |
| line-editor smoke path | `ed` is the historical line editor | NetBSD `bin/ed` (BSD-3-Clause) is the behavioural reference; the Rust editor already exists from P4t | GNU ed is reference only | not part of K183; the shipped editor's console/stdin I/O is K186 |
