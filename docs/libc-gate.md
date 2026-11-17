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

## Current verdict (2026-11-17, C74i)

**NOT MET.** `cargo run -q -p libc-gate -- verdict` exits 1.

* MET: 1.1, 1.1.pack, 1.1.audit, 1.5, 4.1, 4.3, 6.1, 6.3, 6.4.
* NOT MET: 1.2 (G1 0/4), 1.4 (26 families cannot find `ravenbsd-resources` in the minion
  clone), 1.7 (26 lint violations; the loader files are clean after C74i), 3.1 (size budgets),
  4.2 (`inet_text` soak), 5.1 (hello/stdio/time), 5.2 (core-servers now MET; file-tools still
  fails in uutils `df`).
* BLOCKED: 1.6 (Miri), 2.1/2.2 (`tools/libc-world` and C72 outputs), 6.2 (`mandoc`).

The exact evidence and the seven-case matrix are in `libc/ravenlibc/GATE-REPORT.md` and under
`build/libc-gate/`. C74h re-ran the full gate on `beedfbbd`: the symbol audit is now MET (66/66,
exit 0); the host-test row now runs through `cargo nextest run`; `libc-dynamic`, `libc-signals`,
`libc-spawn` and `libc-threads` pass; `libc-hello` fails only the P1b `string` check (auxv ok);
`libc-stdio` and `libc-time` remain P1b. C74i then added the three missing loader SAFETY
comments: `libc-lint` now reports 26 violations, all outside the loader files (P1f/phil 14,
P1h/otto 8, P1i/tim 3, ABI/contract 1). C74n closed 5.2.core-servers with a minimal profile and
a 300 s case limit; 5.2.file-tools now stages and starts `tooltest`, which fails at `df` because
the uutils `read_fs_list` returns an empty list on RavenBSD (owner: uutils/P1f), not libc. The
gate cannot close until the size-budget lane, the G1 donor data, the `inet_text` soak, the three
P1b target cases, the file-tools/uutils owner, the lint backlog,
and the missing Miri/world/mandoc tooling are resolved by their owners.
