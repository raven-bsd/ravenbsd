<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K376 TLB load generator

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

Run the low-load control only when the host is actually idle:

```sh
sh scripts/tlb-stress.sh --load 0 --iterations 3 --out build/tlb-stress/control
```

The guard refuses `--load 0` when host load1 > 20 (zero samples). For the full S159 reproduction
use four batches of five:

```sh
for n in 1 2 3 4; do
  sh scripts/tlb-stress.sh --load 100 --iterations 5 \
    --no-prebuild --out "build/tlb-stress/batch-$n"
done
```

`--no-prebuild` reuses the image built by the first batch; every batch is foreground with a
measured 1800 s cap.
