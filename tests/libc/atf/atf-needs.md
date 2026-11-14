# What `atf-c` itself needs from the libc

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

Observed by `grep` over the non-test `atf-c` sources in
`tests/libc-oracle/vendor/atf/atf-c` and `atf-c/detail`, and by actually compiling the archive in
oracle mode (`-std=gnu11 -D_GNU_SOURCE -DHAVE_CONFIG_H` plus our `config/host_bconfig.h`).

## Headers

`assert.h ctype.h dirent.h err.h errno.h fcntl.h libgen.h limits.h regex.h signal.h stdarg.h
stdbool.h stddef.h stdint.h stdio.h stdlib.h string.h sys/mount.h sys/param.h sys/resource.h
sys/stat.h sys/time.h sys/types.h sys/uio.h sys/wait.h unistd.h`

## Functions and syscalls

`close dup2 execvp fork getcwd geteuid kill lstat mkdtemp mkstemp open pipe read regcomp regexec
rmdir setrlimit snprintf strdup strerror umask unlink waitpid write writev`

## Result

* **Oracle build: succeeds.** All 17 non-test `.c` files compile with the host glibc and link into
  `build/libc-conformance/libatf-c/oracle/libatf-c.a`; every ATF program links it. NetBSD's
  generated `bconfig.h` is not used.
* **Target build: not attempted here.** The sysroot C60/C37/C61 install does not exist
  (`build/x86_64/sysroot` has no `usr/include/stdio.h`), so the runner records every target body
  `not-run: sysroot-incomplete:...` before compiling anything. When a sysroot lands, the first
  target failure of `atf-c` must be recorded as `atf-c-build-failed: <first missing
  header/symbol>` for the whole suite; this file is the checklist for that census input.

## Shim boundary

`shim/rl_shim.h` supplies only macros (`__arraycount`, `__unused`, `__used`, `__dead`,
`__printflike`, `__BEGIN_DECLS`, `__END_DECLS`, `__RCSID`, `__COPYRIGHT`, `__UNCONST`,
`__STRING`, `__CTASSERT`). It contains no stand-in for `strlcpy`, `getprogname`, `errx` or any
other function: a missing function is a correctly classified oracle failure.
