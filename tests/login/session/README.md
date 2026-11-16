<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# REL1 login session fixture

This directory is the test-only session for REL1. It builds a `shell-test` initfs, replaces
`/etc/services.d/rsh.toml` with the `getty` service, and overlays the binaries and account files
needed for a real serial login.

| File | Purpose |
|---|---|
| `build.sh` | builds `login`/`getty`, the `shell-test` initfs and the overlay |
| `login-session.toml` | qtest dialogue: banner, `root`, `raven`, motd, `/bin/rsh`, `exit` |
| `getty.toml` | one-shot `getty` service; `readiness = "exit"`, initfs grant |
| `svcd.conf` | `exit_when_exits = getty` |
| `etc/passwd`, `etc/group` | root account; the shell field selects `/bin/rsh` |
| `etc/shadow` | REL1b's deterministic SHA-512-crypt root record for `raven` |
| `etc/profile` | `PATH`, `HOME`, `TERM`, `USER`, `SHELL` |
| `etc/motd` | login message |

The production `/etc/passwd`, `/etc/group`, `/etc/shadow`, `/etc/profile`, `/etc/motd` and the
profile service fragment are REL1c's files. This fixture exists so REL1 can prove the login
without touching REL1c's Owns; it is not installed by any release profile.

The shell field is `/bin/rsh` because the brush target build is still skipped. `getty` reads the
shell path from `/etc/passwd`, so switching the release account to `/bin/brush` is a data change,
not a login-code change.

```sh
cargo nextest run -p login
cargo xtask qtest --case tests/login/session/login-session.toml --arch x86_64
```
