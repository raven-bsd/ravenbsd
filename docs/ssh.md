<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH (upstream OpenSSH add-on)

SSH is the upstream OpenSSH 10.6p2 add-on (DR-65). The Raven tree does not vendor a second
implementation and does not implement SSH in Rust. K200 owns `addons/ssh/**` and the source pin;
K195 stages the built binaries as a release service and owns the login gate.

Status: **build and service staging DONE; host-to-guest login BLOCKED** on the platform pipe
poll/event readiness, then the netd queued-TX flush (see [Blockers](#blockers)). The release
image boots with the `sshd` service manifest, the host-key one-shot and the K196 `ptyd`; the
`sshd` privilege-separation account is staged by this ticket and the libc `pipe(2)` is fixed by
K370. The current `ssh-login` pre-step reaches `svcd: sshd up` and an accepted TCP 22 connection,
but closes at `kex_exchange_identification: Connection closed by remote host` before the SSH
banner. The initfs demo's `/tmp/ssh` host key is ephemeral; UFS persistence is not claimed.

## Pinned source

| | |
|---|---|
| Source | OpenSSH portable, `addons/ssh/openssh/` (unmodified) |
| Tag | `V_10_6_P2` (peeled commit `d449e662b6b8b8cb20e852d8084c269af1c5e8f1`) |
| Version | `OpenSSH_10.6p2` (`version.h`) |
| Licence | OpenSSH `LICENCE` (BSD-style, ISC-compatible) |
| Provenance | `PROVENANCE.toml` import `addons/ssh/openssh`; `.provenance-ignore` exempts the tree |

The build boundary and the split `sshd-session`/`sshd-auth` binaries are documented in
[`docs/ssh-addon.md`](ssh-addon.md) (K200/K212). This page covers the service and the gate.

## Build and stage

```sh
timeout 60 sh addons/ssh/build.sh --check   # pinned source, provenance, host tools
timeout 600 sh addons/ssh/build.sh          # T1 OpenSSL prerequisite, then configure/make
```

Outputs: `build/ssh/x86_64/bin/{sshd,ssh,ssh-keygen}` and
`build/ssh/x86_64/libexec/{sshd-session,sshd-auth}`. `--check` passes; the host cannot execute
the RavenBSD static binaries (`sshd -V` hangs), so the version evidence is the guest
`ssh-version` qtest (`cargo xtask qtest --case ssh-version --arch x86_64`).

`profiles/release.d/85-ssh.toml` stages the binaries read-only in the release/UFS profile:

| Target | Source |
|---|---|
| `/sbin/sshd`, `/bin/ssh`, `/bin/ssh-keygen` | `build/ssh/x86_64/bin/` |
| `/usr/libexec/sshd-session`, `/usr/libexec/sshd-auth` | `build/ssh/x86_64/libexec/` |
| `/etc/ssh/sshd_config` | `profiles/ssh.d/sshd_config` |
| `/etc/services.d/{sshd,ssh-hostkeys,ptyd}.toml` | `profiles/ssh.d/*.service.manifest` |
| `/sbin/ptyd` | `raven-ptyd` (K196 pty scheme) |
| `/var/empty/.keep`, `/etc/ssh/.keep` | directory anchors |

`sshd` and its one-shot services are `required = false` and `restart = "never"`: a failed SSH
service is reported but does not turn a successful boot into a failed image, and a daemon that
exits must stay exited and visible rather than loop. The `release-login` gate is therefore
unaffected until the platform primitives land.

## Service set

* `ssh-hostkeys` — one-shot `/bin/rsh -c "mkdir -p /tmp/ssh; ssh-keygen -q -t ed25519 -N '' -f
  /tmp/ssh/ssh_host_ed25519_key"`. The initfs root is read-only, so the demo key lives in the
  `/tmp` ramfs and is **ephemeral across boots**. A persistent UFS root must stage the same
  command against `/etc/ssh`; that image path is not merged, so persistence is not claimed.
  `-A` is not used: the K200 configure baked an absolute host `SSHDIR` into the binaries, so
  `ssh-keygen -A` would write to `<host-build-dir>/etc/ssh` inside the guest.
* `ptyd` — the K196 `pty:` scheme used by interactive `ssh -tt` sessions.
* `sshd` — `/sbin/sshd -D -E /tmp/sshd.log -f /etc/ssh/sshd_config`. `-D` keeps the listener in
  the foreground for svcd, `-E` writes the daemon log to the writable `/tmp` (services start
  with no stdio fds), and `-f` pins the target config because the compiled-in `SSHDIR` is the
  host build directory.

Demo policy (`profiles/ssh.d/sshd_config`): `PermitRootLogin yes` and
`PasswordAuthentication yes` with the image password from `docs/auth.md` (`raven`); `root` and
the unprivileged `raven` account (K229) may log in. `PermitEmptyPasswords no`,
`AuthorizedKeysFile none`, no PAM, no shipped key, no world-readable private key. This is a
0.1.0 demo policy; do not expose it to an untrusted network.

## Network and the login gate

QEMU user networking is one-way for inbound connections. The K194 tap/bridge makes the guest
reachable from the host at `10.0.2.15`:

```sh
cargo xtask image --arch x86_64 --profile release --output build/qtest/ssh-login.img
timeout 300 cargo xtask qtest --case ssh-login --arch x86_64 --no-build
```

The qtest pre-step (`tests/ssh/ssh-login.sh`) brings the bridge up with passwordless sudo, boots
the prebuilt image on the tap, and drives the real host OpenSSH client
(`tests/ssh/ssh-login.py`):

* `ssh raven@10.0.2.15 uname -a` must return the RavenBSD uname;
* `ssh -tt raven@10.0.2.15` must serve `pwd`/`exit` through a PTY;
* a wrong password must be refused without reaching a shell;
* the host key is captured before and after a second boot (ephemeral on the initfs demo).

The current pre-step exits 3: the serial has `svcd: sshd up` and `netd: ready`, the TCP 22
connection is accepted, and the client closes at `kex_exchange_identification: Connection closed
by remote host` before the banner. This is the pipe poll/event gap; the prior guest
`/tmp/sshd.log` line is `monitor_read: poll: Bad file descriptor`. After that, the netd
queued-TX flush is the next gate.

`RAVEN_SSH_REPEAT=10 sh tests/ssh/ssh-login.sh` runs the command session ten times and reports
the wall-time p50/p90 for the 0.1.0 release gate. The direct command, once unblocked, is:

```sh
sh tests/ssh/ssh-login.sh
# or, with the bridge already up and the image booted on the tap:
ssh -o StrictHostKeyChecking=no raven@10.0.2.15 ls
```

## Blockers

1. **RESOLVED — `sshd` privilege-separation account.** `profiles/ssh.d/passwd`/`group` and
   `profiles/release.d/login.toml` stage root,
   `sshd:x:74:74:SSH privilege separation:/var/empty:/bin/false` and `raven`; `/var/empty` is
   staged. The current image reaches `svcd: sshd up` and accepts TCP 22.
2. **RESOLVED — libc `pipe(2)`/`pipe2(2)`.** K370 binds `PIPE2` (0x6f); the target
   `tests/ssh/pipe-probe.sh` round-trips a pipe. This is not the current gate.
3. **OPEN — pipe endpoints are not poll/event sources.** `kernel/src/syscall/event.rs`
   `source_from_handle`/`readiness` has no `Object::Pipe` arm and `kernel/src/process/pipe.rs`
   does not publish readable/writable readiness to the event path. Upstream
   `addons/ssh/openssh/monitor.c` `monitor_openfds()` (1907) / `monitor_read()` (495, poll at
   509-511) and `session.c` `do_exec_no_pty` poll those pipes. The prior `/tmp/sshd.log` evidence
   is `monitor_read: poll: Bad file descriptor`; the current `ssh-login` client closes at
   `kex_exchange_identification: Connection closed by remote host`. The exact files are
   `kernel/src/syscall/event.rs` and `kernel/src/process/pipe.rs`, outside K195's Owns.
4. **NEXT — netd queued-TX flush/doorbell.** Once the pipe poll is fixed, the prior K195
   hostfwd trace (`tickets/bob/reports/K195.md` rounds 32-33) shows the server's 44-byte
   channel-open confirmation queued in netd TX (`tx=(44, 0, 37, 4454)`) but not flushed; the
   client times out and the child exits 255. This is a netd/TCP egress issue, outside K195's
   Owns.
5. **FOLLOW-UP — UFS host-key persistence.** The release image is initfs-only; the demo host
   key is generated in `/tmp/ssh` and is ephemeral. The K193 UFS qcow2 image/staging is not in
   this clone, so persistence is not claimed; the ticket explicitly permits the ephemeral
   initfs demo.

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| SSH implementation | no in-house SSH | OpenSSH portable, BSD-style, pinned upstream | OpenSSH is the reference implementation | Vendor OpenSSH unmodified; no self-written protocol/crypto. |
| Crypto | OpenSSL add-on (DR-45) | OpenSSH portable links OpenSSL | same | Link the T1 OpenSSL add-on; no new crypto. |
| Process/session | POSIX fork/PTY | OpenSSH expects fork/pty; K196 supplies them | same | Fix the OS primitives; do not patch SSH into a different model. |

No donor code is copied into the base scaffold; OpenSSH stays an unmodified C add-on.
Donor revisions: OpenSSH `d449e662…` (V_10_6_P2), T1 OpenSSL `636dfadc…` (3.5.0).
