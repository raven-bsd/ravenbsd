<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# ABI completeness (K468)

`tests/abi-completeness` compares the kernel native-syscall dispatch surface with the generated
table in `docs/syscall-abi.md`. The test parses:

- the `raven-abi` enum variants (`crates/raven-abi/src/syscall.rs`);
- every raw `pub const NAME: usize = 0xNN;` in `crates/raven-abi/src/*.rs` and
  `kernel/src/syscall/*.rs` that `kernel/src/syscall/mod.rs` dispatches before
  `Syscall::from_usize` (including `SIGRETURN`, which the K451 inventory's
  `if number ==` regex misses);
- the `## Syscalls` rows of `docs/syscall-abi.md`;
- the known-omission and reserved-range register below.

The measured command is:

```
timeout 300 cargo nextest run -p raven-abi-completeness
```

## Measured sets at `fb1112c1`

| Set | Count |
|---|---:|
| Kernel dispatch entries | 106 |
| Documented `docs/syscall-abi.md` rows | 61 |
| Missing from the table (registered below) | 45 |
| Documented but not dispatched | 0 |
| Duplicate dispatch numbers | 0 |
| Duplicate documented numbers | 0 |
| Undocumented numeric gaps | 0 after the reserved ranges below |

The two focus rows are `PROC_CHROOT` (`0x0D`) and `PROC_REBOOT` (`0x46`). The initial red named
them first:

```
focus PROC_CHROOT (0x0d) is not registered as a known omission
focus PROC_REBOOT (0x46) is not registered as a known omission
missing from the documented table: 0x0d PROC_CHROOT (dispatch kernel/src/syscall/mod.rs:105)
missing from the documented table: 0x46 PROC_REBOOT (dispatch kernel/src/syscall/mod.rs:102)
...
ABI completeness failures (424)
```

## Fix decision

The table is **generated**: `docs/syscall-abi.md` says it is produced from
`crates/raven-abi` by `tools/abi-diff`. The generator already emits the two focus rows on this
tree:

```
$ timeout 120 cargo run -q -p abi-diff -- generate | grep -E 'PROC_CHROOT|PROC_REBOOT'
SYSCALL 0x0d PROC_CHROOT argc=2 ret=0
SYSCALL 0x46 PROC_REBOOT argc=1 ret=!
```

So `PROC_CHROOT` and `PROC_REBOOT` are a **stale generated output**, not a missing generator
input. `docs/syscall-abi.md` (the rendered table) and `tests/abi/abi-v1.txt` (the committed
snapshot) were not regenerated after the K223/K235 additions. They are outside K468 Owns, and
`crates/raven-abi`/`crates/raven-bootinfo` are reviewer-owned contract crates (handbook rule 5),
so this ticket does not edit them.

The other 43 entries are kernel-only raw numbers (`PROC_FORK`, `PREAD`, `PWRITE`, `SEEK`,
`PROC_EXEC`, the `DUPLEX_*` pair, `PIPE`/`PIPE2`, the scheduler calls and the `PROC_CTTY_*`
calls). They are dispatched before `Syscall::from_usize` but are not in `crates/raven-abi`, so
the generator cannot see them. The reviewer decision is either to extend the generator input
(the contract crate) or to classify them explicitly as private. `GETRANDOM` and `CLOCK_SET`
are the S72 reviewer-assigned raw numbers already noted in `docs/syscall-abi.md`.

**Reviewer action:** run `scripts/abi-diff.sh regenerate`, update the rendered table from the
regenerated snapshot, and either add the kernel-only raw numbers to the contract/generator or
mark them private. When an entry is fixed, remove its line from the known-omission register
below; the test fails on a stale register line.

## Known omissions (reviewer-owned)

Each comment is machine-readable; the table is the human view. The test asserts
`dispatch == documented ∪ known-omissions`, so an unregistered omission or a stale register
line fails.

