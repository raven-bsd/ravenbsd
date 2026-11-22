<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, rev 24. Base `origin/master` `314a225d` (K442), code `d19f095c6`.
Boots use `-snapshot`; S1-repeat and S8 each run on a dedicated fresh boot. The gate is red: S2,
S5-session, S7's exit/bytes gate and S8.

Run: `timeout 900 env RAVEN_QEMU_BRIDGE=kenbr0 RAVEN_QEMU_TAP=kentap0 sh tests/ssh/matrix.sh`
(exit 1, 8:55) and `timeout 900 make check-fast` (exit 0; `check-fast: PASSED`).

## Matrix

| Row | Result | Exact assertion |
|---|---|---|
| S0 | PASS | `K434-S0-ECHO-0123456789abcdef\n` echoed 30/30, 164 ms |
| S1 | PASS | `SSH-2.0-OpenSSH_10.6\r\n` attempt 1 (3145 ms), control 3912 ms, client-first 4534 ms |
| S1-repeat | PASS | 10/10 fresh connections: banner `...OpenSSH_10.6`, auth `Authenticated ...`; walls 8003-8035 ms; exits -9 after auth |
| S2 | **FAIL** | peer `total_read=0`, 0 records (no KEXINIT record on that connection) |
| S3 | PASS | wrong exit 255 + `Permission denied`; `Authenticated to ...`; server `Accepted password for raven from UNKNOWN port 65535 ssh2` |
| S4 | PASS | `hostkeys-00@openssh.com`, no `EFAULT` |
| S5 | PASS | confirmation delivery + identity qtest PASS |
| S5-session | **FAIL** | no EPIPE; exit -9 at 15 s; fd4 `socket:[273257612]`, `pipe:[273171275]`; TCP peer `ESTAB` |
| S6 | PASS | type 91 + `channel 0: open confirm rwindow 0 rmax 32768` |
| S7 | **FAIL** | 5/10 exact; 5 exit 255/0 bytes; byte-correct attempts exit -9 at ~30 s |
| S7-routing | PASS | client exact on byte-correct attempts; `console_total_delta=0` |
| S8-pty-alloc | PASS | `pty-alloc: pass` (75 s cap, measured 39 s) |
| S8 | **FAIL** | `PTY allocation request failed on channel 0` on a dedicated fresh boot; test-only `/bin/tty` not reached |

## S1-repeat

Dedicated boot, one readiness probe, ten fresh attempts:

```text
10/10 banner + Authenticated
walls_ms=[8017,8032,8022,8033,8035,8021,8029,8003,8015,8015]
exits=[-9,-9,-9,-9,-9,-9,-9,-9,-9,-9]
```

The exit -9 is the post-auth completion failure (S5-session/S7-exit), not banner/auth.

## S7 exit/wall

```text
#1-#5   bytes exact  exit -9  walls 30007-30047 ms
#6     0 bytes      exit 255 wall 2207 ms
#7     0 bytes      exit 255 wall 1053 ms
#8     0 bytes      exit 255 wall 51 ms
#9     0 bytes      exit -9  wall 30043 ms
#10    0 bytes      exit -9  wall 30041 ms
```

S7-routing PASS on the exact bytes; S7 is FAIL because 5/10 are empty and no exit 0.

## S8: K442 merged, PTY still fails

`pty-alloc` passes. S8 runs on its own fresh boot (snapshot, readiness probe) and still returns
`PTY allocation request failed on channel 0`; the staged `/bin/tty` is not reached. K442 is on
master; the SSH PTY request remains red.

## S2

The peer probe got `total_read=0`, 0 records. The same run's S1/S1-repeat and S3 auth passed; the
matrix records the exact zero-record probe, not a label.

## Artifacts (SHA-256)

```text
d22e9cdb9fcea46e1c3a2fc7cb31c39c976448476f15c5e24eef9ead849d0f4d  build/ssh/matrix/matrix.tsv
247bf798f0ce2524228e8452f67f02520f5a7f710e943e0ae41226d40faa2cbd  build/ssh/matrix/attempts.tsv
fd8e9f4c2f9368c099ececa3b556733bcfea2dcb20d71c1f22cf91bf7016e1ab  build/ssh/matrix/serial.log
573de1dabdab965d03d0dcb5c614a36b1c9820ba1a8a8d54a46e47a01e8b19f0  build/ssh/matrix/serial-repeat.log
12ff94a71de96ccbae1381e73f82857f9c412fdac660f94a48d67d2f40397c81  build/ssh/matrix/serial-s8.log
1e62b70a7f7b36f1f4f0ac6b29495d1d18a806531ccab6b5dcf5ea81446e7c61  build/ssh/matrix/serial-echo.log
fcf7971f519210f4a03d31392e1ba770d7425509994933e262e42c2786db41ce  build/ssh/matrix/s7-marker.log
74084c418e64f68b0a598665517394d1b96a0b16bf292fab425c0bebce7234e6  build/ssh/matrix/s8-tt.log
53fe5550859833b30f5b79af9ce99101ca78d4e149aece7794a30ffd8d0eb703  build/ssh/matrix/pty-alloc.log
6bf0c91259120241bff87211652e4da21e6096617f8cada90d96347071e06f2a  build/ssh/matrix/s5-vvv.log-probe.log
c718e550b9e0b3bb017fdc6b280c3b04c3f54cf74e35d8fde74f6b1b572ecff7  build/ssh/matrix/s5-vvv.log-strace.log
a1e776d3be284f3dd42735f49228c2057c4461529702a55b1c259ed8fe326d69  build/ssh/matrix/socket-write-identity.log
b1c97b20ba303e617eb0c175a4c20445de184d94cb3a37fd27693d4524ccb9a0  matrix run log
```

## Open gaps

- S2: 0-record peer probe in this run.
- S7: 5/10 exact; no exit 0; K443 pending.
- S5-session: exit -9 at 15 s; no EPIPE.
- S8: PTY allocation still fails after K442; test-only tty not reached.
- S6 length: not in the client log.
- S3 peer address: `from UNKNOWN port 65535`.
