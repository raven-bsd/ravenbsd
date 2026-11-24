<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH (upstream OpenSSH add-on)

SSH is the upstream OpenSSH 10.6p2 add-on (DR-65). The Raven tree does not vendor a second
implementation and does not implement SSH in Rust. K200 owns `addons/ssh/**` and the source pin;
K195 stages the built binaries as a release service and owns the login gate.

Status: **build, service staging, the `sshd` account, libc `pipe(2)`, pollable pipes, the
scheme-socket poll wake and the post-auth read EFAULT (K429) are DONE; the host-to-guest login is
BLOCKED at the session delivery stage.** The banner, KEX, password authentication, the channel
confirmation composition and the write wake all work: the 44-byte confirmation is written to netd
promptly. The bytes do not reach the peer in every run (the intermittent 0-byte peer probe
persists), so the command sessions still fail; `ssh -tt` additionally fails with `PTY allocation
request failed on channel 0` (K430). Login is not claimed; the real tap gate is **0/10**.

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
| `/etc/passwd`, `/etc/group` | `profiles/ssh.d/{passwd,group}` (root, uid/gid 74 `sshd`, raven) |
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

The release account database (`profiles/ssh.d/{passwd,group}`, wired by
`profiles/release.d/login.toml`) keeps `root` and the unprivileged `raven` account (K229) and adds
the standard OpenSSH privilege-separation account: `sshd:x:74:74:SSH privilege
separation:/var/empty:/bin/false` (group `sshd:x:74:`). Running as root, `sshd` refuses to start
without `getpwnam("sshd")`; the account is a system account with no password and no shell.

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
timeout 300 sh tests/ssh/ssh-login.sh
```

`tests/ssh/ssh-login.sh` is the whole gate in one command. It brings the bridge up with
passwordless sudo, then runs, against one boot:

* a raw byte probe with the K375 fake peer tracer (`tests/fixtures/ssh-peer`) before, between and
  after the sessions, to see whether the guest answers a banner+KEXINIT+ECDH exchange;
* ten `ssh raven@10.0.2.15 ls` command sessions (`RAVEN_SSH_REPEAT` overrides the count);
* the `ssh -tt` PTY case (`pwd`/`exit`);
* the wrong-password refusal.

A failure is classified and quoted with its evidence (client line, raw-probe transcript, guest
serial), and `build/ssh/login-test/classification.txt` carries the table:

| Class | Meaning | Exit |
|---|---|---|
| `a` | banner not received (bytes arrived, no usable SSH banner) | 10 |
| `b` | TX wedge: the fresh peer read 0 bytes (or only the banner with no SSH record), netd `TxError::Busy` territory | 11 |
| `c` | auth refused (password prompt reached, password rejected or a wrong password accepted) | 12 |
| `d` | session/command failure (authenticated, command or PTY did not complete) | 13 |

`0` is a full pass (10/10 command sessions + PTY + wrong-password refusal); `2` is a setup error.
While K382 (virtio-net TX) is open the gate is expected to exit 11 with class `b` and the 0-byte
evidence; login stays unclaimed until the 10/10 run passes after K382.

The qtest wrapper (`timeout 300 cargo xtask qtest --case ssh-login --arch x86_64 --no-build`) runs
the same script as its pre-step, so a failed gate carries the class into the qtest result.

## Blockers

One area remains before a login can be claimed; it is outside K195's Owns
(`tests/ssh/** tests/qtest/cases/ssh-* profiles/ssh.d/** profiles/release.d/** docs/ssh.md`).

1. **The virtio-net TX path wedges after a number of frames (platform).** The SSH protocol now
   reaches the session stage: `sshd -d` shows the banner, KEX and `Accepted password`, the client
   sends `CHANNEL_OPEN`, and `serverloop.c` composes the confirmation
   (`serverloop.c:650-660`); the client never logs `channel 0: open confirm`. The loop flushes only
   when the connection fd reports `POLLOUT` (`serverloop.c:216`, `395`, `305`), and the segment
   cannot leave the NIC.

   Fresh trace on the merged tree (K372), one boot per run:
   * host, tap `10.0.2.15:22`: `ssh raven@10.0.2.15 ls` — **0/10** (attempt 1 timed out, attempts
     2–10 closed at the banner after the 8 s ConnectTimeout);
   * guest `netstat -i` after the loop:
     `eth0 rx_packets=122 tx_packets=132 rx_bytes=11566 tx_bytes=15642 rx_errors=0 tx_errors=37
     rx_drops=16 tx_drops=0`;
   * guest `netstat -a` during the stall:
     `tcp ESTABLISHED 10.0.2.15:22 remote=10.0.2.2:42320 iface=eth0 rx=0 tx=0`;
   * guest `/tmp/sshd.log` (INFO): `Server listening on 0.0.0.0 port 22.`, then the connection is
     accepted and disconnected; with `sshd -d` the same boot shows the confirmation composed but
     the client never receives it;
   * driver boot log: `virtio-net: queue0 rx size=8`, `virtio-net: queue1 tx size=8`,
     `virtio-net: ring offset=0x1000 len=24576 rx=8 tx=8 pool=22528 dma=65536`,
     `devd: driver virtio-net msix unavailable: bad handle or file descriptor; using legacy irq`.

   Exact code path: netd's `NicDevice::transmit` returns `TxError::Busy` when `tx_free` is empty
   (`userland/servers/netd/src/nic.rs:243-245`); `reclaim_tx` only returns buffers when the
   driver advances `header.tx_cons` (`nic.rs:175-188`); the counter is incremented in
   `crates/raven-netcore/src/backend/counting.rs:58`. The driver has one TX in flight and only
   reclaims after the virtio used index advances (`userland/drivers/virtio-net/src/main.rs:731-763`),
   with an 8-entry TX frame ring and an 8-entry virtqueue
   (`userland/drivers/virtio-net/src/lib.rs:438-442`). No `virtio-net: pump:` error is logged, so
   the driver is waiting for a device completion that never arrives; the NIC TX ring stays full
   and every later TCP segment is dropped at the NIC.

   The TCP send state (`snd_una/snd_nxt/snd_wnd/snd_cwnd`, persist deadline) is not exposed by any
   guest facility: `netstat` prints the socket state and `rx`/`tx` queue bytes only. Reading the
   `raven-tcpip` `TcpState` fields needs a kstat/API in netd or the stack, which is outside this
   ticket's Owns; the socket traffic confirms the connection is ESTABLISHED with empty netd
   queues while the NIC counters show the TX errors.

K223's per-process root/`chroot`, K196's fork/PTY, the K206/K210/K211 AF_UNIX pair, K370's libc
`pipe(2)`, K372's pollable pipe endpoints and the `sshd` account are in place; they are not
blockers.

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| SSH implementation | no in-house SSH | OpenSSH portable, BSD-style, pinned upstream | OpenSSH is the reference implementation | Vendor OpenSSH unmodified; no self-written protocol/crypto. |
| Crypto | OpenSSL add-on (DR-45) | OpenSSH portable links OpenSSL | same | Link the T1 OpenSSL add-on; no new crypto. |
| Process/session | POSIX fork/PTY | OpenSSH expects fork/pty; K196 supplies them | same | Fix the OS primitives; do not patch SSH into a different model. |

No donor code is copied into the base scaffold; OpenSSH stays an unmodified C add-on.
Donor revisions: OpenSSH `d449e662…` (V_10_6_P2), T1 OpenSSL `636dfadc…` (3.5.0).
