<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4a5 - POSIX newgrp

## Matrix

| Feature | Behaviour | Evidence |
|---|---|---|
| no operand | switch to the caller's login group from `/etc/passwd` | `test.sh`; `lib_ut` |
| `newgrp group` | switch to a named group from `/etc/group` | `test.sh`; `lib_ut` |
| `newgrp gid` | numeric operand resolves by gid | `test.sh`; `lib_ut` |
| `-l`, `-` | start the shell as a login shell (`-<basename>` argv[0] and login environment) | `test.sh`; `lib_ut` |
| membership | root, the user's primary group and explicit `members` entries are allowed | `test.sh`; `lib_ut` |
| non-member | denied with `permission denied: group password is not supported`; no prompt | `test.sh`; `lib_ut` |
| unknown group | `newgrp: <name>: no such group`, exit 1 | `test.sh`; `lib_ut` |
| shell | `$SHELL`, then the passwd shell, then `/bin/brush` | `test.sh`; `lib_ut` |
| credentials | `setgroups([gid])` then `setresgid(gid)`, verified before the shell | `lib.rs`; same `proc` path as `su` |
| exec | `raven_rt::spawn` of the shell, then `proc_wait` | `main.rs`; same path as `su` |

Usage errors exit 2; unknown group, permission and credential failures exit 1.

## Exact limits

- No interactive group-password prompt. A group whose record marks a password and the caller is
  not a member returns the documented permission error instead of reading the console.
- `/etc/gshadow` is not consulted; the group record's `x`/`*` marker is not a verifiable hash in
  this slice.
- The target `setgroups`/`setresgid`/`spawn` path is not exercised on the host (the host `main`
  is a stub); the host tests cover the pure parser, group lookup, membership rule and shell plan.
  The credential/exec calls are the same ones `su` uses.
- No new server, kernel primitive, shell or libc change.

## Acceptance

```
sh tests/posix/utility/run-simple4.sh
cargo nextest run -p newgrp
cargo xtask posix-check --profile posix --arch x86_64 --host-only
make check-fast
```

Evidence: `docs/posix/evidence/P4-simple4.toml`.
