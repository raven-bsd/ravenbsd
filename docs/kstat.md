<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Typed kernel statistics (KSTAT1)

Status: **DONE** — OBSERVED by `cargo nextest run -p raven-kstat`, the `kstat` and
`trace-context` qtests, and the measured 900 s `make check-fast` run. The typed record, stable
naming, bounded snapshot and human/NDJSON tool are implemented. The first consumer is the existing
DTrace per-CPU ring statistics read through the bounded `DTRACE_STATS` syscall (0x94).

Effective Owns for this ticket: the ticket's `kernel/src/kstat/**`, `crates/raven-kstat/**`,
`userland/bin/kstat/**`, `tests/qtest/cases/kstat*` and `docs/kstat.md`, plus
`docs/man1/kstat.1` (added for the docs gate). The scheduler/IPC/VM/console console-consumer
provider is a **named follow-up**, not an edit in KSTAT1: see the follow-up section below.

## Schema

A kstat is one typed record with a stable `module:instance:name` namespace and a value/unit:

```text
module:instance:name          e.g. dtrace:cpu0:produced
module/instance/name          ASCII [a-z0-9_.-], 1..=30 bytes
type                          char | int32 | uint32 | int64 | uint64 | string
unit                          count | bytes | ns | ticks
value                         scalar or checked aggregate
```

The 30-byte component limit is the Illumos `KSTAT_STRLEN` contract (30 significant bytes plus the
NUL terminator). A malformed name returns a stated error (`kstat name must be
module:instance:name`, invalid character, or component too long); it is never silently accepted.
Snapshots are fixed-capacity arrays: pushing past the caller's buffer returns `kstat snapshot is
full`, and global aggregates use checked addition so an overflow is an error, never a wrap.

## Read path

`crates/raven-kstat` is the shared model plus the DTrace provider. `kernel/src/kstat/mod.rs` holds
the model source; the crate includes it with `#[path]` so the same naming/type/overflow rules run
on the host and in the target tool. The provider reads `CpuStats` with the existing
`DTRACE_STATS` syscall, which already bounds the copy by the caller's `max` and returns per-CPU
records. It builds:

* `dtrace:cpuN:produced`, `consumed`, `dropped`, `capacity` for each online CPU;
* `dtrace:global:produced`, `consumed`, `dropped`, `capacity` (checked sums);
* `dtrace:global:cpus` (the represented CPU count).

The model itself does not allocate. `userland/bin/kstat` prints one line per record (human) or one
JSON object per line (`-n`/`--ndjson`) and accepts one exact name filter. The tool also measures
the read in nanoseconds and prints `kstat: read_ns=...`.

## First consumer and accuracy

The first consumer is the existing DTrace ring counter set, not a duplicate counter path. The
target tool enables the syscall-entry probe, records the baseline, issues a known 64
`CLOCK_GET` calls, and checks that the per-CPU `produced` delta is at least 64. It then reads the
typed snapshot while a worker thread keeps making syscalls, proving the read survives load. The
qtest asserts the raw counts, the accuracy delta, the malformed-name rejection, the 4096-record
ring capacity, the NDJSON line and `kstat: ok`; no number is fabricated.

## Donor reconciliation (DR-52)

| Topic | Illumos (CDDL, contract only) | BSD | Linux | Raven decision |
|---|---|---|---|---|
| record, naming, types | `illumos-gate @ 586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, `usr/src/uts/common/sys/kstat.h`: `KSTAT_STRLEN` 62, `struct kstat` 68-100 (`ks_module`/`ks_instance`/`ks_name`/`ks_type`/`ks_data`/`ks_ndata`/`ks_snaptime`), `kstat_named_t` 451, `KSTAT_DATA_CHAR/INT32/UINT32/INT64/UINT64/STRING` 494-535, `kstat_intr_t` 578; `usr/src/man/man3kstat/kstat.3kstat` (namespace and snapshot model), `usr/src/man/man9f/kstat_create.9f` (instance/class/type/count), `usr/src/man/man9f/kstat_named_init.9f` (named type/value) | no permissive kstat framework; FreeBSD `sys/sys/stats.h` (BSD-2-Clause) `enum vsd_dtype`/`voistatdata_numeric` supplies the sum/max/min aggregate vocabulary; implementation `sys/kern/subr_stats.c` absent from the sparse checkout | no `fs/proc/stat.c`, `kernel/sched/stats.c`, `include/linux/percpu_counter.h` or `lib/percpu_counter.c` in the sparse checkout; outcome only | original Rust model from the Illumos contract; no CDDL code copied |
| per-CPU counters / bounded read | `usr/src/uts/common/io/kstat.c` (CDDL): `read_kstat_data` 62, `write_kstat_data` 403, `kstat_ioctl` 664, `kstat_attach` 706 — the `/dev/kstat` driver copies a user kstat header/data with bounded `copyin`/`copyout` | FreeBSD `sys/sys/counter.h` (BSD-2-Clause): `counter_u64_alloc/free/zero/fetch`, `COUNTER_ARRAY_COPY`, `counter_ratecheck`; implementation `sys/kern/subr_counter.c` absent | outcome only | clean-room Rust fixed-capacity snapshot; the existing `DTRACE_STATS` syscall is the bounded read, with a caller-provided `max` and per-CPU records |
| event-counter naming | `kstat.3kstat` namespace rules | NetBSD `sys/sys/evcnt.h` / `sys/kern/subr_evcnt.c` are **4-clause BSD (advertising clause): NO-COPY**; read-only | none | not copied; the stable `module:instance:name` rule comes from the Illumos contract |

No CDDL/GPL code was copied or translated. The FreeBSD headers are BSD-2-Clause; their
implementation files are absent from the sparse checkout, so the Rust implementation is original
against the header contract (recorded divergence). New files are first-party BSD-3-Clause and
carry the SPDX header; no `PROVENANCE.toml` import entry is needed because no donor source was
copied.

## Follow-up (named, not an edit)

The scheduler/IPC/VM/console console-consumer provider is not part of KSTAT1. It needs a kernel
hook and the public counter calls:

* `kernel/src/main.rs` (`mod kstat;`);
* `kernel/src/syscall/mod.rs` (dispatch; or an extension of `kernel/src/syscall/dtrace.rs`);
* `kernel/src/sched/mod.rs` (`switches()`);
* `kernel/src/ipc/capture.rs` (`slot_stats()`);
* `kernel/src/memory/frame.rs` (`stats()`);
* `kernel/src/console.rs` (`dropped()`).

The shared `kernel/src/kstat/mod.rs` model is ready; the follow-up wires these as providers
without changing the already-tested DTrace consumer.

## Running

```sh
timeout 120 cargo nextest run -p raven-kstat
timeout 120 cargo xtask qtest --case kstat --arch x86_64
timeout 90 cargo xtask qtest --case trace-context --arch x86_64
timeout 900 make check-fast
```

The report with the raw qtest counts, the read-cost measurement, the measured 900 s check-fast
result, mutation evidence and the named console-consumer follow-up is
`tickets/tim/reports/KSTAT1.md`.
