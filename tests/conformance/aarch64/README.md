<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# aarch64 conformance runner (S182)

The shared conformance tables under `tests/conformance/<topic>/` are qtest cases. This runner
discovers them, rewrites only the architecture-specific details, and runs each one under QEMU
virt on aarch64 with `cargo xtask qtest --arch aarch64`. The expected regexes, steps, forbids,
timeouts and expected exit values are copied verbatim, so both architectures assert the same
contract.

The runner can be built before every table lands. With no shared tables it is still a valid
readiness check with `--allow-empty`; the bundled selftest proves the aarch64 execution path.

## Table contract

Each table is a qtest case (`tests/qtest/src/case.rs`) under `tests/conformance/<topic>/`:

* no `skip =`: an aarch64 limitation is a divergence record, never a skip;
* no `loader_kernel` (x86-64 test kernels) and no x86-only `qemu_extra` (`q35`, `if=ide`);
* the `profile` (or an `image` already built for aarch64) must produce an aarch64 image;
* `boot_cmdline` may use `console=ttyS0`; the runner rewrites it to `console=ttyAMA0`.

The runner copies each table to `build/conformance/aarch64/<name>-aarch64.toml`, sets
`arch = "aarch64"`, gives it a unique name (so the x86-64 and aarch64 image/log names do not
collide), and leaves every expectation unchanged. A table that cannot meet this contract fails
with a `TableError`; record the divergence in `divergences/<topic>.md` and the ticket that fixes
it.

## Self-contained rows

A `tests/conformance/<topic>/` directory with `run.py`/`build.sh` and no `.toml` is a
self-contained row rather than a shared qtest table. It must provide an aarch64 entry point with
the same assertions, or a `divergences/<topic>.md` record. The runner reports the row as a
divergence and exits 1; it never silently skips it. The first such row is `credentials`
(S179/S187), an x86-64-only runner; its record is `divergences/credentials.md`.

## Usage

Heavy work must run through the host build slot, with a measured outer timeout:

```sh
/home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/scripts/build-slot.sh conformance-aarch64 -- \
  timeout 1800 python3 tests/conformance/aarch64/run.py
```

The runner wraps each table's `cargo xtask qtest` in a `timeout` of the table's `timeout_secs`
plus a 420 s image-build margin. The outer cap must cover the number of tables times that
budget; one table is normally under 5 minutes on a cached profile.

Readiness and execution checks:

```sh
python3 tests/conformance/aarch64/run.py --allow-empty      # 0 tables and 0 self-contained rows
python3 tests/conformance/aarch64/run.py --plan             # translate only, no QEMU
python3 tests/conformance/aarch64/run.py --self-test        # aarch64 virt boot selftest
python3 tests/conformance/aarch64/run.py --table tests/conformance/aarch64/selftest/negative-skip.toml
```

Options: `--include-mutations` also runs `*mutation*.toml` tables; `--table <path>` runs one
file; `--self-test` runs the bundled aarch64 boot row; `--plan` translates and reports without
QEMU; `--allow-empty` accepts the current zero-table state.

Exit codes: `0` all selected tables passed with no self-contained divergence (or the allowed-empty
readiness check); `1` a table failed or declared a skip, or a self-contained row is a divergence;
`2` no tables or self-contained rows without `--allow-empty`, or a usage error.

The selftest is `selftest/boot-uname-aarch64.toml`; `selftest/negative-skip.toml` is the
negative check that proves a declared skip is rejected.
