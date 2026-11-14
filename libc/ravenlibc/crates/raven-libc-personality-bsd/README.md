<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-libc-personality-bsd

The ring-2 BSD personality of ravenlibc (ticket C66 5.B, `02-api-surface` B01-B04, B08, B09,
B20, X56). Pure logic: the kernel and server interactions are the traits in `seams`; the C-ABI
shell (`raven-libc/src/personality_bsd/**`) implements them.

## Facilities

| Module | API | Gap |
|---|---|---|
| `kqueue` | `kqueue1/kevent`, READ/WRITE/TIMER/USER/PROC filters | `PG-7` SIGNAL, `PG-38` VNODE, `PG-44` process handles |
| `sysctl` | the static 12-row `TABLE`, name and MIB forms | `PG-21` for `hw.physmem`; no dynamic tree |
| `closefrom` | `closefrom`, `close_range` | `CLOSE_RANGE_UNSHARE` is `EINVAL` |
| `setproctitle` | prefix, restore, 255-byte truncation, fast cache | `procd` (`PG-44`); the shell ignores `ENOSYS` |
| `pthread_np` | the alias table of 02 B09 | alias-only: `raven-libc-thread` (C57) is absent |
| `getmntinfo` | `getmntinfo`/`getfsstat` over a mount seam | `PG-37` (`ENOSYS`) |
| `errno_alias` | `__error`/`__errno` | shell-only |
| `pledge_glue` | the C53 promise model and the phase-1 backend | `PG-39` enforcement absent |
| `aux` | `elf_aux_info` key widths and errors | C61 retains four values |

## Deviations recorded by C66

* `raven-libc-thread` (C57) is absent: `pthread_np` ships the alias table only, and the shell
  creates no pthread alias files (an alias of a missing base cannot link).
* `raven-libc-pledge` (C53) is present; the glue imports `reduce::PledgeState` and
  `unveil::UnveilTable` (the ticket said `UnveilState`; the table is the C53 name).
* `FdTable` gains the same additive `close_fd`/`set_cloexec`/`close_range_between` defaults as
  the Linux crate; `MountList` and `SrcOps` carry the `PG-37`/`PG-44` defaults.
* `sysctl` strings are returned with their NUL; integers accept a 4- or 8-byte caller buffer
  (`*oldlenp`), and `kern.argmax`/`hw.model` use the proposed `0x7a00` namespace.
* `setproctitle` reads the program name from C61's `program_invocation_name`; the shell ignores
  the absent-`procd` `ENOSYS` (the function is `void`).
* The installed `sys/sysctl.h` does not declare `sysctlgetmibinfo`, `sys/mount.h` does not
  declare `getmntinfo`/`getfsstat`, and `unistd.h` does not declare `close_range`/
  `setproctitle_fast`; the symbols ship anyway and the C66 report records the C38 gaps.

## Tests

`cargo test -p raven-libc-personality-bsd` runs the `_ut.rs` tables: `sys/event.h` header parity,
filter x flag matrix, receipts and room exhaustion, stale fd, 4096 cap, every `sysctl` row by
name and MIB, closefrom/close_range, setproctitle prefix rules, getmntinfo, pledge/unveil and
`elf_aux_info`, plus the integration smoke test under `tests/`.
