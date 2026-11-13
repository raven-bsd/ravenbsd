<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-tls

The thread control block (`Pthread`), the ELF TLS block layout and the dynamic thread vector.
Layer L0; no `alloc`, no `std`, no `unsafe` outside `raw.rs`.

`cargo test -p raven-libc-tls` runs the golden vectors of C57 section 7.1 (`tests/vectors.rs`),
the 100,000-case property loop per architecture and the independent model differential
(`tests/model.rs`), plus the per-module unit tests.

## Loader obligations (C62 reads these five sentences)

1. `static_tls_cnt` is the number of `PT_TLS` modules known at startup: the executable's one
   module, or zero when the program has no `PT_TLS`; it comes from
   `compute_static_layout(modules, size_of::<Pthread>())` and its `tls_cnt`.
2. Every new module loaded by `dlopen` must be allocated EAGERLY for every existing thread at
   load time (A-tls-2), never lazily on first access, and its per-thread block must be added to
   every thread's DTV before any thread can execute code that uses it.
3. `install_new_tls` runs under the thread-list lock, copies each thread's old DTV words, appends
   the new module's `block + DTP_OFFSET`, and publishes each thread's new DTV with Release
   ordering (A-tls-1).
4. A thread reads its DTV pointer with Acquire (never a plain load) and `dtv[module]` is only
   valid when `module <= dtv[0]`; `dtv[0]` is the module count, not a generation.
5. A DT_TLS block of a recycled stack must be zeroed before the image is copied and the DTV is
   rewritten (A-tls-4), because `.tbss` bytes of the previous thread are otherwise visible.

## Layout formula

For `memsz`, image address `A` and effective `align` (power of two, at most 4096):

* `size = memsz + ((-memsz - A) & (align - 1))` (wrapping, overflow-checked);
* variant II: `offset = size`; variant I: `offset = GAP_ABOVE_TP + ((-GAP_ABOVE_TP + A) & (align - 1))`;
* `align = max(align, MIN_TLS_ALIGN)` and
  `total = (2*8 + tcb_size [+ offset on variant I] + size + align + MIN_TLS_ALIGN - 1) & -MIN_TLS_ALIGN`.

The `Pthread` is 512 bytes on the three targets. On x86-64 the first six words and `tid`/`errno`
are at the ABI offsets 0/8/16/24/32/40/48/52; on AArch64 and RISC-V the structure ends with
`canary` and `dtv`, so `dtv` is the word at `tp - 8`.
