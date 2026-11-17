<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc gate

The libc gate is the six-row acceptance gate for ravenlibc: correctness, portability, size,
hardening, on-target cases and documentation. The check definitions live in
`tests/libc/gate/rows.toml`; the evaluator is the `libc-gate` crate. A row is MET only when every
required check in it is MET; BLOCKED checks must be unblocked by their owning ticket, not by
lowering the threshold or editing expected output.

## Running it

```sh
cargo xtask libc-pack --arch x86_64
cargo xtask libc-link --arch x86_64
cargo run -q -p libc-gate -- eval
cargo run -q -p libc-gate -- table
cargo run -q -p libc-gate -- verdict
```

`eval` writes one evidence file per check under `build/libc-gate/`; `table` renders the markdown
table for `libc/ravenlibc/GATE-REPORT.md`; `verdict` exits 0 only when the gate is MET. The base
gates are `cargo xtask libc-lint` and `make check-fast`.

`libc-pack` stages `build/<arch>/sysroot` (crt objects, `libc.a`, stub archives, headers, and the
dynamic `libc.so.1`/`ld-elf.so.1` once `libc-link` has built them), so the C target cases compile
against a complete sysroot without a manual staging step.

The seven on-target cases are:

```sh
cargo xtask qtest --case libc-hello --arch x86_64
cargo xtask qtest --case libc-stdio --arch x86_64
cargo xtask qtest --case libc-time --arch x86_64
cargo xtask qtest --case libc-threads --arch x86_64
cargo xtask qtest --case libc-signals --arch x86_64
cargo xtask qtest --case libc-dynamic --arch x86_64
cargo xtask qtest --case libc-spawn --arch x86_64
```

The donor G1 suite is `cargo run -q -p raven-atf-run -- gate --g1`.

## Current verdict (2026-11-17, C74c)

**NOT MET.** `cargo run -q -p libc-gate -- verdict` exits 1.

* MET: 1.1, 1.1.pack, 1.5, 1.7, 4.1, 4.3, 6.1, 6.3, 6.4.
* NOT MET: 1.1.audit (stale `expected.tsv`), 1.2 (G1 0/4), 1.4 (four `compat-*` oracle
  features), 3.1 (size budgets), 4.2 (`inet_text` soak), 5.1 (hello/stdio/time/threads/dynamic),
  5.2 (core-servers and file-tools timeouts).
* BLOCKED: 1.6 (Miri), 2.1/2.2 (`tools/libc-world` and C72 outputs), 6.2 (`mandoc`).

The exact evidence and the seven-case matrix are in `libc/ravenlibc/GATE-REPORT.md` and under
`build/libc-gate/`. `libc-signals` and `libc-spawn` pass. C74c makes `libc-hello`'s auxv check
pass (only the P1b `string` check remains), and `libc-threads` passes at runtime when the C74b
`libc-pack` wrapper headers (`stdatomic.h` and the other `#include_next` files) are removed; that
header staging is a `libc_pack.rs` defect outside C74c's Owns. `libc-dynamic` fails the C test's
interim `shape-check.sh` (`partial file tail at 0x0`), which is in the C test build script, not
`libc_link`. `libc-time` needs P1b `sleep`/`usleep`/`tzname`; `libc-stdio` faults at `0x3` (P1b).
