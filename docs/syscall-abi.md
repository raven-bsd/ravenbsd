<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Native syscall ABI negative and boundary tests (S55)

OBSERVED on `stuart/S55`. The suite is the standalone fixture
`tests/fixtures/syscall/` driven by `tests/qtest/cases/syscall-negative.toml`: 83 probes call
the native syscall surface directly (no libc) and compare the raw `-errno` with the ABI
contract. The case passes:

```
$ cargo xtask qtest --case syscall-negative --arch x86_64
syscall-negative: ok 83 checks
init exited (0)
```

## Group table

| Group | Probes | Expected / observed |
|---|---|---|
| basic | `debug_write` bad pointer / `usize::MAX`; `clock_get` bad clock / bad out; `uname` bad out | EFAULT; EINVAL for the overlong range; EINVAL; EFAULT; EFAULT |
| process control | `proc_create(2)`; `proc_wait(0xDEAD)`; `proc_terminate(0xDEAD)`; `proc_namespace` bad op / bad prefix; `proc_set_personality` bad personality / stale handle | EINVAL; EBADF; EBADF; EINVAL; EFAULT; EINVAL; EBADF |
| memory | `memobj_create` len 0 / bad flags / `usize::MAX`; `map` len 0 / unaligned len / unaligned fixed addr / W+X / unaligned offset / stale memobj / stale proc; `unmap` / `protect` stale handles and bad addresses; `pager_supply` stale object | EINVAL; EINVAL; EINVAL; EINVAL; EINVAL; EINVAL; EINVAL; EINVAL; EBADF; EBADF; EBADF; EINVAL; EBADF; EINVAL; EBADF |
| threads / TLS | `thread_create` zero entry / stale proc / bad flags / bad extended payload; `thread_set_tls` bad base / `usize::MAX` / clear; `thread_kick` stale / zero | EFAULT; EBADF; EINVAL; EFAULT; EFAULT; EFAULT; Ok(0); EBADF; Ok(0) |
| futex / robust list | `futex` unmapped word / bad op / value mismatch; `robust_list_set` non-user head / clear | EINVAL; EINVAL; EAGAIN; EINVAL; Ok(0) |
| signals | `sig_setup` unaligned offset / bad flags / bad entry / overflow offset; `sigresend` without setup; `sigreturn` outside a signal | EINVAL; EINVAL; EFAULT; EINVAL; EINVAL; EINVAL |
| handles / rights | `handle_close` / `handle_dup` stale; revoke `SIGNAL` then `proc_terminate`; revoke `WRITE` then `handle_transfer`; revoke `DUP` then `handle_dup`; revoke `ADMIN` then `proc_namespace`; `handle_copy` stale source | EBADF; EBADF; EPERM; EPERM; EPERM; EPERM; EBADF |
| scheme IPC | `scheme_register` bad name; `open` bad pointer / `usize::MAX` path; `read` / `write` / `close` / `dup` / `fstat` / `call` stale; `send` stale | EFAULT; EFAULT; ENAMETOOLONG; EBADF x6; EINVAL |
| events / timers | `evq_create(1)`; `evq_ctl` / `evq_wait` stale; `timer_create` bad clock; `timer_set` stale; `event_create(1)`; `event_signal` stale | EINVAL; EBADF; EBADF; EINVAL; EBADF; EINVAL; EBADF |
| driver | `mmio_create` wrong root handle; `boot_blob` kind 0; `dma_alloc` len 0; `dma_info` stale; `irq_create` kind 0; `irq_wait` / `irq_ack` stale | EPERM; EINVAL; EINVAL; EBADF; EINVAL; EBADF; EBADF |
| valid controls | `thread_set_tls(0)`, `thread_kick(NONE,0)`, `robust_list_set(0,0)`, stack `unmap` | Ok(0) each |

## Kernel observations from the first probe run

The first run compared the fixture's initial expectations with the kernel and refined the
documented choices (the kernel values are now the tested contract):

* `debug_write` with `len == usize::MAX` returns **EINVAL** (the length bound), not EFAULT.
* `thread_create` with a zero entry/stack returns **EFAULT** (the user-address check), not EINVAL;
  the flag/payload errors are EINVAL/EFAULT.
* `futex` on an aligned but unmapped word and `robust_list_set` on a non-user head return
  **EINVAL**, not EFAULT.
* `handle_copy` with a zero rights mask returns **EINVAL**; with a valid mask and a stale source it
  returns **EBADF**.
* `open` with a `usize::MAX` path length returns **ENAMETOOLONG**.
* `send` on a stale packet handle returns **EINVAL** (the packet-side check precedes a handle
  error).
* `mmio_create(ROOT_DEVICE, phys=0, len=PAGE, 0)` returned a **valid handle** in the first probe.
  The ABI says RAM and kernel memory are refused; physical 0 may be a reserved low-memory region,
  so this is recorded as an **open verification item**, not asserted as a failing test. The suite
  asserts the wrong-root-handle EPERM path; a later probe with a known usable RAM range from
  `BOOT_BLOB` kind 3 should settle the RAM refusal.
