<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc 1-CPU crash at `0xca3a10`: static diagnosis

Status: **static read-only diagnosis (OBSERVED for the ELF/layout; the runtime root cause is
UNVERIFIED).** Tim owns the runtime bisect and the fix. This page classifies the fault address
and proposes the minimal experiment; it does not claim a QEMU run or a fixed crash.

## 1. Inputs

* Fixture: `tests/libc/target/smp-libc-concurrency.c` (8 threads, 2000 malloc/stdio rounds).
* Build: `tests/libc/target/build-smp.sh x86_64` (the qtest pre-step).
* ELF: `build/libc-test/libc-test`, x86-64, static, not stripped, `_start=0x401000`.
* Crash: `process: child killed by page fault at 0xca3a10` before `malloc ok`.
* Tim's diagnostic log (`/tmp/opencode/s132-diag1.log`) prints the `pthread_t` values:
  `threads[0]=0xca3a00`, `threads[1]=0x14a5a00`, ... `threads[7]=0x44b1a00`.

`readelf -lW build/libc-test/libc-test` (regenerated x86-64 build):

```
LOAD 0x000000 0x0000000000400000 0x0000000000400000 0x08a000 0x08a000 R E 0x1000
LOAD 0x08a000 0x000000000048a000 0x000000000048a000 0x00d000 0x00d000 R   0x1000
LOAD 0x097000 0x0000000000497000 0x0000000000497000 0x007000 0x007000 RW  0x1000
LOAD 0x09e000 0x000000000049e000 0x000000000049e000 0x000000 0x003654 RW  0x1000
```

Sections: `.text 0x401000..0x48a000`, `.rodata 0x48a000..0x497000`,
`.data 0x497000..0x49e000`, `.bss 0x49e000..0x4a1654`. The last symbol is
`raven_libc_tls::dtv::GENERATION` at `0x4a1650`.

## 2. Address classification

`0xca3a10` is **not code, rodata, data or bss**: it is `0x8013bc` above the end of `.bss`.
`addr2line -e build/libc-test/libc-test -f -C 0xca3a10` returns `??`/`??:0`, and the nearest
symbol below it is the bss `dtv::GENERATION` object.

It is the **TLS/TCB block of thread 0**, specifically the `Pthread.prev` field:

