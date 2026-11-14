# `raven-atf-run`: donor test-suite runner and G1 conformance gate

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
artifact. The runner is a host tool.

## The decision: vendor `atf-c`, do not reimplement it

The NetBSD and FreeBSD bodies use the full ATF macro set (`ATF_CHECK_ERRNO`,
`ATF_REQUIRE_MSG`, `atf_tc_set_md_var`, `expect_fail/exit/signal/death/timeout`, `atf_utils_*`).
A compatible header of ours would have to reproduce `tc.c`'s result-file state machine and would
silently change verdicts when it drifted. The donor's own `atf-c` is BSD-2, pure C and 17 compiled
sources; it is vendored unchanged under `tests/libc-oracle/vendor/atf` and linked into every ATF
program. A verdict difference is therefore a libc difference, not a shim difference.

`tests/libc/atf/config/` contains our hand-written `bconfig.h` equivalents and the three
`ATF_DEFS_*` macros. NetBSD's generated `bconfig.h` is never used. `shim/rl_shim.h` is macros
only: a missing host function must fail the oracle build, which is the correct classification.

`atf-needs.md` lists the libc surface `atf-c` itself uses.

## Layout

```
src/atf_proto.rs   parse `program -l`
src/result.rs      parse the ATF result file and map it to a verdict
src/runner.rs      fresh work directory, cleared environment, process-group timeout
src/gen_make.rs    the exact BSD make subset (no make is executed)
src/gen_meson.rs   mlibc `all_test_cases`, picolibc file names, pjdfstest TAP scripts
src/build.rs       oracle and target build, compiler-error classification
src/suite.rs       suite files, overrides, orchestrator
src/table.rs       TSV baseline and `CONFORMANCE.md`
src/check.rs       regression / vanished / unlisted
src/gate.rs        G1 criteria read from `tests/libc-oracle/g1.toml`
src/owners.rs      program path -> owning ticket
src/exclude.rs     manual and oracle exclusions
src/report.rs      human, TAP, JSON, JUnit
tests/fixtures/    shell ATF programs and TSV fixtures
```

## Commands

```sh
cargo run -q -p raven-atf-run -- list tests/libc/atf/tests/fixtures/tp-mixed.sh
cargo run -q -p raven-atf-run -- run  tests/libc/atf/tests/fixtures/tp-pass.sh --report tap
cargo run -q -p raven-atf-run -- gen-suite --all
cargo run -q -p raven-atf-run -- suite --suite netbsd-libc --mode oracle --jobs 8
cargo run -q -p raven-atf-run -- suite --suite netbsd-libc --mode target \
    --sysroot build/x86_64/sysroot --run-backend none
cargo run -q -p raven-atf-run -- table --write
cargo run -q -p raven-atf-run -- check
cargo run -q -p raven-atf-run -- gate --g1
```

Exit codes: `0` ok, `1` a check or a test failed, `2` usage/I/O, `3` gate not met, `77` a
required host tool is missing.

## Run backends

* `none` (default): build-only. A program that builds is `not-run: no-run-backend` for every
  body; a program that does not build is `fail` with the classified reason. This is the only
  backend C71 can prove.
* `host-linux`: refused with exit 2 while `raven-libc-sys` has no Linux backend (O-B5).
* `qemu`: an interface only; exit 2, the on-target ATF run is C73's later ticket.

The target build consumes only the sysroot C60/C37/C61 install. A missing file makes every body
`not-run` with `sysroot-incomplete:<path>`; a target run is never faked.

## ATF execution details

`atf-c` requires the test program itself to exist in `-s <srcdir>`, while donor tests also read
data files from the vendored source directory. For every case the runner therefore builds a
shadow source directory of symlinks: one link per data entry plus a link to the built program,
and passes that as `-s`. The vendored tree is never written.

Each case runs in a fresh directory with `PATH` (only), `HOME`, `TZ=UTC`, `LANG=C.UTF-8`,
`LC_ALL=C.UTF-8` and `TMPDIR`. The child gets its own process group; on timeout the group is
killed with `kill -KILL -- -<pgid>`, so forked grandchildren die too.

A program that exits 0 without a result file is a **fail**, never a pass. `expected_failure` is a
pass with reason `xfail:<why>`; `expected_exit`/`expected_signal`/`expected_death`/
`expected_timeout` are judged against the real process status.

## Table and regression rule

`table` merges `build/libc-conformance/oracle/<suite>.tsv` with the target TSV. An oracle failure
is `oracle-fail` (auto-excluded, ticket `-`); an oracle pass takes the target verdict. A pass rate
uses `eligible = bodies - excluded - oracle-fail - skips for missing root/program/file`.

`check` exits 1 on (1) a baseline `pass` that is no longer a pass, (2) a baseline row with no
result and no `removed` marker, (3) a result body absent from the baseline. Improvements are
reported and exit 0; `table --write` refuses to write regressions without
`--accept-regressions "reason"`.

## Known limits (honest)

* glibc 2.35 lacks `strlcpy`, `getprogname`, `strtoi`, NetBSD headers (`sys/atomic.h`,
  `vis.h`, `h_macros.h`, ...): those programs are `oracle-fail`, not libc bugs, and are listed.
* The vendored trees are those the C35 tool accepted. FreeBSD `libthr` and OpenBSD
  `libm`/`libpthread` were absent from the donor checkouts; mlibc has no per-file licence text
  in its first 80 lines, so the tool refused every file (O-B6); pjdfstest is shell-only.
* The target half is `not-run`: `build/x86_64/sysroot` is not installed by C60/C37/C61.
