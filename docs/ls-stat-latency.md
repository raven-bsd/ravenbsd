<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `ls -l`/`stat` latency on KVM 4 GiB (K163)

Status: **OBSERVED at `c1d45abc`.** KVM 4 GiB, 4 vCPU, shell-test image. `ls -lah /bin` is
0.147 s after the output-buffer fix; the remaining budget is the process spawn/shell round-trip
and the per-entry Scheme stat. The target <0.100 s needs K165 (spawn) and a vfsd/initfsd stat
handoff; this document records the exact numbers.

## 1. Budget

Wall clock after the guest prompt (`echo` is the shell round-trip baseline):

| Command | Before buffer | After buffer |
|---|---:|---:|
| `echo k163` (shell builtin) | 0.052 | 0.051 |
| `/bin/true` (spawn/loader) | 0.085 | 0.075 |
| `ls /` | 0.108 | 0.082 |
| `ls -1 /bin` | 0.112 | 0.083 |
| `ls -l /bin` | 0.217 | 0.142 |
| `ls -lah /bin` (console) | 0.226 | 0.147 |
| `ls -lah /bin > /tmp/ls.out` | 0.137 | 0.222 |
| `stat /bin/ls` | 0.076 | 0.095 |

The pre-buffer console-versus-file difference was 0.089 s; it is the per-line `Stdout` flush.
The post-buffer console listing is one `RawWriter` write, so that cost is gone from the `ls`
path (the redirect run was slower only because the host was loaded during that sample).

uutils-internal timings from the instrumented build (redirected, so no console cost):

```text
ls -lah /bin: list=56 ms, entry=54 ms, meta=27 ms over 31 calls, output=0 ms
ls -1 /bin:   list=3 ms,  entry=1 ms,  meta=0 ms over 1 call,  output=0 ms
ls -l /bin:   list=53 ms, entry=52 ms, meta=27 ms over 29 calls, output=0 ms
```

`ls -1` is 3 ms inside the process; `-l` adds ~27 ms of per-entry metadata and ~25 ms of
readdir/format work. The `/bin/true` baseline (0.075 s) is larger than the whole `ls -1` process
work, so **spawn/exec + shell round-trip dominates the remaining wall time**.

## 2. Fix in this ticket

`userland/uutils/src/uu/ls/`: `ListState.out` collects the listing in an `OutputBuffer`; at
flush it is written once through `uucore::io::RawWriter(stdout())` on RavenBSD (plain `stdout()`
on other targets). This removes the per-line `Stdout` flushes and does not cache file data.
`display.rs`/`dired.rs` take `&mut OutputBuffer`. Output text and `--json` are unchanged.

## 3. Handoffs

- **Spawn/exec → K165/Kevin.** `/bin/true` wall 0.075 s; `echo` shell round-trip 0.051 s;
  `ls -1` wall 0.083 s with only 3 ms inside `ls`. Getting `ls -lah` under 0.100 s requires the
  spawn path, not another `ls` change.
- **Per-entry Scheme stat → vfsd/initfsd owner.** `-l` adds ~27 ms for 22 entries (~1.2 ms per
  `stat`), measured through `PathData::metadata` after `readdir`. This is the second budget item
  if spawn is fixed.
- **Console → K142/Phil** is no longer on the `ls` hot path after the one-write buffer; other
  line-buffered writers still have the per-line cost.

## 4. Regression status

No `ls-stat-latency` qtest was added: the target is not green (0.147 s), and a TCG 4 GiB case is
also host-load sensitive. `base-tools-ls-latency` (TCG) failed its 2 s step in a loaded run with
`ls /bin` never reaching the prompt; the KVM numbers above are the valid sample. Add the bounded
case when K165 and the stat handoff bring the KVM 4 GiB run under 0.100 s.

## 5. Donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| `ls -l` stat | `stat`/`fstat` outcomes only. | FreeBSD/NetBSD `ls -l`/`stat` use inode attributes; permissive, not copied. | GNU `ls -l` outcome only. | No algorithm change; buffer the output. |
| Output buffering | Not applicable. | POSIX stdio line buffering is the reference; no code copied. | `stdout` line buffering outcome only. | First-party `OutputBuffer` in the vendored MIT uutils. |
| Per-entry stat cost | Not applicable. | Not applicable. | Not applicable. | Handoff to vfsd/initfsd owner with the measured 27 ms/31 calls. |

## 6. PG-31 one-call lstat (later rework)

The 27 ms metadata budget was not 29–31 initfsd `Lstat` calls: libc `stat_at` was the interim
`open + FSTAT + close` sequence, three Scheme calls per entry. PG-31 now uses one `Call::Lstat`:

- `userland/servers/initfsd`: implements `Call::Lstat { path }` with the model stat.
- `libc/ravenlibc/crates/raven-libc-platform/src/platform.rs`: `stat_at` sends one `Call::Lstat`
  on the cached `file:/` root when `AT_SYMLINK_NOFOLLOW` is set. Follow (`stat`) keeps the
  open+FSTAT+close fallback so symlink semantics are unchanged; an unsupported/failed Lstat also
  falls back. No cache of file data.
- `ls -l` uses `lstat` for directory children, so its 29–31 metadata calls become one Scheme call
  each.

Measured KVM 4 GiB after PG-31:

```text
ls -l /bin        0.140 -> 0.120 s
ls -lah /bin      0.146 -> 0.126 s
stat /bin/ls      0.075-0.077 s
```

The ~20 ms saving is consistent with 27 ms -> ~7 ms at the metadata stage; the full `ls -lah`
target remains above 100 ms because spawn/shell is 70-77 ms (K168). No `ls-stat-latency` case is
added until K168 and this stage are both green; `base-tools` passes (33.8 s) and `make check-fast`
is green.
