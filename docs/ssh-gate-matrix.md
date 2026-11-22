# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# SSH gate matrix

Status: **RED. The SSH gate is not stable across boots.** K446 ran 5 fresh boots
x 5 fresh connections on the gate image; **0 of 25 attempts** produced the exact
banner and an authenticated, completed session. SSH is the only release priority;
no product change was made by K446 (evidence only).

## Method

* Image: `build/qtest/ssh-login.img`, commit `314a225d`,
  sha256 `6713edd3b505aedf1e3f8fb77898f393a70a221d6c6528d674d910896952adbe`.
* Harness: `tests/ssh/bootloop.sh` + `tests/ssh/bootloop.py` (K446), over the
  K194 tap/bridge; one QEMU per boot, five attempts per boot, no retries.
* Per attempt: a fresh raw TCP connection reads the SSH banner (exact bytes
  `SSH-2.0-OpenSSH_10.6p2\r\n`), then a fresh `ssh -v` password session must
  print `Authenticated ... using "password"` and exit 0.
* Evidence: `tests/evidence/ssh-bootloop/20261122T104637Z/`
  (`attempts.tsv`, per-attempt `.banner`/`.ssh.log`, per-boot `serial.log`,
  `SHA256SUMS`).

## Result (5 boots x 5 attempts)

| Boot | Attempt 1 | 2 | 3 | 4 | 5 | Summary |
|---|---|---|---|---|---|---|
| 1 | 22 B wrong | 22 B wrong | 22 B wrong | 0 B | 0 B | 3 wrong banner, 2 zero-byte |
| 2 | 0 B | 0 B | 22 B wrong | 22 B wrong | 22 B wrong | 2 zero-byte, 3 wrong banner |
| 3 | 22 B wrong | 22 B wrong | 22 B wrong | 22 B wrong | 22 B wrong | 5 wrong banner |
| 4 | 22 B wrong | 22 B wrong | 22 B wrong | 22 B wrong | 22 B wrong | 5 wrong banner |
| 5 | 0 B | 0 B | 0 B | 0 B | 0 B | 5 zero-byte |

Counts over 25 attempts:

* exact banner: **0/25**;
* banner `SSH-2.0-OpenSSH_10.6\r\n` (missing the `p2` portable suffix): **16/25**;
* zero-byte banner (TX wedge): **9/25** — every zero-byte attempt is listed with
  its boot/attempt number in `attempts.tsv`;
* `Authenticated ... using "password"` line: **16/25** (all 16 responsive
  attempts authenticate);
* session/command exit 0: **0/25** — the 16 authenticated sessions hang after
  `Sending command: true` and the client times out; the 9 zero-byte attempts
  never reach auth.

## Findings

1. **Banner version string is wrong.** The server advertises
   `SSH-2.0-OpenSSH_10.6`; the donor `addons/ssh/openssh/version.h` defines
   `SSH_RELEASE = SSH_VERSION SSH_PORTABLE` (`OpenSSH_10.6` + `p2`), and the
   expected gate bytes are `SSH-2.0-OpenSSH_10.6p2\r\n`. Every responsive boot
   (16/16) shows the missing suffix — a deterministic build/version defect, not
   a flake.
2. **Zero-byte TX wedge persists and is boot-correlated**: 9/25 attempts,
   concentrated in boots 2 (first two) and 5 (all five).
3. **Post-auth session hang**: authentication succeeds in the responsive boots,
   but the exec session never returns (no exit code); the gate's
   "command completed" requirement fails for 16/16.

K434's observation (3/10 zero-byte in one boot, 10/10 in another) is confirmed
and extended: the instability is per-boot and the banner string is wrong on
every responsive boot.

## How to rerun

```sh
cargo xtask image --arch x86_64 --profile release --output build/qtest/ssh-login.img
timeout 1800 sh tests/ssh/bootloop.sh
```

Requires root/passwordless sudo for the K194 tap helper. A non-zero exit is the
measured result; do not retry an attempt to hide it.