<!-- abi-completeness: known-omission 0x08 GETRANDOM -->
<!-- abi-completeness: known-omission 0x09 CLOCK_SET -->
<!-- abi-completeness: known-omission 0x0D PROC_CHROOT -->
<!-- abi-completeness: known-omission 0x35 PROC_SELF_INFO -->
<!-- abi-completeness: known-omission 0x36 PROC_SETPGID -->
<!-- abi-completeness: known-omission 0x37 PROC_SETSID -->
<!-- abi-completeness: known-omission 0x38 PROC_SETCRED -->
<!-- abi-completeness: known-omission 0x39 PROC_FORK -->
<!-- abi-completeness: known-omission 0x3A PROC_PID_INFO -->
<!-- abi-completeness: known-omission 0x3B PROC_SETCRED_EX -->
<!-- abi-completeness: known-omission 0x3C PROC_CRED_INFO -->
<!-- abi-completeness: known-omission 0x3D PROC_GETGROUPS -->
<!-- abi-completeness: known-omission 0x3E PROC_HANDLE_INFO -->
<!-- abi-completeness: known-omission 0x3F PROC_EXEC_CRED -->
<!-- abi-completeness: known-omission 0x45 PROC_SET_COMMAND -->
<!-- abi-completeness: known-omission 0x46 PROC_REBOOT -->
<!-- abi-completeness: known-omission 0x48 PROC_WAIT_EX -->
<!-- abi-completeness: known-omission 0x49 PROC_GETPGID -->
<!-- abi-completeness: known-omission 0x4A PROC_SIGNAL -->
<!-- abi-completeness: known-omission 0x4B PROC_SIGRECV -->
<!-- abi-completeness: known-omission 0x4C PROC_TIMES -->
<!-- abi-completeness: known-omission 0x4D PROC_GETRUSAGE -->
<!-- abi-completeness: known-omission 0x4E PROC_GETRLIMIT -->
<!-- abi-completeness: known-omission 0x4F PROC_SETRLIMIT -->
<!-- abi-completeness: known-omission 0x6A PROC_EXEC -->
<!-- abi-completeness: known-omission 0x6B PREAD -->
<!-- abi-completeness: known-omission 0x6C PWRITE -->
<!-- abi-completeness: known-omission 0x6D SEEK -->
<!-- abi-completeness: known-omission 0x6E PIPE -->
<!-- abi-completeness: known-omission 0x6F PIPE2 -->
<!-- abi-completeness: known-omission 0x78 GETPRIORITY -->
<!-- abi-completeness: known-omission 0x79 SETPRIORITY -->
<!-- abi-completeness: known-omission 0x7A NICE -->
<!-- abi-completeness: known-omission 0x7B SCHED_GET_PRIORITY_MAX -->
<!-- abi-completeness: known-omission 0x7C SCHED_GET_PRIORITY_MIN -->
<!-- abi-completeness: known-omission 0x7D SCHED_SETSCHEDULER -->
<!-- abi-completeness: known-omission 0x7E SCHED_GETSCHEDULER -->
<!-- abi-completeness: known-omission 0x110 DUPLEX_PAIR -->
<!-- abi-completeness: known-omission 0x111 DUPLEX_SHUTDOWN -->
<!-- abi-completeness: known-omission 0x112 DUPLEX_SENDMSG -->
<!-- abi-completeness: known-omission 0x113 DUPLEX_RECVMSG -->
<!-- abi-completeness: known-omission 0x114 DUPLEX_PEERCRED -->
<!-- abi-completeness: known-omission 0x1E0 PROC_CTTY_SET -->
<!-- abi-completeness: known-omission 0x1E1 PROC_CTTY_GET -->
<!-- abi-completeness: known-omission 0x1E2 PROC_CTTY_SET_FG -->

