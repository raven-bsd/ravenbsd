<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Login, getty and the first session (REL1)

Status: REL1 implementation. `getty` owns `tty:console` and prints the banner and `login:`
prompt; `login` verifies the root password against the REL1b shadow record, prints the motd and
exits 0; `getty` then starts the shell named by `/etc/passwd` as root. There is no setuid binary
and no hand-written crypto.

## Session flow

```
svcd -> /sbin/getty (grants: initfs)
          banner + "login: "
          read user name
          spawn /bin/login <user>
          wait; only on exit 0:
          spawn <passwd shell> with PATH/HOME/TERM/USER/SHELL/PWD
          wait; exit with the shell status
        /bin/login
          "Password: " with tty echo off
          read /etc/shadow -> raven_auth::verify_root_shadow(root line, password)
          restore echo
          on success: print /etc/motd, exit 0
          on failure: "Login incorrect", exit 1
```

**Why the two-step spawn:** the kernel has no `exec`, and a spawned child does not inherit the
service's `initfs` grant. `getty` therefore owns the grant and does both spawns; `login` only
verifies and returns status. This keeps the password path small and avoids giving an
authenticator an image loader. **OBSERVED** in `userland/bin/getty/src/main.rs`.

## Account files

| File | Content | Owner |
|---|---|---|
| `/etc/passwd` | `root:x:0:0:root:/:/bin/rsh` | REL1c (production), `tests/login/session/etc/` (test) |
| `/etc/group` | `root:x:0:root` | REL1c / test fixture |
| `/etc/shadow` | one root `$6$` line from REL1b | REL1c / test fixture |
| `/etc/profile` | `PATH`, `HOME`, `TERM`, `USER`, `SHELL` | REL1c / test fixture |
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
any account other than `root` is rejected at both the getty username check and the shadow line.
The release password for 0.1.0 is `raven`; the hash is generated at build time from a fixed salt
so the image is reproducible. **OBSERVED** in `crates/raven-auth/src/shadow.rs` and
`userland/bin/login/src/lib_ut.rs`.

The password is read with the `tty:` `SET_MODE` echo bit off and restored afterwards; the
console driver's canonical line discipline still delivers the line. The password is not written
to the console, a log, or a file. **OBSERVED** in `userland/bin/login/src/lib.rs` and
`crates/raven-proto-tty`.

## Environment and shell

`getty` parses `/etc/profile` as flat `KEY=VALUE` lines and passes `PATH`, `HOME`, `TERM`,
`USER`, `SHELL` and `PWD=/` in the shell spawn environment. The shell path is the `/etc/passwd`
root shell; missing values fall back to `PATH=/bin:/sbin`, `HOME=/`, `TERM=vt100`,
`SHELL=/bin/rsh`. **OBSERVED** in `userland/bin/getty/src/main.rs`.

The current image's shell is `/bin/rsh`; the brush target build (K26a) is still skipped, so the
test `passwd` selects `/bin/rsh`. Changing the release account to `/bin/brush` is a data change
in REL1c/REL2a, not a login-code change. `rsh` currently initialises its own variables and does
not read inherited environment, so the environment handoff is proven by the `login`/profile
unit tests and will be consumed by brush; this is a known rsh limitation. **OBSERVED** in
`userland/bin/rsh/src/main.rs`.

## Test fixture and evidence

```sh
cargo nextest run -p login
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
rsh: ready
raven# uname -a
RavenBSD raven 0.1 ... x86_64
raven# exit
init exited (0)
```

The `shell-session` qtest remains the regression for the shell/consd/vfsd stack; REL1 does not
modify that case or its profile. **OBSERVED** in `tests/login/session/login-session.toml`.

## Security notes

* No setuid, no password file write, no plaintext password on the console or in the initfs.
* Root-only: the getty normalises the user name and rejects anything but `root`; the shadow
  verifier rejects non-root lines and locked hashes.
* Input is bounded: 32-byte user names, 256-byte lines and profile values, 256-byte account
  fields; malformed passwd/profile/shadow lines are ignored rather than acted on.
* There is no retry/lockout, utmp/wtmp, session accounting or multi-user support yet. A failed
  login exits nonzero and svcd ends the test session; a release getty should retry on a real
  console.
* The console itself is trusted: `consd` runs as the console server and the serial line is the
  only input. There is no remote login in 0.1.0.

## Open items

* REL1c must install the production `/etc` files and the getty/login service fragment.
* REL2a/REL2b select the release profile and add the release-login qtest; `brush` replaces
  `/bin/rsh` when K26a unblocks.
* A future user-management ticket (U1) replaces the single root record with a real account
  store and adds retry/lockout and session accounting.
