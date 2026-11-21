<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Typed kernel statistics (KSTAT1)

Status: **BLOCKED — the kernel read path and userland staging are outside KSTAT1's Owns.** The
schema and acceptance case are prepared; no product code is claimed.

## Schema (planned, Illumos contract only)

A kstat is one typed record with a stable name, instance, type, unit and value/aggregate:

```text
module:name[:instance]        e.g. scheduler:cpu0:syscalls
type: char | int32 | uint32 | int64 | uint64 | string
unit: count | bytes | ns | ticks
value: scalar or aggregate (sum/max/min over per-CPU inputs)
```

Names use ASCII `[a-z0-9_.:-]`, at most 31 bytes (`KSTAT_STRLEN` from the Illumos header); the
`module:name[:instance]` form is canonical. The planned first consumer is the existing
`DTRACE_*` stats surface (`CpuStats`: produced/consumed/dropped/capacity per CPU), plus the
scheduler/IPC/VM/console counters the existing `sys:`/`svc:`/`net:` tools already need. A
snapshot is bounded by a caller-provided record buffer; malformed names return a clear error;
aggregation never allocates on the hot path.

## Blocking read path

The ticket owns `kernel/src/kstat/**`, `crates/raven-kstat/**`, `userland/bin/kstat/**` and the
qtest/doc, but a kstat must be registered and read through a kernel path:

* `kernel/src/main.rs` has no `mod kstat;` — a new `kernel/src/kstat` module is not compiled.
* The kernel syscall dispatch is `kernel/src/syscall/mod.rs` (and the existing `dtrace.rs` raw
  `STATS` = `0x94`); wiring the typed snapshot there is outside Owns.
* A scheme-based read (`sys:kstat` or a new `kstat:` server) needs `userland/servers/sysd` or a
  new service plus profile staging; the existing `sysd` cannot read kernel counters.
* The userland `kstat` needs a new workspace member line in the root `Cargo.toml` (the standing
  new-crate exception) and a shell/qtest profile to stage it.

The existing `DTRACE_*` `CpuStats` (`crates/raven-dtrace-user/src/abi.rs`) is a valid first
consumer, but connecting it to a typed kstat still requires the `dtrace.rs` dispatch and/or a
userland aggregator outside the current Owns.

## Donor reconciliation (DR-52)

| Topic | Illumos (CDDL, contract only) | BSD | Linux | Raven decision |
|---|---|---|---|---|
| kstat record and naming | `illumos-gate @ 586678ac`, `usr/src/uts/common/sys/kstat.h:62 KSTAT_STRLEN`, `:68 kstat`, `:75/78/80 module/name/class`, `:89 ks_update`, `:99/106/109 kstat32`, `:451 kstat_named_t`, `:494-535 KSTAT_DATA_*`, `:578 kstat_intr`; the `os/kstat.c` and `cmd/kstat/kstat.c` files are absent from the sparse checkout | No permissive kstat source in the sparse checkout; no code copied | `fs/proc/stat`/perf outcomes only; no code read | Original Rust typed record and stable naming; Illumos header is the contract only, no code copied |

## Acceptance (blocked)

`tests/qtest/cases/kstat.toml` is the bounded case; its pre step fails until the three product
surfaces and the read path exist. The expected raw counters are the existing `DTRACE`/scheduler/
IPC/VM/console counters, not fake values.