| Number | Name | Current source |
|---|---|---|
| `0x08` | `GETRANDOM` | S72 reviewer-assigned raw number; not a `raven-abi` variant |
| `0x09` | `CLOCK_SET` | S72 reviewer-assigned raw number; not a `raven-abi` variant |
| `0x0D` | `PROC_CHROOT` | `raven-abi` raw constant; generator emits it, rendered table stale |
| `0x35` | `PROC_SELF_INFO` | kernel `process_ctl` raw number |
| `0x36` | `PROC_SETPGID` | kernel `process_ctl` raw number |
| `0x37` | `PROC_SETSID` | kernel `process_ctl` raw number |
| `0x38` | `PROC_SETCRED` | kernel `process_ctl` raw number |
| `0x39` | `PROC_FORK` | kernel `process_ctl` raw number |
| `0x3A` | `PROC_PID_INFO` | kernel `process_ctl` raw number |
| `0x3B` | `PROC_SETCRED_EX` | kernel `process_ctl` raw number |
| `0x3C` | `PROC_CRED_INFO` | kernel `process_ctl` raw number |
| `0x3D` | `PROC_GETGROUPS` | kernel `process_ctl` raw number |
| `0x3E` | `PROC_HANDLE_INFO` | kernel `process_ctl` raw number |
| `0x3F` | `PROC_EXEC_CRED` | kernel `process_ctl` raw number |
| `0x45` | `PROC_SET_COMMAND` | kernel `process_ctl` raw number |
| `0x46` | `PROC_REBOOT` | `raven-abi` raw constant; generator emits it, rendered table stale |
| `0x48` | `PROC_WAIT_EX` | kernel `process_ctl` raw number |
| `0x49` | `PROC_GETPGID` | kernel `process_ctl` raw number |
| `0x4A` | `PROC_SIGNAL` | kernel `process_ctl` raw number |
| `0x4B` | `PROC_SIGRECV` | kernel `process_ctl` raw number |
| `0x4C` | `PROC_TIMES` | kernel `process_ctl` raw number |
| `0x4D` | `PROC_GETRUSAGE` | kernel `process_ctl` raw number |
| `0x4E` | `PROC_GETRLIMIT` | kernel `process_ctl` raw number |
| `0x4F` | `PROC_SETRLIMIT` | kernel `process_ctl` raw number |
| `0x6A` | `PROC_EXEC` | kernel `exec` raw number |
| `0x6B` | `PREAD` | kernel `ipc` raw number |
| `0x6C` | `PWRITE` | kernel `ipc` raw number |
| `0x6D` | `SEEK` | kernel `ipc` raw number |
| `0x6E` | `PIPE` | kernel `pipe` raw number |
| `0x6F` | `PIPE2` | kernel `pipe` raw number |
| `0x78` | `GETPRIORITY` | kernel `priority` raw number |
| `0x79` | `SETPRIORITY` | kernel `priority` raw number |
| `0x7A` | `NICE` | kernel `priority` raw number |
| `0x7B` | `SCHED_GET_PRIORITY_MAX` | kernel `priority` raw number |
| `0x7C` | `SCHED_GET_PRIORITY_MIN` | kernel `priority` raw number |
| `0x7D` | `SCHED_SETSCHEDULER` | kernel `priority` raw number |
| `0x7E` | `SCHED_GETSCHEDULER` | kernel `priority` raw number |
| `0x110` | `DUPLEX_PAIR` | kernel `duplex` raw number |
| `0x111` | `DUPLEX_SHUTDOWN` | kernel `duplex` raw number |
| `0x112` | `DUPLEX_SENDMSG` | kernel `duplex` raw number |
| `0x113` | `DUPLEX_RECVMSG` | kernel `duplex` raw number |
| `0x114` | `DUPLEX_PEERCRED` | kernel `duplex` raw number |
| `0x1E0` | `PROC_CTTY_SET` | kernel `tty` raw number |
| `0x1E1` | `PROC_CTTY_GET` | kernel `tty` raw number |
| `0x1E2` | `PROC_CTTY_SET_FG` | kernel `tty` raw number |

## Reserved number ranges

A number that is neither dispatched nor documented is an ABI gap. The ranges below are the
reserved holes in the v1 numbering; the test fails on any hole outside them.

<!-- abi-completeness: reserved 0x00 0x00 -->
<!-- abi-completeness: reserved 0x0A 0x0A -->
<!-- abi-completeness: reserved 0x0E 0x1F -->
<!-- abi-completeness: reserved 0x27 0x2F -->
<!-- abi-completeness: reserved 0x47 0x47 -->
<!-- abi-completeness: reserved 0x56 0x57 -->
<!-- abi-completeness: reserved 0x5F 0x5F -->
<!-- abi-completeness: reserved 0x77 0x77 -->
<!-- abi-completeness: reserved 0x7F 0x7F -->
<!-- abi-completeness: reserved 0x83 0x83 -->
<!-- abi-completeness: reserved 0x85 0xFF -->
<!-- abi-completeness: reserved 0x101 0x10F -->
<!-- abi-completeness: reserved 0x115 0x1DF -->

## Mutation

- Remove a documented row from `docs/syscall-abi.md` (for example `EXIT`) and the test prints
  `documented but not dispatched: ...` and `missing from the documented table: 0x01 EXIT`.
- Duplicate a documented number (for example give `YIELD` the number `0x01`) and the test prints
  `duplicate documented number 0x01`.
- Add a new raw dispatch without a table row or register line and the test prints
  `missing from the documented table: 0x.. NAME`.
- Remove a register line after the table is fixed and the test prints `stale known omission`.

No kernel, libc or userland behaviour change; no contract crate edit.
