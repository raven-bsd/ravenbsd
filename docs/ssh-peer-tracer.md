<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K375 fake SSH peer tracer

Host-side test tooling for byte-exact SSH diagnosis of the guest `sshd`. It is **not** part of the
product: nothing here is linked into the image, and no OpenSSH, netd, TCP or libc source is changed.

The tool answers the questions a client-side timeout cannot: did the guest send its version
banner, what was the exact KEXINIT record, and did it answer the client key exchange with
`SSH2_MSG_KEX_ECDH_REPLY`, or send nothing / close?

## Build and run

```sh
sh tests/fixtures/ssh-peer/build.sh        # host peer + guest initfs under build/fixtures/ssh-peer
sh tests/fixtures/ssh-peer/run.sh          # Linux control, then the guest qtest
```

`run.sh` runs the local Linux OpenSSH server as the control, then boots the `ssh-peer` qtest with
guest port 22 forwarded to `127.0.0.1:2222`. Artifacts:

* `/tmp/opencode/ssh-peer-linux.log` - Linux banner-only control;
* `/tmp/opencode/ssh-peer-linux-kex.log` - Linux banner + KEXINIT + ECDH control;
* `build/fixtures/ssh-peer/peer.log` - the successful guest session;
* `build/fixtures/ssh-peer/peer.attempts.log` - every failed connection attempt, in order.

## Tool

`peer.c` is first-party C built with the host `cc`. Version exchange and packet framing follow RFC
4253 section 4.2 and RFC 4251 section 6; no donor source is copied.

```
peer --host H --port P --mode client [--send-banner] [--send-kexinit]
     [--idle-ms N] [--post-idle-ms N] [--log FILE]
```

* Every read and write is timestamped with `CLOCK_MONOTONIC` (ms) and flushed immediately, with a
  hex and printable dump. A zero-byte stall is an explicit `idle-timeout` line.
* `--send-banner` writes `SSH-2.0-FakePeer_1.0\r\n`. `--send-kexinit` writes a valid unencrypted
  KEXINIT after the server banner (random cookie, 10 name-lists, 4..255-byte padding, total a
  multiple of 8). After the server's KEXINIT it sends `SSH_MSG_KEX_ECDH_INIT` with a random
  32-byte curve25519 public key, which is what makes the server's ECDH_REPLY observable; without a
  client ECDH_INIT a correct server has nothing to reply to.
* The analyzer parses banners and binary packets: `packet_length`, `padding_length`, payload
  length, 8-byte alignment, message number/name, the KEXINIT name-lists, and counts
  KEXINIT/ECDH_REPLY/NEWKEYS. After NEWKEYS it observes `--post-idle-ms` for a close.
* Exit codes: 0 captured data, 2 connect failure, 3 connected but zero bytes.

## Proven Linux control

```text
[t=   43.496ms] banner length=42 value="SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.18"
[t=   17.533ms] packet length=1108 padding=10 payload=1097 aligned8=yes msg=20 (SSH2_MSG_KEXINIT)
[t=   69.687ms] packet length=188 padding=8 payload=179 aligned8=yes msg=31 (SSH2_MSG_KEX_ECDH_REPLY)
[t=   70.005ms] packet length=12 padding=10 payload=1 aligned8=yes msg=21 (SSH2_MSG_NEWKEYS)
```

## Guest result

`ssh-peer: pass` (qtest), peer exit 0. Guest serial reached `svcd: all services up`; the peer
captured the real OpenSSH 10.6 KEX byte-exactly:

```text
[t=15565.167ms] banner length=22 value="SSH-2.0-OpenSSH_10.6"
[t=15917.935ms] packet length=956 padding=9 payload=946 aligned8=yes msg=20 (SSH2_MSG_KEXINIT)
[t=16355.976ms] packet length=188 padding=8 payload=179 aligned8=yes msg=31 (SSH2_MSG_KEX_ECDH_REPLY)
[t=16356.257ms] packet length=12 padding=10 payload=1 aligned8=yes msg=21 (SSH2_MSG_NEWKEYS)
[t=16356.285ms] handshake NEWKEYS received; observing close for 2000ms
```

The guest KEXINIT offered `mlkem768x25519-sha256,sntrup761x25519-sha512,...` first; the client list
starts with `curve25519-sha256`, so the negotiated method is curve25519-sha256 and the reply carries
an `ssh-ed25519` host key. The reply packet is 192 bytes on the wire (packet_length 188, padding 8,
payload 179). The guest sent no `DISCONNECT` and no close within the 2 s post-handshake window.

**K195 hand-off:** the guest SSH framing and KEX are healthy. The banner, KEXINIT, ECDH reply and
NEWKEYS are all present and well formed, so the K195 post-auth channel-open stall is not a missing
or truncated KEX record. The next diagnostic step is to extend this peer past NEWKEYS (send NEWKEYS,
SERVICE_REQUEST, USERAUTH) or to record netd TX after `channel 0: send open`; K195 keeps the login
gate.

## Guest fixture

`build.sh` assembles `build/fixtures/ssh-peer/initfs.img` from the server profile's network service
set plus the real `addons/ssh` binaries, with a host-generated Ed25519 host key, `/etc/passwd`,
`/etc/group` and `/etc/shadow`. The service binaries are stripped copies; the source artifacts are
untouched. The initfs is test-only and never staged in the product image.

The `ssh-peer` qtest uses `-netdev user,hostfwd=tcp:127.0.0.1:2222-:22` rather than the K194 tap.
Hostfwd is the existing bounded qtest convention and needs no root; the tap gate remains K195's.
The guest is long-running, so the case expects the harness timeout and `run.sh` judges the peer log.

## Limits

* No authentication, no NEWKEYS reply, no encrypted packets: the tool stops at the KEX boundary.
* The client ephemeral key is random; the peer never derives a shared secret.
* The host control requires a local OpenSSH server on `127.0.0.1:22`.
