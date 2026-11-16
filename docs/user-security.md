<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Adversarial credential-transition review (U3e)

OBSERVED on `stuart/U3e` (base `origin/master` `ab3e577a`). Test-only change: no kernel, libc,
`raven-rt`, `raven-userdb` or tool source was modified. No board claim.

## Scope

`su` and `passwd` are the only tools that both authenticate a password and change process
credentials. The review attacks the decisions before the transition, not only the diagnostic
string:

* a failed password, a locked `!` hash, a missing shadow record and an unknown account must not
  install a target uid/gid;
* a `passwd` old-password mismatch or a non-root target must not touch the account files;
* the target environment, supplementary group list and saved ids come from the target account,
  never from the caller;
* two `passwd` writers must serialize on `/etc/.pwd.lock` and leave no partial update.

The host matrix drives the real pure functions (`su::verify_target_password`,
`su::environment`, `su::supplementary_groups`, `passwd::verify_old_password`,
`passwd::target_allowed`, `passwd::validate_password`) and the real
`raven-userdb::store::Transaction` lock. It models the credential install with a caller snapshot
that is mutated only after every check succeeds. The target fixture runs the actual setuid
binaries; it is the only place the six credential words and the console prompts are real.

## Host cases

`cd tests/user/secure && cargo nextest run` (standalone test workspace; six tests):

| Case | Expected | Observed |
|---|---|---|
| wrong target password as uid 1000 | `BadPassword`; caller uid/euid/gid/egid/groups unchanged | pass |
| nonexistent target | `NoSuchUser`; caller unchanged | pass |
| target with no shadow record (`nobody`) | `NoShadow`; caller unchanged | pass |
| locked `!` hash, including the correct password | `Locked`; caller unchanged | pass |
| root bypass to a regular account | plan sets uid/euid/suid/gid/egid/sgid to the target; no saved root id | pass |
| empty shell/home in passwd | `shell_path` falls back to `/bin/brush`; `HOME=/`, login `PWD=/` | pass |
| caller `PATH`/`HOME`/`SHELL`/`USER`/`LOGNAME`/`LD_PRELOAD`/`IFS` | rebuilt fixed list; only `TERM` is appended by `main`; one entry per key | pass |
| `passwd` old-password mismatch | `BadOldPassword`; passwd/group/shadow byte-for-byte unchanged; old hash still verifies | pass |
| non-root changing another account | `Denied`; files unchanged | pass |
| locked hash for `passwd --change` | `BadOldPassword`; files unchanged | pass |
| root one-character root password vs regular eight-character minimum | root policy only for `uid 0 -> uid 0` | pass |
| alice in `users`, then `wheel` | group list `[1000,100]` then `[1000,0,100]`; primary removed; duplicates collapse | pass |
| two `passwd` transactions | second sees `Busy`; winner commits; retry sees the new hash; lock removed; no lost update | pass |

## Target qtest

`tests/qtest/cases/user-secure.toml` and `tests/user/secure/fixture/` build an initfs with
setuid `/bin/su` and `/bin/passwd`, the four services, a writable ramfs `/etc`, and a serial
dialogue for the prompts. The case is `skip`ped because the first target read already fails for a
product reason:

```
secure-debug: etc=invalid argument
secure-debug: client-etc=open
secure: fail open file
```

The fixture's account install uses the same plain `/etc/passwd` path as
`raven-userdb::rt::RtIo`. `raven-rt::fs::open` resolves the path but passes it to the kernel
without the `file:` scheme:

* `userland/rt/src/fs.rs`: `open` calls `resolve(path)` and then
  `Client::open(&resolved, flags)`. `resolve` returns an absolute path such as `/etc/passwd`.
* `kernel/src/ipc/model.rs`: `split_path` requires a `scheme:` separator and returns
  `MissingScheme` otherwise; the syscall maps that to `EINVAL`.
* `crates/raven-userdb/src/rt.rs`: `RtIo::read` calls `fs::read_to_vec("/etc/passwd")` (plain).

`Client::open("file:/etc/passwd", READ)` succeeds in the same process. The same defect blocks
every target tool that uses `RtIo`/`Paths::system()` (`useradd`, `adduser`, `groupadd`, `su`,
`passwd`, and the lookup paths in `id`/`whoami`). The `user-secure` case is therefore kept as a
ready harness but skipped until the layer is fixed; it must not be enabled by weakening its
assertions.

**Follow-up ticket: `U3f-rt-fs-plain-path`.** In `raven-rt::fs::open`, prefix a path without a
scheme with `file:` after `resolve` (or make `resolve` return the scheme path), and add a host
test for `"/etc/passwd"` and `"file:/etc/passwd"` producing the same target request. The minimal
target repro is the two lines above; the source path is a single `open` call. U3f also re-enables
this `user-secure` case.

## Other observations

* `docs/user-management.md` section 7.3 says `su` preserves the real uid and sets effective/saved
  ids. The shipped target code calls `proc::setresuid(target)`/`setresgid(target)`, which sets all
  three ids to the target. The code is the safer behavior (a root caller does not keep real uid 0
  in the target shell); the documentation sentence should be corrected. The host plan and the
  target probe both assert all three ids.
* `docs/user-management.md` also says a member of `wheel` may `su` to root. The shipped `su` has
  no caller-group check; every non-root caller must supply the target password. That is stricter
  than the document, not a privilege escalation, but the document or a future wheel policy must
  be reconciled. U3e does not add a wheel bypass.
* A failed `su` authentication has no delay in the shipped path. The document mentions a failed
  auth delay; this is a brute-force hardening gap, not a privilege transition.
* `passwd` takes `/etc/.pwd.lock` before the interactive old/new prompts. A second writer fails
  `Busy` after the bounded wait rather than waiting for the typist. This is a usability limit, not
  a corruption or privilege bug; the host concurrency case asserts the lock/commit ordering.
* U3d's setgid-directory inheritance defect in `ramfsd` remains open; it is unrelated to this
  transition and is not asserted here.
* The target qtest does not cover concurrent `passwd` writers or a successful password change;
  those need `rand:` and two simultaneous console dialogues. The host transaction case is the
  deterministic evidence for the lock, and a successful target change belongs to the follow-up
  that fixes plain paths.

## Acceptance evidence

```
$ cargo nextest run -p su -p passwd
pass
$ cargo xtask qtest --case setuid --arch x86_64
setuid: pass
$ make check-fast
check-fast: PASSED
```

The host secure matrix is a separate standalone workspace because U3e does not own the root
`Cargo.toml` member list:

```
$ (cd tests/user/secure && cargo nextest run)
6 tests run: 6 passed
```
