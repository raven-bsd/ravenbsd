<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# S85k 0.1.0 SSH login gate

The 0.1.0 release gate for a real host OpenSSH login to the guest. It is test-only: no kernel,
sshd, add-on or profile behaviour is changed here.

## Run

```sh
timeout 600 sh tests/ssh/0.1.0-login-gate/run.sh --arch x86_64 --accel kvm
timeout 600 sh tests/ssh/0.1.0-login-gate/run.sh --arch x86_64 --accel tcg
timeout 600 sh tests/ssh/0.1.0-login-gate/run.sh --arch aarch64 --accel tcg
```

`run.sh` builds the `release` profile image with `cargo xtask image` unless `--image PATH` names a
prebuilt image. It boots the image with QEMU user networking and a host-forwarded port, proves the
guest `/sbin/sshd`, `/usr/libexec/sshd-session` and `/usr/libexec/sshd-auth` are staged, runs the
host `ssh` client with the marker command `echo S85K-SSH-LOGIN-OK`, then runs a `pwd` PTY row and
asserts a clean close.

The console login is used only to inspect the staged sshd; it is never accepted as an SSH result.
A missing sshd, missing split-server binaries, no SSH banner, host-key/handshake failure,
authentication refusal, PTY failure and session failure each have their own stage and exit code
(see `run.sh --help`).

## Evidence

The runner writes the serial transcript, host-client logs, the per-cell summary and a
`SHA256SUMS` file under `tickets/evidence/artifacts/S85k/` in `ravenbsd-prjmgmt`. It resolves
that directory from `RAVEN_EVIDENCE_DIR` or by walking up to the workspace root; it never writes
evidence into the product tree.

## Mutation

`--password wrong` is the authentication mutation: with sshd present the gate must fail at the
`authentication` stage. On an image without sshd the gate fails earlier at `sshd_missing`; both are
stage assertions, never a pass.

## Unit checks

```sh
cd tests/ssh/0.1.0-login-gate
timeout 30 python3 -m unittest -v gate_test.py
```

The classifier tests pin that a console transcript, a refused password, a failed handshake, a
failed PTY request and a missing clean close are all failures.
