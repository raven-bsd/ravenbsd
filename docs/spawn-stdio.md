<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Spawn stdio handoff (K06c)

Status: host/qtest implementation. `base-tools` and `shell-session` pass; no board-specific claim
is made by this ticket.

## Problem

The kernel has no `exec` and a spawned child starts with an empty fd table. Rust std and the
libc stdio layer therefore treated descriptors 0/1/2 as closed: std opened `/dev/null`, and every
child write disappeared. The shell's own `tty:console` handle was never visible to the child.

## Mechanism

`libc/ravenlibc/crates/raven-libc/src/start.rs` step 10 now calls `ensure_standard_fds()` for
every process, not only for `AT_SECURE`:

1. For fd 0, 1 and 2, check `F_GETFD`; an already-open descriptor is left untouched.
2. Otherwise open the qualified scheme path `tty:console` with read/write wire flags and install
   the returned handle at that exact fd with `install_at_fd`, kind `Tty`.
3. If the console scheme is not available yet, fall back to `/dev/null` at the lowest free fd.

The child duplicates nothing from the parent: it opens its own console handles at startup. This is
the "adopt it in libc start" option in the ticket, so `SpawnOptions` call sites and the shell's
spawn call do not change. The shell continues to own its own `tty:console` handle.

## Why not pass a handle through `SpawnOptions`

`raven-libc-platform` keeps the fd table process-local; a parent cannot install a child fd. A
handle passed in options would need an auxiliary-vector channel and a second handoff protocol.
Opening `tty:console` in the child is the smaller, direct path and also covers every existing
spawn caller (initfs, file images, `GrantSys`, tools).

## Tests

```
cargo xtask qtest --case base-tools --arch x86_64
cargo xtask qtest --case shell-session --arch x86_64
make check-fast
```

`base-tools` boots `shell-test`, runs `ls /bin` in the shell and requires the child lines `cat`,
`ls` and `uname` on the serial console. `shell-session` is the full scripted session. Both cases
would fail before this change because every child write was discarded.

The fallback path is intentionally conservative: an early service whose console scheme does not
exist yet still gets `/dev/null` instead of a startup fault.

## Limitations

* The child opens three independent console handles, not one shared descriptor table entry; this is
  deliberate because the fd table is libc-local.
* A process that already has fd 0/1/2 keeps them; this ticket does not redirect a deliberate
  parent-provided descriptor scheme.
* `tty:console` availability before `consd` is a startup-order property, not a board fact.
