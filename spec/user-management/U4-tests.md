<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# U4 - user-management test matrix

Lane: bob      Wave: 12      Milestone: M-0.2.0
Depends: U2, U3      Blocks: 0.2.0 user-management gate
Estimated size: M   Type: tests/verification
Status: draft (from U1)
Risk: medium

Owns-globs: tests/qtest/cases/user-management* tests/fixtures/user-management/qtest/** 
tests/login/** docs/user-management-tests.md profiles/*user-management* Cargo.toml

## Why

U1-U3 need one executed record that covers the account files, tools, NSS, permissions and the
root-only migration. The 0.1.0 login gate must remain green while non-root accounts are added.

## Contract

* Add a QEMU fixture/controller that boots the release/0.2.0 profile and runs:
  root login, `useradd -m alice`, `passwd alice`, `id alice`, `su - alice`, `whoami`, home
  mode/owner, `groupadd`, `id -G`, duplicate name/uid rejection, locked account, wrong password,
  non-root `/etc/shadow` denial, and a malformed-file root-recovery case.
* Add the negative matrix from U1: uid/gid collision, missing primary group, >16 groups, setuid
  script, `nosuid`, setuid file without execute, malformed shadow, empty password field, path
  traversal in `-d`, and a non-root edit attempt.
* Reuse the canonical fixture records in `tests/fixtures/user-management/` (U1); keep new QEMU-only
  data under `qtest/` so U1's files stay stable. The root release hash is REL1b's vector.
* Run `shell-session`/REL2b as a regression; do not weaken the root/`raven` gate or the existing
  login tests.
* Record the exact serial transcript and exit statuses in the report; no board claim without a
  board log (this ticket is QEMU-only unless the lead extends it).

## Tests and acceptance

```
cargo xtask qtest --case user-management --arch x86_64
cargo xtask qtest --case shell-session --arch x86_64
cargo xtask qtest --case linux-root --arch x86_64
make check-fast
```

The case is DONE only if the positive and negative steps all print their expected lines and the
root login gate still exits 0. A missing user tool, an unenforced mode or a setuid fake success is
a failure.

## Report

`tickets/bob/reports/U4.md` with the transcript, the matrix table (case -> expected -> observed)
and the 0.2.0 gate result.
