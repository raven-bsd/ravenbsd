# `tests/libc/regress`: exit-status donor suites

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

This directory documents the non-ATF suites that `raven-atf-run` drives with the exit-status and
TAP frameworks. The suite descriptions live in `tests/libc-oracle/suites/<suite>.toml`; the
vendored C lives only under `tests/libc-oracle/vendor/`. The Makefile/meson readers' invented
fixtures are unit-test data in `tests/libc/atf/src/gen_make_ut.rs` and `gen_meson_ut.rs` (the
ticket forbids copying donor Makefiles into fixtures).

## OpenBSD `regress/lib/libc`

One directory per topic, one `PROG` (sometimes several source files). The reader honours
`PROG=`, `SRCS+=`, `SRCS.<prog>+=`, `CFLAGS+`/`CPPFLAGS+`/`COPTS.<prog>+=` (whitelist only),
`LDADD+=` (six `-l` names), and `SUBDIR+=`. `gen-suite` records each program as
`framework = "exit-status"`; `expect_exit` defaults to 0. Tests that want arguments from their
`run-regress-*` recipe are not guessed: the row stays `expect_exit = 0` with `args = []` and a
non-zero result is a real failure to triage.

A donor root `Makefile` may be missing because the C35 vendor tool copies only directories that
contain accepted source files. `gen_make::scan` then walks the immediate children and follows
their own `SUBDIR+=` lists.

## picolibc

Programs are `test/test-*.c` and `test/libc-testsuite/*.c`. `overrides.toml` carries the handful
of `-D` flags and extra sources from `test/meson.build`; each override names the meson line.
picolibc tests that exit 77 for "not supported here" are reported by the exit-status mapping as a
failure, not silently changed to a skip; that is a known plan gap, not a hidden pass.

## mlibc

The reader takes the first `all_test_cases = [ ... ]` list from `tests/meson.build`. `rtld/`
programs are `needs-dynamic`; the scaffold has no shared-object test loader yet.

## pjdfstest

TAP scripts under `tests/<group>/*.t`, status `needs-fs`. The filesystem lane owns the pass list;
C71 only carries the suite description and the `needs-fs` status.

## LTP

`suites/ltp.toml` is `framework = "exit-status"`, `build = "external"`, with a `<dir>` supplied
at run time. LTP is GPL-2.0 (tier C) and is **never vendored**; no LTP file is present in this
repository. Exit map: 0 pass, 1 fail, 2 unresolved/fail, 4 unsupported/skip, 5 untested/skip.
