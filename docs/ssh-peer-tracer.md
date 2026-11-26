<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Fake SSH peer tracer (K375)

**Test tooling only.** `tests/fixtures/ssh-peer/peer.c` is a first-party host-side C program that
pretends to be an SSH client at the version-exchange and key-exchange level. It is built with the
host `cc`, never linked into the product, never staged in the image, and changes no OpenSSH, netd,
TCP or libc code. It exists so an SSH stall can be described byte-exactly instead of from the
OpenSSH client's timeout message.

The version exchange (RFC 4253 section 4.2) and the binary packet format (RFC 4251 section 6) are
implemented from the specifications; no donor code is used.

## Build

```sh
timeout 60 cc -O2 -Wall -Wextra -o /tmp/opencode/ssh-peer tests/fixtures/ssh-peer/peer.c
```

## Modes and flags

```text
--host H --port P          connect target (tap guest or 127.0.0.1 hostfwd)
--mode client              the only mode (default)
--send-banner              send "SSH-2.0-FakePeer_1.0\r\n"
--send-kexinit             send a syntactically valid, unencrypted KEXINIT
                           (curve25519-sha256, ssh-ed25519, aes128-ctr, hmac-sha2-256)
--send-kex-ecdh-init       after the server's KEXINIT, send KEX_ECDH_INIT with the
                           X25519 base point (u=9) so the server answers KEX_ECDH_REPLY
--idle-ms N                stop after N ms with no bytes (default 3000)
--log FILE                 write the trace to FILE (also mirrored to stdout)
```

Exit codes: `0` connected and the session ran to EOF/idle; `2` usage or setup error. The tool does
not judge the peer under test — the log is the evidence.

## Log format

Every read and write is timestamped with `CLOCK_MONOTONIC` in milliseconds (relative to connect),
shown as hex and printable text and flushed immediately, so a zero-byte stall stays visible. The
stream is also parsed:

```text
000010 ms version line: `SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.18`
000015 ms record #1 total=1112 packet_length=1108 padding=10 payload=1097 type=20 (SSH_MSG_KEXINIT)
000024 ms record #2 total=192 packet_length=188 padding=8 payload=179 type=31 (SSH_MSG_KEX_ECDH_REPLY)
000024 ms record #3 total=16 packet_length=12 padding=10 payload=1 type=21 (SSH_MSG_NEWKEYS)
005028 ms idle 5000 ms, no bytes; total_read=1362 total_records=3
```

The record line carries `uint32 packet_length`, `padding_length`, payload length and the message
type, so a malformed or truncated record is visible. A read that is not yet a full record is kept
and parsed on the next read.

## What it answers

1. **Banner**: whether the peer sends `SSH-2.0-...` after accept, and after how many ms.
2. **KEXINIT**: whether the peer sends a KEXINIT record, its exact length, and whether the record
   parses (`packet_length`, `padding_length`, payload, padding).
3. **KEX answer**: after the fake peer sends its banner and KEXINIT (and KEX_ECDH_INIT), whether
   the peer answers with `SSH2_MSG_KEX_ECDH_REPLY`, or stalls/closes and at which byte.
4. **Close**: whether the socket is closed, and the last bytes before the close.

After `SSH_MSG_NEWKEYS` the stream is encrypted; the tracer does not implement ciphers/MACs, so it
reports a record framing error there and keeps the raw bytes. That is the end of the plaintext
trace, not a failure.

## Linux control (proven first, no QEMU)

Against the local host sshd on `127.0.0.1:22`:

```sh
timeout 30 /tmp/opencode/ssh-peer --host 127.0.0.1 --port 22 --mode client \
    --idle-ms 3000 --log /tmp/opencode/ssh-peer-linux.log
timeout 30 /tmp/opencode/ssh-peer --host 127.0.0.1 --port 22 --mode client \
    --send-banner --send-kexinit --idle-ms 5000 --log /tmp/opencode/ssh-peer-linux-kex.log
```

Observed: the real `SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.18` banner at 21 ms, and the real
KEXINIT record (`total=1112 packet_length=1108 padding=10 payload=1097 type=20`) at 21 ms. With
`--send-kex-ecdh-init` the host sshd answers `type=31 (SSH_MSG_KEX_ECDH_REPLY)` (192 bytes) and
`type=21 (SSH_MSG_NEWKEYS)` (16 bytes).

## Guest run

Prerequisite: a release image with the K195 sshd staging (built on the K195 branch).

```sh
cargo xtask image --arch x86_64 --profile release --output build/qtest/ssh-login.img
timeout 600 sh tests/fixtures/ssh-peer/run.sh
```

`run.sh` brings up the K194 tap/bridge, boots the image on the tap, waits for `10.0.2.15:22`, and
runs the tracer three times (banner only, banner+KEXINIT, banner+KEXINIT+KEX_ECDH_INIT). Logs go
to `build/fixtures/ssh-peer/guest-{banner,kexinit,ecdh}.log` plus `serial.log` and `qemu.log`. It
is bounded, repeatable and refuses cleanly without passwordless sudo.

## Scope

K375 is the instrument for the K195/K382 SSH gate; the login gate itself stays with K195. No
product fix belongs in this ticket.
