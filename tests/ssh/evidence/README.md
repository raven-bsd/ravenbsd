<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K195 SSH login evidence

Product base `origin/master` `90ce9282`; the K195 staging is in the `tim/K195` test-only
commits. The release image is `build/qtest/ssh-login.img` (335,544,320 bytes) built from that
clean tree with `profiles/release.d/85-ssh.toml` and the `sshd` account.

| Gate | Command | Exit | Result | Log |
|---|---|---:|---|---|
| add-on check | `sh addons/ssh/build.sh --check` | 0 | `OpenSSH_10.6p2`, T1 present after build | report |
| add-on build | `sh addons/ssh/build.sh` | 0 | five binaries in `build/ssh/x86_64` | report |
| host `sshd -V` | `build/ssh/x86_64/bin/sshd -V` | 124 | host kernel cannot execute the static RavenBSD binary; guest version below | `sshd-V-host.log` |
| guest version | `cargo xtask qtest --case ssh-version --arch x86_64` | 0 | **pass**: `OpenSSH_10.6p2, OpenSSL 3.5.0` | `ssh-version.log` |
| pipe(2) probe | `sh tests/ssh/pipe-probe.sh` | 0 | **pass**: `pipe-probe: ok` | `pipe-probe.log` |
| host SSH login | `cargo xtask qtest --case ssh-login --arch x86_64 --no-build` | 1 | **FAIL** at pre-step exit 3: `Connection closed by 10.0.2.15 port 22` before the banner | `ssh-login-qtest.log`, `client.log`, `serial-1.log` |
| release regression | `cargo xtask qtest --case release-login --arch x86_64 --no-build` | 0 | **pass** at the measured 900 s cap (the 300 s cap timed out in the loader pre-step) | `release-login.log` |
| check-fast | `make check-fast` | 0 | **`check-fast: PASSED`**, 181/181 changed tests | `check-fast.log` |
| docs | `cargo xtask docs` | 0 | 219 pages | report |

## Current login failure

The tap serial (`serial-1.log`) has:

```text
netd: interface eth0 10.0.2.15/24
netd: ready
svcd: netd up
svcd: ssh-hostkeys up
svcd: sshd up
svcd: all services up
```

The host client (`client.log`) closes before the banner:

```text
kex_exchange_identification: Connection closed by remote host
Connection closed by 10.0.2.15 port 22
```

The server accepts TCP 22 but closes the SSH transport. The prior K195 `/tmp/sshd.log`
evidence names `monitor_read: poll: Bad file descriptor`; `kernel/src/syscall/event.rs`
`source_from_handle`/`readiness` has no `Object::Pipe` arm and `kernel/src/process/pipe.rs`
does not publish pipe readiness to the event path. This is the first OPEN blocker. The prior
hostfwd trace (`tickets/bob/reports/K195.md` rounds 32-33) then shows the netd queued-TX
confirmation (`tx=(44, 0, 37, 4454)`) not flushed; that is the next blocker after the pipe
poll.

The initfs image's host key is generated in `/tmp/ssh` and is ephemeral. The K193 UFS qcow2
persistent `/etc/ssh` path is not in this clone; persistence is not claimed.

## Evidence files

* `serial-1.log` — the tap boot with `svcd: sshd up` and the netd interface/ready lines.
* `client.log` — the host OpenSSH client close before the banner.
* `ssh-login-qtest.log` — the qtest pre-step error with the single-line stage.
* `ssh-version.log` — the guest upstream version smoke.
* `pipe-probe.log` — the target `pipe(2)` round-trip.
* `release-login.log` — the release login regression.
* `check-fast.log` — the ticket gate.
* `sshd-V-host.log` — empty; the host `sshd -V` was killed by the ticket's 60 s cap.

The test harness (`tests/ssh/ssh-login.py`) waits for the command output, catches
`pexpect` TIMEOUT/EOF and exits 3 with the last client line. The first run raised an uncaught
pexpect exception; the harness fix only names the stage and does not weaken the login assertion.
