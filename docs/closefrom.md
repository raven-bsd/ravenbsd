<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# closefrom and close_range

K214 fixes the `closefrom(3)` / `close_range(3, INT_MAX, 0)` path that OpenSSH 10.6 uses when
`HAVE_CLOSEFROM` is absent. **OBSERVED** by the `closefrom` qtest.

## The problem

OpenSSH's `openbsd-compat/bsd-closefrom.c` calls `close_range(lowfd, INT_MAX, 0)`. The BSD
personality converted `INT_MAX` to `i32::MAX` and used the `FdTable::close_range_between`
trait default, which called `close_fd` for every integer from `lo` to `hi`: ~2.1 billion close
operations before the first SSH log line. `sshd -V` and `ssh-keygen` were unaffected because they
exit or never call `closefrom`.

## The fix

The real tables (`RealFdTable` for the Linux personality and `RealFdTableBsd`, both in
`libc/ravenlibc/crates/raven-libc/src/personality_common.rs`) now override
`close_range_between`:

* the platform table's `close_range(first, last)` walks the used-bit bitmap up to its hard
  `RLIMIT_NOFILE` and closes only the handles that are actually open; it never iterates every
  integer to `hi`;
* the call is inclusive on `hi` (the platform range is half-open, so `hi + 1` is passed);
* the personality side-table entries in `[lo, hi]` are removed in the same step;
* the returned count is the real open-descriptor delta.

`CLOSE_RANGE_CLOEXEC` uses `FdTable::set_cloexec_range`. Fake tables keep the trait's simple
loop; the real override clamps `hi` to the hard `RLIMIT_NOFILE - 1` (`bounded_hi`), because no
descriptor at or above that limit can exist. `closefrom(3)` uses the existing efficient
`close_from` path.

Semantics are unchanged: the lower bound is inclusive, the `close_range` upper bound is
inclusive, closed descriptors are ignored, single-close errors are ignored, side-table entries
are removed, `CLOSE_RANGE_UNSHARE` and unknown flags are `EINVAL`, and a reversed range is
`EINVAL`.

## Evidence

The `closefrom` qtest (libc-test harness, real console stdio) opens descriptors 5..10, then:

```
closefrom: closefrom-ms=2
closefrom: range-ms=1
closefrom: cloexec-ms=7
closefrom: done
```

`closefrom(3)` and `close_range(3, INT_MAX, 0)` close the opened descriptors and keep stdio; the
`CLOSE_RANGE_CLOEXEC` form marks them (still open, `FD_CLOEXEC` set) and is cleaned up by the
close form. The measured times are milliseconds, not the ~2^31-iteration hang.

For K195: `sshd -t` can now pass `closefrom` and reach its first log line.