* `pthread_t` is the TCB address (`pthread_create` stores `thread::create`'s `tcb`).
* Tim's diagnostic prints `threads[0]=0xca3a00`; the fault is exactly `0xca3a00 + 0x10`.
* x86-64 `Pthread` layout (`libc/ravenlibc/crates/raven-libc-tls/src/tcb.rs`):
  `0x00 self_ptr`, `0x08 dtv`, **`0x10 prev`**, `0x18 next`, `0x20 sysinfo`, ...,
  `join_word` at `0xe0`, `name` at `0xee`, `sig_private` at `0x100`; total size `0x200`
  (`raw.rs:419` asserts `size_of::<Pthread>() == 512`).
* Therefore `0xca3a10` is `Pthread.prev`, not a code pointer or a guard page.

### Stack math

For `pthread_create(NULL)` the defaults are 8 MiB stack and 4 KiB guard
(`limits.rs`: `DEFAULT_STACK_SIZE = 8 * 1024 * 1024`, `DEFAULT_GUARD_SIZE = 4096`). The
x86-64 static TLS layout is 0x200 bytes (`DTV_HEADROOM + Pthread`), the TSD array is
`PTHREAD_KEYS_MAX * 8 = 128 * 8 = 0x400` bytes. `stack::allocate` therefore maps:

```
guard   = 0x1000
body    = round_up(0x800000 + 0x200 + 0x400) = 0x801000
map     = 0x802000
base    = first free page after .bss = 0x4a2000
end     = 0xca4000
tcb     = end - tsd(0x400) - tcb_size(0x200) = 0xca3a00
fault   = tcb + offset_of!(Pthread, prev) = 0xca3a10
```

The TCB is therefore **inside the intended mapping**, in its last page; the guard page is
`0x4a2000..0x4a3000`. This rules out a simple stack overflow or a low-end guard hit. It means the
page was either unmapped after allocation or made inaccessible while the TCB was still in use.

## 3. C57c path audit

| File | Finding |
|---|---|
| `thread/create.rs` | `StartArgs` is carved at `stack_limit + 16`; TCB/TLS/TSD at the top of the same mapping. `set_links` runs before `kernel.create`; the thread-list lock covers insert. No static size/offset error. |
| `thread/exit.rs` | Unlink writes `previous.prev`/`next.next` through `raw::set_links` at TCB `+0x10`/`+0x18`, then sets `exit_word=1`, then calls `kernel.exit`. A detached thread uses `exit_ex`/zombie. The first TCB write is exactly the faulting `+0x10`. |
| `thread/join.rs` | Waits for `exit_word`, then `thread_is_dead` (`join_word == 0`), then `list::sync`, `stack::reap`, reads `pthread.result`, and calls `stack::release` (unmap). The unmap frees the page containing the TCB. If any list writer still targets that TCB after this point, the fault is at `+0x10`. |
| `thread/stack.rs` | `map_size`/`guard`/TLS/TSD math above is internally consistent. The stack cache only accepts the default-size mapping; the 0x802000 mapping is unmapped, not cached. |
| `raw.rs` | `set_links`/`prev_thread`/`next_thread` use `+0x10`/`+0x18`; `thread_is_dead` reads `join_word` at TCB `+0xe0`; `Pthread` size is const-asserted at 512. |
| `pthread_*` wrappers | `pthread_create` boxes `StartInfo`, returns the TCB as `pthread_t`; `pthread_join` passes that TCB to `thread::join`. No wrapper offset bug. |
| Fixture | Creates all 8 threads first, then joins in order. A worker may exit and unlink while the main thread is creating/joining others; the list and the TCB page lifetime are the critical race surface. |

The K11 join/exit rule is present in comments (`thread_is_dead` must follow the kernel's
clear-tid), but the fault address is the list-link field, not `join_word`. The static evidence
therefore points at the **TCB lifetime across `pthread_exit` set_links and `pthread_join`
stack::release**, not at the stack-size math.

## 4. Candidate causes and first test

1. **Join/exit unmap race (most likely).** `pthread_join` unmaps the mapping that contains the
   TCB after `thread_is_dead`; a `pthread_exit`/list writer still targets `Pthread.prev` at
   TCB `+0x10`. First files to test:
   `libc/ravenlibc/crates/raven-libc-thread/src/thread/join.rs` (`timedjoin`,
   `stack::release`) and `thread/exit.rs` (`set_links`), with `raw.rs:400` (`thread_is_dead`).
2. **Early `join_word` clear / wrong clear address.** Verify
   `kernel/src/syscall/thread.rs::exit` clears exactly `tcb + offset_of!(Pthread, join_word)`
   (0xe0) and only at the final exit, not before the user `pthread_exit` finishes.
3. **List corruption (K11).** Check `insert_links`/`exit` neighbor updates and whether a
   `pthread_exit` can write a neighbor TCB whose stack was already joined.
4. Layout/guard is **unlikely**: the TCB page is inside the mapping; the guard is at the low
   end.

## 5. Minimal experiment for Tim

Do not change the committed fixture. Run these one at a time; stop at the first that changes the
fault.

1. **Leak the joined stack.** In `thread/join.rs`, temporarily skip `stack::release` (keep the
   mapping). If `0xca3a10` disappears, the bug is an unmap-after-join/TCB-use race; inspect
   `thread_is_dead`/`join_word` and `exit.rs::set_links`. If it still faults, the page was never
   mapped RW or the TCB pointer/offset is wrong.
2. **Print the lifetime tuple** in `pthread_create`, `pthread_join` and `pthread_exit`:
   `tcb`, `map_base`, `map_size`, `stack_base`, `stack_size`, `exit_word`, `join_word`,
   `prev`, `next` (the first four pages of the TCB are enough). Compare the join unmap range to
   `tcb..tcb+0x200`.
3. **Reduce the shape.** Run `THREADS=1` and `THREADS=2`; then join each thread immediately
   after create instead of creating all 8 first. A crash that needs the all-creates-first shape
   points at list/stack lifetime, not malloc/stdio.
4. **Disable the guard protection only** (`stack::allocate` `protect_none` call) as a diagnostic.
   If that changes the fault, the guard/base math is implicated; the static math says it should
   not.
5. **Do not** raise the stack size, add a retry, or mask the page fault. Those hide the lifetime
   bug.

Recommended first patch for Tim: make `pthread_join` defer or skip the TCB-containing unmap
while a list writer might still hold the TCB (or prove `thread_is_dead` cannot be true before
`pthread_exit`'s last list write), then re-run the 1-CPU case. The report records this as a
candidate, not a fix.

## 6. Donor reconciliation (DR-57)

The thread path is the existing first-party musl-derived C57c port (`create/exit/join/stack`,
musl `pthread_create.c`/`pthread_join.c` at `7813e9119eb5fe681b6750b8a033a0490bca0941`). This
diagnosis adds no donor code, no C and no copied CDDL/GPL. The layout constants are the frozen
C36/ABI values already in `limits.rs` and `tcb.rs`.

## 7. What is not claimed

This is a static read plus host symbol tools. No QEMU was run while Tim bisects; the crash timing
depends on scheduling and remains runtime evidence owned by S132/Tim. The fault address is
classified; the exact unmap writer needs the runtime experiment above.
