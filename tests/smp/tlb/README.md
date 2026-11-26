<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K376 TLB load generator and gate

`scripts/tlb-stress.sh` drives the existing `posix-fs-smp` TCG `-smp 4` case and owns the
per-iteration evidence. `load-generator.sh` is the deterministic pressure source:

```sh
sh tests/smp/tlb/load-generator.sh start 100 build/tlb-stress/load.pids
sh tests/smp/tlb/load-generator.sh stop  build/tlb-stress/load.pids
```

`start` spawns exactly N `yes(1)` workers (one fixed work unit each) and records their PIDs;
`stop` kills exactly those workers. The harness always stops them on exit, including INT/TERM.
The measured `/proc/loadavg` is the evidence; the requested worker count is only the pressure
class.

## Gate

```sh
# Default load-matched run: 20 iterations, TCG -smp 4, 100 workers plus S113 10/10.
sh scripts/tlb-stress.sh --load 100 --iterations 20

# Low-load control: 3 iterations with no added workers.
sh scripts/tlb-stress.sh --load 0 --iterations 3
```

`--load 0` refuses before the first sample when host load1 > 20; a refused host is not a sample.
For a long run, split the 20 rows into four recorded five-row batches after the first prebuild:

```sh
for n in 1 2 3 4; do
  sh scripts/tlb-stress.sh --load 100 --iterations 5 \
    --no-prebuild --no-s113 --out "build/tlb-stress/batch-$n"
done
```

Measured on a 20-CPU host: one load-matched row is 295-311 s (the case's own 300 s timeout plus
boot) and a five-row batch is about 1530-1550 s, so a batch uses a 1800 s foreground cap. One
low-load row is 128-139 s at host load1 4.75-7.52. The separate S113 run is 10 rows, worst
132.7 s, average 108.2 s, about 1085 s total and a 2100 s cap. The per-row qtest cap is 420 s
(the case's 300 s timeout plus boot/build margin). No cap is raised to hide a timeout.

## Reading a row

`results.tsv` has one row per iteration (and one `s113` row): iteration, start/end UTC, load
1/5/15 at start and end, qtest exit code, PASS/FAIL, duration and log path. `serial-lines.txt`
holds every serial line matching `tlb`, `timeout`, `stale`, `FAIL` or `pass` for each row.
A nonzero qtest exit is FAIL for that iteration and is never retried; a timed-out row stays in
the log and in `results.tsv`. The `s113` row is the separate S85b/S113 TLB reproducer and is not
a substitute for an SMP gate row.

## Donor note

Harness only; no kernel or network behaviour is taken from a donor. The load-matched gate is
the correct shape because S159 is a load-dependent TLB ack-delay defect: a low-load-only run
does not reproduce it, and a green S113 counter run does not exercise the same pressure.
