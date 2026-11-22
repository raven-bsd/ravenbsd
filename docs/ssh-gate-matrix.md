<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, rev 25. Base `origin/master` `a666aab4` (K440/K441/K444), merge
`7dc52bb2e`; code `e7af0a01f`. Boots are `-snapshot`; the matrix runs in a private `ken434` network
namespace with its own `ravenbr0`/`ravendemo0` and 10.0.2.15 guest, so other lanes cannot steal the
route. S1-repeat and S8 have dedicated boots. The gate is red.

Run: `timeout 900 sh tests/ssh/matrix.sh` (exit 1, 9:44) and `timeout 900 make check-fast`
(exit 0; `check-fast: PASSED`).

## Matrix

| Row | Result | Exact assertion |
|---|---|---|
| S0 | PASS | `K434-S0-ECHO-0123456789abcdef\n` echoed 30/30, 332 ms |
| S1 | **FAIL** | attempt 1 0 bytes; control attempt 2 exact `SSH-2.0-OpenSSH_10.6\r\n` (9350 ms) |
| S1-client-first | PASS | exact banner (4347 ms) |
| S1-repeat | **FAIL** | 9/10 banner; 8/10 auth (`auth_fail=2`); walls 8001-8036 ms; exits -9 |
| S2 | **PASS** | attempt 3 exact `total=960 packet_length=956 padding=9 payload=946 type=20`; attempts 1-2 0 records |
| S3 | **FAIL** | sampled correct auth exit 255; server line present from a later auth (`Accepted password for raven from UNKNOWN port 65535 ssh2`); wrong exit 255 + `Permission denied` |
| S4 | PASS | `hostkeys-00@openssh.com`, no `EFAULT` |
| S5 | PASS | confirmation delivery + identity qtest PASS |
| S5-session | **FAIL** | no EPIPE; exit -9 at 15 s; fd4 `pipe:[275150884]`; TCP `10.0.2.2:53228 -> 10.0.2.15:22` |
| S6 | PASS | type 91 + `channel 0: open confirm rwindow 0 rmax 32768` |
| S7 | **FAIL** | 4/10 exact; six exit 255/0 bytes; byte-correct attempts exit -9 at ~30 s |
| S7-routing | **FAIL** | client 0 bytes on the sampled marker attempt |
| S8-pty-alloc | PASS | `pty-alloc: pass` |
| S8-termios | PASS | K444 `termios: pass` (measured 1:47, cap 150) |
| S8 | **FAIL** | `PTY allocation request failed on channel 0`; server `openpty: Broken pipe` |

## S8 exact server error

The dedicated S8 boot captured `/tmp/sshd.log` over the console:

```text
Accepted password for raven from UNKNOWN port 65535 ssh2
openpty: Broken pipe
session_pty_req: session 0 alloc failed
Connection closed by UNKNOWN port 65535
```

`pty-alloc` and the K444 `termios`/TCSADRAIN qtest both pass. The failure is the OpenSSH monitor
`openpty` returning `Broken pipe`; the test-only `/bin/tty` is not reached.

## S2 bounded retry

```text
#1  0 records
#2  0 records
#3  total=960 packet_length=956 padding=9 payload=946 type=20
```

The first two failures are recorded, not silent; attempt 3 is exact.

## S7 per attempt

```text
#1   exact marker  exit -9  wall 30044 ms
#2   0 bytes       exit 255 wall 8049 ms
#3   0 bytes       exit 255 wall 8062 ms
#4   exact marker  exit -9  wall 30034 ms
#5   0 bytes       exit 255 wall 102 ms
#6   exact marker  exit -9  wall 30014 ms
#7   0 bytes       exit 255 wall 101 ms
#8   exact marker  exit -9  wall 30009 ms
#9   0 bytes       exit 255 wall 151 ms
#10  0 bytes       exit 255 wall 425 ms
```

K443 remains in flight; no gate claim.

## Artifacts (SHA-256)

```text
a0c9a898728339ce09e4b4389f1fbff2c72ab6b2de95c85e4bec26140a2f50bd  build/ssh/matrix/matrix.tsv
73d993144149fbb115d9b878f59795824a26eca0aa86fe101c6f747ab62a27da  build/ssh/matrix/attempts.tsv
eb8c78d32e2e79a29b03d876d77154a1f4d9e7dde004719bc8a5a374906adda2  build/ssh/matrix/serial.log
116cbf710f1ed225c9a9e987f34a05ca321377f103e1df0560d6d7e8c34b200d  build/ssh/matrix/serial-repeat.log
d11b11c9fcfe601bba4c12b4e978adaf1e93325573d9067ef2d093e70f519baf  build/ssh/matrix/serial-s8.log
2b3501b2149c6a10ad4cf37d674627851c2e8ae8be3c979f00e0b38c404484a0  build/ssh/matrix/serial-echo.log
65ddcae8e45c42a5e50df7aa3be868dd3bfa5f1bfa53b73158c1dfba5c147372  build/ssh/matrix/s7-marker.log
ec200e75fcb16ec8ca2a09a30c091e29b1c89424d59c646bbd96599547b62186  build/ssh/matrix/s8-tt.log
44161a693851211eb056230e887ddf36404cc0ca20f2c34330fcada9c744d0da  build/ssh/matrix/pty-alloc.log
f04438b7df2d3d153a4ef2353e08b709662578161fe0dc363cffa41baba5034f  build/ssh/matrix/termios.log
a7c26941039c7ebb5040b390aa1d20a908f218f1e25f62757479fbb626b7f205  build/ssh/matrix/s5-vvv.log-probe.log
7ad36270a8ec18d0404298c309009fdd3ae1c8f533e7322559daeed0e0d31f75  build/ssh/matrix/s5-vvv.log-strace.log
a1e776d3be284f3dd42735f49228c2057c4461529702a55b1c259ed8fe326d69  build/ssh/matrix/socket-write-identity.log
487ac3132abe11658b8c7047a29c1bc71642197afa592002379f1396ceaf7802  build/ssh/matrix/s8-server.log
cd542c651f867ff16f3997b7a4130110c32bafe33e450a6c4a78afa30a1f20b3  matrix run log
```

## Open gaps

- S1: first fresh attempt 0 bytes; control exact.
- S1-repeat: 8/10 auth.
- S2: first two probes 0 records; attempt 3 exact (timing isolated by bounded retry).
- S3: sampled auth exit 255 while a later auth line exists.
- S5-session: no EPIPE; exit -9 at 15 s.
- S7: 4/10 exact; K443 pending.
- S8: `openpty: Broken pipe` in the monitor; termios/pty-alloc pass.
- S6 length: not in the client log.
