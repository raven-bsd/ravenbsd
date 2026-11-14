# tests/libc/target

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

Host-side drivers for the C73 on-target libc cases.

* `cases.tsv` is the single table: `case<TAB>hard PGs<TAB>soft PGs<TAB>binaries<TAB>link`.
  `build.sh` and `pg-probe.sh` read it; `tests/cases-consistency.sh` compares it with the
  seven `tests/qtest/cases/libc-*.toml` files (the `skip` text must list exactly the HARD
  column).
* `pg-probe.sh [<case>|all]` runs the mechanical probes of C73 4.2. It is read-only and always
  exits 0. `present` is a `grep` match in the product tree, `absent` is no match, `unknown`
  means a human must read the merged ticket named in `tickets/INDEX.md`. A case prints
  `RUNNABLE` only when every HARD item is `present`; `unknown` blocks. `RL_PROBE_ROOT` and
  `RL_PROBE_TSV` point the probe at a fixture tree in the host tests.
* `build.sh <case> [<arch>]` builds one C program into `build/libc-test/`. It exits 77 with a
  `skipped:` line when a HARD PG, a tool, or the sysroot is absent, so `raven-qtest` records
  the case as skipped. `RL_FORCE=1` builds a blocked case anyway; it never removes a `skip`.
  `RAVENLIBC_SYSROOT` overrides `build/<arch>/sysroot`. Only `x86_64` is in this ticket;
  `aarch64` and `riscv64` exit 77.
* `shape-check.sh <elf>` is the interim PG-53 check: `PT_LOAD` align 0x1000, at most 16 loads,
  no writable+executable segment, and matching file/virtual offsets modulo 4096. `READELF`
  selects the reader.
* `tests/*.sh` are host tests and run without a sysroot or QEMU:
  `sh tests/libc/target/tests/pg-probe-test.sh`,
  `sh tests/libc/target/tests/build-dryrun.sh`,
  `sh tests/libc/target/tests/shape-check-test.sh`,
  `sh tests/libc/target/tests/cases-consistency.sh`.

`build/libc-test/` is output and ignored by Git. The five `generated = true` files listed in
`profiles/libc-test.d/10-libc-test.toml` are always written by `build.sh`: the program, the
child, the plug, and zero-byte placeholders for the shared library and loader when the case is
static.
