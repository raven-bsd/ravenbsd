<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Native exec, setuid and setgid (U3b)

OBSERVED on `stuart/U3b` (base `stuart/U3a` + `origin/master` `168c7574`). Host tests and the
`setuid` qtest; no board claim.

## Exec path

```
open file: handle -> fstat (kernel inline FSTAT) -> read/parse ELF -> proc_create
                  -> map image/stack -> PROC_EXEC_CRED(child, file, flags)
                  -> thread_create
```

`raven_rt::spawn_file(file, path, options, nosuid)` takes an **open scheme-file handle**, reads the
image from offset zero, parses it with `raven-elf` (so a script/non-ELF image never reaches the
credential step), and calls the new raw syscall `proc_exec_cred` before the first thread exists.

`proc_exec_cred` (raw number `0x3E`, not in the frozen ABI crate) takes the target process handle,
the open file handle and flags. The kernel obtains the file's 64-byte `Stat` through an inline
FSTAT reply (no user buffer to forge), checks the caller's execute permission, and only then writes
the child's effective/saved ids. `EXEC_NOSUID` (`flags & 1`) suppresses the setuid/setgid bits.
The target must be an empty process the caller may administer; applying credentials to the caller
itself is refused. `PROC_SETCRED_EX` semantics are unchanged.

## Credential matrix

| Image | Caller | Result |
|---|---|---|
| 0644 regular file | root or uid 1000 | `EACCES` before a thread starts |
| 0755 regular file, no bits | uid 1000 | child uid/euid/gid/egid all 1000 |
| 4755 regular file | uid 1000 | child euid 0; real uid 1000 |
| 2755 regular file, owner group 0 | uid 1000 | child egid 0; real gid 1000 |
| 4755 script / non-ELF | uid 1000 | rejected by the ELF parser; no credential change |
| 4755 with `nosuid` policy | uid 1000 | child euid 1000 (bits suppressed) |

Root executes a file only when at least one execute bit is set; non-root uses the
owner/group/other execute class. The kernel's setuid path changes only `euid`/`suid` and
`egid`/`sgid`; real ids and supplementary groups stay inherited.

## QEMU transcript

```
$ cargo xtask qtest --case setuid --arch x86_64
setuid: pass
exit 0
```

Serial:

```
setuid: 0644=denied
setuid: run 0755
setuid-child: mode=0755 uid=1000 euid=1000 gid=1000 egid=1000
setuid: run 4755
setuid-child: mode=4755 uid=1000 euid=0 gid=1000 egid=1000
setuid: run 2755
setuid-child: mode=2755 uid=1000 euid=1000 gid=1000 egid=0
setuid: script=bad-elf
setuid: run nosuid
setuid-child: mode=nosuid uid=1000 euid=1000 gid=1000 egid=1000
setuid: ok
init exited (0)
```

`build/qtest/setuid.json`: result `pass`, eight matched expectations, no unmatched or forbidden
hits. The fixture starts `initfsd`, opens each helper path with `initfs:`, and spawns it through
the native path.

## Evidence

```
$ cargo nextest run -p raven-ipc -p raven-proto-fs
$ cargo nextest run -p raven-rt
Summary: 115 passed
$ cargo check -p raven-kernel --target x86_64-unknown-none
exit 0
$ cargo check -p raven-kernel --target aarch64-unknown-none-softfloat
exit 0
$ cargo xtask qtest --case setuid --arch x86_64
setuid: pass
$ make check-fast
check-fast: PASSED
```

## Unsafe blocks

The kernel change adds no `unsafe`. `userland/rt/src/spawn.rs` adds the raw `proc_exec_cred`
syscall wrapper with a SAFETY comment (the crate already uses raw syscalls for the loader). The
`setuid` fixture uses raw x86-64 syscall assembly for self-info/setcred/wait with SAFETY comments;
it is a qtest binary, not product library code. No ABI or bootinfo crate was edited.

## Limits

* The `nosuid` policy is passed to `spawn_file` by the caller (the fixture supplies it); wiring a
  `nosuid` flag from the vfsd mount table into the shell/loader is a follow-up. The credential
  suppression itself is proven.
* Only native ELF images are eligible; scripts and foreign formats are rejected before the
  credential step.
* Setuid/setgid applies to the child's effective/saved ids; supplementary-group changes and
  `AT_SECURE` library hardening are follow-ups.
* No board claim; the Pi 4 release gate remains REL3/REL4.
