<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Scheme CALL/PREAD round-trip cost

Status: **PARTIAL — the benchmark, stage trace and regression are delivered; the one-scheme fixed
cost does not reproduce the 1.5 s/3 MiB path, and the split names K141/K139.** The timing samples
were taken on a loaded host (1-min load 17-40); they are not an idle-host PASS. The idle-host
re-measurement remains open.

## Benchmark

`tests/qtest/cases/scheme-call-cost.toml` boots a custom initfs
(`tests/qtest/cases/scheme-call-cost/`, built by its `build.sh`) with a controller, a registered
`bench` server and a client. The client measures one registered scheme:

- null `CALL` (no captures),
- 4 KiB `PREAD`,
- 64 KiB `PREAD`,
- 1 MiB `read`.

128 samples for the three small operations and 100 for the 1 MiB read; every raw sample is printed
as `scheme-call-cost: raw <op> n=<i> ns=<value>`, followed by
`<op> n=<count> p50_ns=<v> p90_ns=<v>`. The client fails if a p50 exceeds the measured guard
threshold (null 250 µs, 4K 500 µs, 64K 1 ms, 1M 10 ms).

Measured (KVM, `-smp 2`; host load recorded):

| sample | host load (1 min) | null p50/p90 | 4K PREAD p50/p90 | 64K PREAD p50/p90 | 1 MiB read p50/p90 |
|---|---|---|---|---|---|
| before the trace | 40.1 | 82.2 / 136.6 µs | 310.4 / 1030.8 µs | 395.6 / 420.9 µs | 4.35 / 4.64 ms |
| trace off, run A | 26.4 | 28.7 / 29.3 µs | 83.6 / 91.5 µs | 122.0 / 135.4 µs | 1.87 / 1.93 ms |
| trace off, run B | 17.3 | 30.3 / 42.8 µs | 154.4 / 188.4 µs | 161.6 / 201.4 µs | 3.21 / 3.44 ms |

The loaded-host spread is the host, not the protocol: the same image and thresholds pass both runs.
750 four-KiB calls at the 84-154 µs one-scheme cost are 63-115 ms, not the 1.5 s reported for the
3 MiB `libc.so.1` copy. That path adds the vfsd proxy and the loader read.

## Kernel stage trace

`k147-trace` on the kernel command line enables bounded stage markers (first 64 captures /
requests): `capture create` (client copy, slot acquire, server map), `copyback`, `capture drop`
(unmap, TLB flush), `read_at`/`call` (capture + request + submit + wait), `deliver` and `send`.
The code is in `kernel/src/ipc/{capture,router,mod}.rs`; the trace is off unless the flag is
present.

Representative 4 KiB trace from the loaded run (ns):

```text
k147 cap create pages=1 copy=16458 slot=2138 map=5169 total=23765
k147 deliver id=5 total=36755
k147 copyback pages=1 total=4345
k147 cap drop pages=1 unmap=79738 flush=11020 total=92213
k147 send id=5 status=0 result=4096 total=941990
k147 rt op=read len=4096 capture=601432 new=12149 submit=4206 wait=1740290 total=2358077
```

The CPU fixed cost for the one-page capture is ~120 µs: create 24 µs (copy 16, slot 2, map 5),
deliver 37 µs, copyback 4 µs, drop 92 µs (unmap 80, flush 11). The multi-millisecond `wait` and
`send` totals in that run are host-load scheduling/preemption, not kernel copies. The 64 KiB
capture traces at ~51 µs create / ~127 µs drop; the 1 MiB path is copy-bound (create 842 µs /
drop 2.0 ms / copyback 269 µs in the same loaded run).

## Split

The one-scheme fixed cost is ~30 µs for a null CALL and ~84-154 µs for a 4 KiB PREAD. The
closeout 3 MiB copy at ~2 ms per 4 KiB is the **nested vfsd proxy** (4 KiB chunks) and the **loader
read size**, not the base IPC path:

- **K141** (Mel): vfsd large-read fallback / proxy chunk. Raising the chunk removes ~750 nested
  request/response round trips.
- **K139** (Kevin): the loader's 3 MiB read path; the initfs libc copy mapping.
- The remaining ~92 µs capture drop (unmap + TLB flush) and ~24 µs create are the largest
  in-Owns fixed costs; a per-server cached small-capture mapping is the proposed follow-up.

No K141/K139 path is edited here.

## Regression and validation

The qtest is the regression: it asserts the p50 guard thresholds above (a return to a 2 ms/4K
path fails) and checks `du`-independent scheme data integrity (first/last pattern bytes). The
command is `timeout 120 cargo xtask qtest --case scheme-call-cost --arch x86_64`; it passes.

Validated in this session: `ipc-donation` pass, `smp-user` pass, `base-tools` pass (warm-image
split: the first 90 s cap expired during the image build/boot on the loaded host; the case's
measured 48 s guest then passed with `--no-build` in 75 s), `make check-fast` PASSED, provenance
0 violations.

## Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| scheme IPC fixed cost | Door call/return is a scheduler-mediated round trip; behaviour only, no code read | BSD pipe/socketpair calls copy between kernel buffers and wake the peer; behaviour only, no code copied | Linux `io_uring`/futex wakes and splice avoid repeated copies; idea only | Measured the base round trip and the capture map/copy/unmap; no donor code copied or translated. The split is a Raven scheduler/vfsd outcome. |

## Provenance

First-party BSD-3-Clause work. No donor code was read or copied for the instrumentation or the
benchmark. The fixture reuses the existing first-party `raven-s11-echo` helper crate; the scheme
packet definitions are the kernel's own `kernel/src/ipc/protocol.rs`.
