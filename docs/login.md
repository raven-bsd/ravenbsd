<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Login, getty and the first session (REL1/SESSION1)

Status: REL1 implementation plus SESSION1 retry and the full brush session. `getty` owns
`tty:console`, prints the banner and `login:` prompt, and retries the attempt on every failure;
`login` verifies the root password against the REL1b shadow record, prints the motd and exits 0;
`getty` then starts brush as a login shell. There is no setuid binary and no hand-written crypto.

## Session flow

```
svcd -> /sbin/getty (grants: initfs)
          banner + "login: "
          read user name
          spawn /bin/login <user>; wait
          status 0 -> spawn /bin/brush -i -l with the profile environment
                      (or --rcfile /etc/brush/brushrc where the image stages it)
          status != 0 -> "Login incorrect" came from login; prompt again
          invalid name / login spawn failure -> clear error; prompt again
        /bin/login
          "Password: " with tty echo off
          read /etc/shadow -> raven_auth::verify_root_shadow(root line, password)
          restore echo; flush console input
          on success: print /etc/motd, exit 0
          on failure: "Login incorrect", exit 1 (LOGIN_RETRY)
```

**Why the two-step spawn:** the kernel has no `exec`, and a spawned child does not inherit the
service's `initfs` grant. `getty` therefore owns the grant and does both spawns; `login` only
verifies and returns status. This keeps the password path small and avoids giving an
authenticator an image loader. **OBSERVED** in `userland/bin/getty/src/main.rs` and
`userland/bin/getty/src/lib.rs`.

The retry loop is `raven_getty::run_session`: a nonzero/error status writes the prompt again and
never exits the session. A zero status is the only path to the shell. Unit tests in
`userland/bin/getty/src/lib_ut.rs` cover a wrong password, an invalid name, a failed `login`
spawn, and a session that stays open after repeated failures.

## Account files

| File | Content | Owner |
|---|---|---|
| `/etc/passwd` | `root:x:0:0:root:/:/bin/rsh` | REL1c (production), `tests/login/session/etc/` (test) |
| `/etc/group` | `root:x:0:root` | REL1c / test fixture |
| `/etc/shadow` | one root `$6$` line from REL1b | REL1c / test fixture |
| `/etc/profile` | `PATH`, `HOME`, `TERM`, `USER`, `LOGNAME`, `SHELL`, `PS1` | REL1c / test fixture |
| `/etc/motd` | login message | REL1c / test fixture |

REL1 does not own the production `/etc` files; `tickets/INDEX.md` assigns them to REL1c
(`mel`). The test fixture under `tests/login/session/etc/` is the same shape so the login path
can be proven now. The release image must install REL1b's `default_root_shadow()` or a fixed
record with the same format; no plaintext password is shipped. **OBSERVED** in
`docs/auth.md` and the REL1c ticket.

## Password verification

`login` reads `/etc/shadow` through the `file:` scheme, selects the nine-field `root` line and
calls `raven_auth::verify_root_shadow`. REL1b uses the vetted `sha-crypt` 0.6 crate
(SHA-512-crypt, `$6$`, constant-time verify). Locked (`!`, `*`) and empty hashes never verify;
any account other than `root` is rejected. The release password for 0.1.0 is `raven`; the hash is
generated at build time from a fixed salt so the image is reproducible. **OBSERVED** in
`crates/raven-auth/src/shadow.rs` and `userland/bin/login/src/lib_ut.rs`.

The password is read with the `tty:` `SET_MODE` echo bit off and restored afterwards; the
console driver's canonical line discipline still delivers the line. The password is not written
to the console, a log, or a file. The login status contract is `login::LOGIN_OK` (0) and
`login::LOGIN_RETRY` (1); getty loops on any nonzero status. **OBSERVED** in
`userland/bin/login/src/lib.rs` and `crates/raven-proto-tty`.

## Environment and shell

`getty` parses `/etc/profile` as flat `KEY=VALUE` lines and passes `PATH`, `HOME`, `TERM`,
`USER`, `LOGNAME`, `SHELL` and `PWD=/` in the shell spawn environment. The shell path is login
policy: `parse_root_passwd` resolves root to `/bin/brush` while keeping the passwd field as
account metadata. Missing values fall back to `PATH=/bin:/sbin`, `HOME=/`, `TERM=vt100`,
`SHELL=/bin/brush`. **OBSERVED** in `userland/bin/getty/src/lib.rs`.

`/etc/profile` sets the Raven Violet `PS1` (`38;5;183` around the visible `raven# ` text),
registers `complete -W 'help exit' raven-test`, and exports `RAVEN_PROFILE=loaded`. When the image
stages `/etc/brush/brushrc` (the Pi 4 release), the profile sources it for the full user@host
prompt, history and completion; the x86_64 release keeps the profile-only prompt. Both release
paths load `/etc/profile`, because getty starts brush with `-i -l`. **OBSERVED** in `etc/profile`,
`etc/brush/brushrc` and `profiles/pi4-release.d/20-userspace.toml`.

Brush's own `--help` renderer colors only when stdout is a terminal
(`userland/brush/brush-shell/src/entry.rs`). The `login-retry` qtest runs it on the console TTY,
and captures it through a pipe to prove the plain path. The qtest harness strips ANSI before
matching, so the machine-checked color evidence is the prompt text (`38;5;183`) plus the pipe
result; the raw TTY escape sequence is not a separate assertion.

## Test fixture and evidence

```sh
cargo nextest run -p raven-getty -p raven-login
cargo xtask qtest --case login-retry --arch x86_64
cargo xtask qtest --case tty-control --arch x86_64
cargo xtask qtest --case tests/login/session/login-session.toml --arch x86_64
cargo xtask qtest --case shell-session --arch x86_64
```

`tests/login/session/build.sh` builds `login` and `getty` for `x86_64-unknown-none`, builds the
`shell-test` initfs, and overlays the test account files, the getty service and the one-shot
`svcd.conf`. The qtest dialogue is:

```
RavenBSD 0.1 (x86_64) (tty:console)
login: root
Password:
RavenBSD 0.1 (x86_64)
Welcome to RavenBSD.
raven# uname -a
RavenBSD raven 0.1 ... x86_64
raven# exit
init exited (0)
```

`login-retry` drives the wrong-password path first:

```
login: root
Password:
Login incorrect
login: root
Password:
raven# ...
```

## Security notes

* No setuid, no password file write, no plaintext password on the console or in the initfs.
* Root-only: the getty normalises the user name; the shadow verifier rejects non-root lines and
  locked hashes. Unknown accounts get the same `Login incorrect` as a wrong password.
* Input is bounded: 32-byte user names, 256-byte lines and profile values, 256-byte account
  fields; malformed passwd/profile/shadow lines are ignored rather than acted on.
* Retry is a prompt loop, not a hidden bypass: every rejection prints `Login incorrect` and
  re-prompts. There is no lockout, utmp/wtmp, session accounting or multi-user support yet.
* The console itself is trusted: `consd` runs as the console server and the serial line is the
  only input. There is no remote login in 0.1.0.

## Open items

* REL1c must install the production `/etc` files and the getty/login service fragment.
* REL2a/REL2b select the release profile and add the release-login qtest; brush replaces
  `/bin/rsh` when K26a unblocks.
* A future user-management ticket (U1) replaces the single root record with a real account
  store and adds lockout and session accounting.
* The target `tty-control` case currently stops at the fixture's `WCONTINUED=8` while the kernel
  raw ABI is `WCONTINUED=4`; see the SESSION1 report for the exact files and owner.
