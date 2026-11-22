<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, rev 26. Base `origin/master` `b63275882` (K458); `067faa71` does not
exist. Merge `904f98e16`; code `64d3eee24`. Boots are `-snapshot`; main matrix in the private
`ken434` netns. S1-repeat and S8 use dedicated boots. The gate is red.

Run: `timeout 900 sh tests/ssh/matrix.sh` (exit 1, 9:12) and `timeout 900 make check-fast` (exit 0;
`check-fast: PASSED`).

## Matrix (K458, private netns)

| Row | Result | Exact assertion |
|---|---|---|
| S0 | PASS | `K434-S0-ECHO-0123456789abcdef\n` echoed 30/30, 386 ms |
| S1 | PASS | `SSH-2.0-OpenSSH_10.6\r\n` attempt 1 (2506 ms), control 3532 ms, client-first 2728 ms |
| S1-repeat | PASS | 10/10 banner + auth; walls 8014-8029 ms; exits -9 |
| S2 | PASS | attempt 2 exact (`total=960 ... type=20`); attempt 1 0 records |
| S3 | PASS | wrong exit 255 + `Permission denied`; `Authenticated to ...`; server `Accepted password for raven ...` |
| S4 | BLOCKED | sampled session auth exit 255; post-auth read not exercised |
| S5 | PASS | confirmation delivery + identity qtest |
| S5-session | FAIL | no EPIPE; exit -9 at 15 s; fd4 `pipe:[275640307]`; TCP `10.0.2.2:46018 -> 10.0.2.15:22` |
| S6 | PASS | type 91 + open confirm |
| S7 | FAIL | 8/10 exact; attempts 9-10 exit 255/0 bytes; byte-correct attempts exit -9 at ~30 s |
| S7-routing | PASS | client exact; `console_total_delta=0` |
| S8-pty-alloc | PASS | `pty-alloc: pass` |
| S8-termios | PASS | K444 `termios: pass` (22.9 s) |
| S8 | FAIL | `PTY allocation request failed on channel 0`; server `openpty: Broken pipe` |

## S8 exact call chain

OpenSSH `sshpty.c:63` calls `openpty(ptyfd, ttyfd, NULL, NULL, NULL)`; it returns -1 and
`sshpty.c:65` logs `openpty: Broken pipe`. The libc path is
`libc/ravenlibc/crates/raven-libc-compat/src/pty/openpty.rs:53 posix_openpt`,
`:57 grantpt`, `:58 unlockpt`, `:59 ptsname_r`, `:60 open(slave)`. The log does not identify which
subcall returned EPIPE; per-call instrumentation would be required. Server log:

```text
Accepted password for raven from UNKNOWN port 65535 ssh2
openpty: Broken pipe
session_pty_req: session 0 alloc failed
```

`pty-alloc` and `termios` pass. K458 (pty unit allocation) did not change this.

## S1 A/B (same image, N=10 each)

- **Shared rev-24 setup** (`qemu-net.sh`, 10.0.2.0/24): 5/10 ok; attempts 1-5 `step=banner`
  fail, #4 `Connection timed out during banner exchange`; 6-10 ok.
- **Private netns rev-25 setup**: 4/10 ok; #1-3 ok, #4-5 `step=auth`, #6 ok, #7-10
  `step=banner` (two timeout witnesses).

The netns is not the cause; both setups fail at banner/auth under repeated connections on the same
image.

## K443

`origin/tim/K443` = `1d8cba580`; it does **not** contain master (`b63275882`), so the conditional
K443 S7 test was not run.

## S7 per attempt

```text
#1-#8 exact, exit -9, walls 30019-30050 ms
#9  0 bytes, exit 255, 8088 ms
#10 0 bytes, exit 255, 8052 ms
```

## Artifacts (SHA-256)

```text
ed45cd7579656d8e7ee61d27e6a3aee7c2789b51a029fde6b18ea6b2bc69004c  build/ssh/matrix/matrix.tsv
d453977115b6dc5bc798fefc85e7caa55ac32d39947fab1645e03a3df25d569d  build/ssh/matrix/attempts.tsv
3081478fe78fb2664bf775a8bea8f9eb26a3438824430715e407ae91c1fdd8ed  build/ssh/matrix/serial.log
39ccb628321c3ab2558e6c6891e1f631dac2950ccda6ec408ebe971af303a9da  build/ssh/matrix/serial-repeat.log
6f980dd42e0d1a5b89deff140538ffd92ec8a09f939a487360d1cccf4e168ff8  build/ssh/matrix/serial-s8.log
0d423e04d6a32506a7ac6be35e1a03fa043b26c5bfa8cc63e676724794e6a993  build/ssh/matrix/serial-echo.log
689aa8987a1602cfb1ac7e540ca2c6ff02815f934e2f241e771927109d873da7  build/ssh/matrix/s7-marker.log
c94d3f3cef5e3d5435c157b7423d854438e8b5e45c094a269ff3a4034af73aa2  build/ssh/matrix/s8-tt.log
070a2c4b7acfeabb6d986d29f7bf65de0fd4a0362e6cfa4a3a90e430cc1c70b4  build/ssh/matrix/pty-alloc.log
210a1c71c29dd571e5cb85cb462813d5c7cbe06382b3183d152dd7690d398101  build/ssh/matrix/termios.log
c79222621ac89037d694d4668b34403a57d1d6fb358b6cb38d8edd49f86fe2b3  build/ssh/matrix/s5-vvv.log-probe.log
df669bc70f9d9110adca73dbdf721640f8d7e8d9700a84c498dd7c1f97a2422d  build/ssh/matrix/s5-vvv.log-strace.log
a1e776d3be284f3dd42735f49228c2057c4461529702a55b1c259ed8fe326d69  build/ssh/matrix/socket-write-identity.log
2e68a354c0d35696700d7a595963093d856ce33c4134910fc65371f9df26a889  build/ssh/matrix/s8-server.log
bc36f27947cb5ec1361222288af03874ec4f6e97b9badd500aa63f416579524d  build/ssh/ab/shared/attempts.tsv
58271c6d73a925648787265fb302277f684e1b4e9d35f1f8f6d3227ea544edbd  build/ssh/ab/netns/attempts.tsv
```

## Open gaps

- S4: sampled auth exit 255.
- S5-session: no EPIPE; exit -9 at 15 s.
- S7: 8/10 exact; K443 not testable (branch lacks master).
- S8: `openpty: Broken pipe`; the exact libc subcall is not logged.
- S1/banner instability is present in both network setups (A/B).
