// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Native syscall numbers and argument conventions (ABI v1, milestone M1 set).
//!
//! Register conventions (`architecture/01-kernel.md` section 3):
//! x86-64 `syscall`: number `rax`, args `rdi rsi rdx r10 r8 r9`, result `rax`;
//! aarch64 `svc #0`: number `x8`, args `x0..x5`, result `x0`;
//! riscv64 `ecall`: number `a7`, args `a0..a5`, result `a0`.
//! A result `>= 0` is success; `-errno` is failure (see [`crate::errno`]).
//!
//! Number ranges: `0x01..` basics, `0x20..` threads/futex, `0x30..` process control,
//! `0x40..` memory, `0x50..` handles, `0x60..` scheme IPC (reserved), `0x70..` driver access
//! (reserved), `0x80..` signals (reserved), `0x100..` pager and future (reserved).

macro_rules! syscalls {
    ($($(#[$doc:meta])* $name:ident = $val:expr;)*) => {
        /// A native syscall number. Variant names mirror the documented `SYS_*` names.
        #[allow(non_camel_case_types)]
        #[repr(usize)]
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum Syscall {
            $($(#[$doc])* $name = $val,)*
        }

        impl Syscall {
            /// All defined syscalls, in declaration order.
            pub const ALL: &'static [Syscall] = &[$(Syscall::$name,)*];

            /// Convert a raw number from user space.
            pub const fn from_usize(n: usize) -> Option<Syscall> {
                match n {
                    $($val => Some(Syscall::$name),)*
                    _ => None,
                }
            }
        }
    };
}

syscalls! {
    /// `exit(code: i32) -> !` terminates the calling process (all its threads).
    EXIT = 0x01;
    /// `yield() -> 0` gives up the CPU.
    YIELD = 0x02;
    /// `debug_write(ptr: *const u8, len: usize) -> bytes_written` writes to the debug console.
    /// Requires the `ADMIN` right on the caller's process handle in later versions; open in M1.
    DEBUG_WRITE = 0x03;
    /// `clock_get(clock_id: u32, out: *mut Timespec) -> 0`. See [`crate::types::clock`].
    CLOCK_GET = 0x04;
    /// `nanosleep(ns: u64) -> 0` sleeps at least `ns` nanoseconds (may return `-EINTR` later).
    NANOSLEEP = 0x05;
    /// `uname(out: *mut Uname) -> 0` fills the system identification record.
    UNAME = 0x06;
    /// `debug_read(buf: *mut u8, len: usize, timeout_ns: u64) -> isize` reads bytes typed on the debug console (serial input).
    /// Blocks until at least one byte is available or `timeout_ns` (0 = wait forever) expires (returns 0 on timeout); returns
    /// the number of bytes stored (at most `len`). Open in this version; will need `ADMIN` like `DEBUG_WRITE`. Ticket S27.
    DEBUG_READ = 0x07;

    /// `thread_create(proc: Handle, entry: usize, stack_top: usize, arg: usize, flags: usize) -> Handle`
    /// creates a thread in `proc` starting at `entry` with the stack pointer at `stack_top`;
    /// `arg` is passed in the first argument register (`rdi`/`x0`/`a0`).
    THREAD_CREATE = 0x20;
    /// `thread_exit(code: i32) -> !` terminates the calling thread.
    THREAD_EXIT = 0x21;
    /// `futex(addr: *const u32, op: u32, val: u32, arg4: u64, addr2: usize, val3: usize) -> isize`.
    /// `op` is [`crate::types::futex::WAIT`] (sleep while `*addr == val`; `-EAGAIN` if it differs,
    /// `-ETIMEDOUT` on timeout; `arg4` is the relative timeout in nanoseconds, 0 = no timeout),
    /// [`crate::types::futex::WAKE`] (wake up to `val` waiters, returns the number woken) or
    /// [`crate::types::futex::REQUEUE`] (wake up to `val` waiters at `addr`, move up to `arg4` of
    /// the rest to `addr2`, and return the number woken plus requeued; `val3` is unused).
    FUTEX = 0x22;

    /// `proc_create(flags: usize) -> Handle` creates an empty process (no threads) and returns a
    /// handle with all rights. Flag bit 0 (`types::personality::CREATE_LINUX`) selects the Linux
    /// personality; every other bit is rejected.
    PROC_CREATE = 0x30;
    /// `proc_wait(proc: Handle, out_code: *mut i32) -> 0` blocks until the process has exited and stores its exit code.
    PROC_WAIT = 0x31;
    /// `proc_terminate(proc: Handle, code: i32) -> 0` terminates `proc` and all its threads; requires
    /// `Rights::SIGNAL` on the process handle. The exit code is stored for `PROC_WAIT`. `EBADF` for an
    /// invalid/stale handle, `EPERM` without `SIGNAL`, `ESRCH` when the process is already terminal.
    /// Terminating self is allowed and behaves like `exit(code)`. See contract C16.
    PROC_TERMINATE = 0x32;
    /// `proc_namespace(proc: Handle, op: u32, prefix: &str) -> 0` adds, removes or clears namespace prefixes; `proc` is
    /// `SELF_PROC` or a process handle with `ADMIN`. Child prefixes must be inside the caller's namespace (`ADMIN` root may
    /// grant any valid prefix). `op` is `types::namespace::{ADD,REMOVE,CLEAR}`; see contract C17.
    PROC_NAMESPACE = 0x33;

    /// `map(proc: Handle, memobj: Handle, offset: usize, addr: usize, len: usize, flags: usize) -> addr`
    /// maps `len` bytes (page multiple) of `memobj` (or fresh zeroed memory when `memobj ==
    /// Handle::NONE`) at `addr` (hint, or exact with `MAP_FIXED`) in `proc`. `flags` encodes
    /// `prot | (map_flags << 8)`, see [`crate::types::prot`] and [`crate::types::map`].
    MAP = 0x40;
    /// `unmap(proc: Handle, addr: usize, len: usize) -> 0`.
    UNMAP = 0x41;
    /// `protect(proc: Handle, addr: usize, len: usize, prot: usize) -> 0`.
    PROTECT = 0x42;

    /// `memobj_create(len: usize, flags: usize) -> Handle` creates a memory object of `len` bytes
    /// (page multiple) with rights `READ | WRITE | MAP | DUP | TRANSFER`. `flags` bit
    /// `types::memobj::PAGER` makes the caller the pager: no frames are allocated, the returned
    /// handle is the pager key, and page faults are served through `PAGER_SUPPLY`. `flags` 0 keeps
    /// the anonymous zero-filled eager object. Map it with [`Syscall::MAP`] (`map::SHARED` shares
    /// the frames, otherwise copy-on-write).
    MEMOBJ_CREATE = 0x43;
    /// `pager_supply(object: Handle, offset: usize, data: &[u8]) -> 0` supplies one page to a
    /// pager-backed memory object created with `memobj_create(..., memobj::PAGER)`. The caller must
    /// be the object's pager; `offset` is page-aligned and inside the object, `data` is 1..=PAGE
    /// bytes. The kernel copies it into a fresh frame, marks the page present and wakes waiters.
    /// Contract C18.
    PAGER_SUPPLY = 0x100;

    /// `handle_close(h: Handle) -> 0`.
    HANDLE_CLOSE = 0x50;
    /// `handle_dup(h: Handle, rights_mask: u32) -> Handle`; new rights = old rights AND mask. Requires `DUP`.
    HANDLE_DUP = 0x51;
    /// `handle_transfer(dest_proc: Handle, h: Handle, rights_mask: u32) -> Handle` installs a duplicate of `h` in the handle table of
    /// the process `dest_proc` (which needs the `WRITE` right) and returns the handle value valid IN THE DESTINATION. `h` needs
    /// `TRANSFER`; new rights = old rights AND mask (rights never grow). Used for spawn-time grants (before the child's first thread
    /// runs) and by servers. Ticket S25.
    HANDLE_TRANSFER = 0x52;
    /// `handle_revoke(h: Handle, rights_mask: u32) -> 0` removes `rights_mask` (0 = all held) from `h` and every handle derived
    /// from it, including handles in other processes. The handle stays open with the remaining rights; a later operation that needs
    /// a revoked right fails `EPERM` (a right never granted stays `EACCES`). Repeating a revoke is a no-op; a stale handle is
    /// `EBADF`. See contract C17.
    HANDLE_REVOKE = 0x53;

    /// `evq_create(flags: u32) -> Handle` creates an event queue (wait-for-many). Ticket S26.
    EVQ_CREATE = 0x58;
    /// `evq_ctl(evq: Handle, op: u32, h: Handle, events: u32, cookie: u64) -> 0`; `op` ADD=1, MOD=2, DEL=3; `events` is a mask of
    /// READABLE=1, WRITABLE=2, HUP=4, SIGNALED=8 (timer expired, event signalled, IRQ fired) plus EDGE=0x8000_0000 for edge-triggered. Ticket S26.
    EVQ_CTL = 0x59;
    /// `evq_wait(evq: Handle, out: *mut EvqEvent, max: usize, timeout_ns: u64) -> count`; `EvqEvent` is `{ cookie: u64, events: u32, _pad: u32 }`;
    /// `timeout_ns == 0` waits forever, a timeout returns 0. Ticket S26.
    EVQ_WAIT = 0x5A;
    /// `timer_create(clock_id: u32, flags: u32) -> Handle` creates a disarmed timer object usable as an event source. Ticket S26.
    TIMER_CREATE = 0x5B;
    /// `event_create(flags: u32) -> Handle` creates a user event (doorbell) that another process can signal. Ticket S26.
    EVENT_CREATE = 0x5C;
    /// `event_signal(h: Handle, value: u64) -> 0` sets the signalled state of a user event and wakes its waiters. Ticket S26.
    EVENT_SIGNAL = 0x5D;
    /// `timer_set(timer: Handle, initial_ns: u64, interval_ns: u64) -> 0` arms (`initial_ns != 0`) or disarms (0) a timer; `interval_ns != 0` repeats. Ticket S26.
    TIMER_SET = 0x5E;

    /// `scheme_register(name: &str, handle: Handle) -> 0`; register a scheme server handle under a name.
    SCHEME_REGISTER = 0x60;
    /// `open(path: &str, flags: u32, mode: u32) -> Handle`; scheme-routed open (file/scheme namespace).
    OPEN = 0x61;
    /// `read(h: Handle, buf: &mut [u8]) -> usize`.
    READ = 0x62;
    /// `write(h: Handle, buf: &[u8]) -> usize`.
    WRITE = 0x63;
    /// `close(h: Handle) -> 0`.
    CLOSE = 0x64;
    /// `dup(h: Handle, rights_mask: u32) -> Handle` (scheme-file variant of HANDLE_DUP).
    DUP = 0x65;
    /// `fstat(h: Handle, buf: &mut Stat) -> 0`.
    FSTAT = 0x66;
    /// `call(h: Handle, request: &[u8]) -> (u32, response buffer)`; typed RPC escape hatch.
    CALL = 0x67;
    /// Server side: `recv() -> (Handle, packet)`; receive the next scheme packet.
    RECV = 0x68;
    /// Server side: `send(handle: Handle, packet: &[u8]) -> 0`; reply to a packet.
    SEND = 0x69;

    /// `mmio_create(root_dev: Handle, phys: u64, len: usize, attrs: u32) -> Handle` returns an `MmioRegion` (map it with [`Syscall::MAP`]);
    /// needs the `ADMIN` root device capability (`Handle::ROOT_DEVICE`); RAM and kernel memory are refused. Ticket S21a.
    MMIO_CREATE = 0x70;
    /// `boot_blob(root_dev: Handle, kind: u32) -> Handle` returns a read-only memory object with a boot-time blob: kind 1 = ACPI tables
    /// bundle, 2 = device tree (DTB), 3 = memory map summary. Ticket S21a.
    BOOT_BLOB = 0x71;
    /// `dma_alloc(root_dev: Handle, len: usize, align: usize, mask_bits: u32, flags: u32) -> Handle` allocates device-visible memory
    /// (physically contiguous, bus address below `1 << mask_bits`); freed on close or process exit. Ticket S21b.
    DMA_ALLOC = 0x72;
    /// `dma_info(h: Handle, out: *mut DmaInfo) -> 0`; `DmaInfo` is `{ bus_addr: u64, len: u64, flags: u32, _pad: u32 }`. Ticket S21b.
    DMA_INFO = 0x73;
    /// `irq_create(root_dev: Handle, kind: u32, number: u32, flags: u32) -> Handle`; kind LEGACY=1 (IOAPIC pin), MSI=2, MSIX=3 (vector allocation). Ticket S21c.
    IRQ_CREATE = 0x74;
    /// `irq_wait(irq: Handle, timeout_ns: u64) -> count` blocks until the interrupt fired since the last wait/ack, returns the pending count. Ticket S21c.
    IRQ_WAIT = 0x75;
    /// `irq_ack(irq: Handle) -> 0` re-enables (unmasks) the interrupt after the driver handled it. Ticket S21c.
    IRQ_ACK = 0x76;
}

/// `proc_reboot(how: u32) -> !` powers off, halts or reboots the machine.
///
/// Root-only (`euid == 0`); `how` is a mask of [`RB_HALT`], [`RB_POWER_OFF`] and
/// [`RB_AUTOBOOT`]. The variant is deliberately not in [`Syscall`], so existing exhaustive libc
/// matches stay intact; the kernel dispatches the raw number before `Syscall::from_usize`.
pub const PROC_REBOOT: usize = 0x46;

/// Reboot to the normal multi-user state.
pub const RB_AUTOBOOT: u32 = 0;
/// Halt without powering off.
pub const RB_HALT: u32 = 0x0000_0008;
/// Power off if the platform supports it.
pub const RB_POWER_OFF: u32 = 0x0000_4000;
/// Mask of reboot flags accepted by [`PROC_REBOOT`].
pub const RB_FLAGS: u32 = RB_HALT | RB_POWER_OFF;

/// `trace_set(id: u64) -> 0`.
///
/// Sets the calling thread's current trace id (`R-OBS-6`); zero clears it. Unprivileged: a thread
/// may only set its own context, and the kernel stamps that id on its scheme requests and lets a
/// server inherit it for the duration of the call. A raw constant, not a [`Syscall`] variant, so
/// existing exhaustive matches downstream stay intact. `0x08`/`0x09` are S72's `GETRANDOM` and
/// `CLOCK_SET`; `TRACE_SET` is `0x0B`.
pub const TRACE_SET: usize = 0x0B;

/// `record_lock(file: Handle, cmd: u32, lock: *mut RecordLock) -> 0`.
///
/// POSIX advisory byte-range lock operations: `cmd` is `F_GETLK` (5), `F_SETLK` (6) or
/// `F_SETLKW` (7); `lock` is a [`types::fcntl::RecordLock`]. The kernel resolves the file identity,
/// enforces the range conflicts and blocks `F_SETLKW` until the range is free. Raw constant, not a
/// [`Syscall`] variant; the lock table lives in the kernel so close and process-exit release are
/// exact.
pub const RECORD_LOCK: usize = 0x0C;

/// `proc_chroot(path: *const u8, len: usize) -> 0` sets the calling process's root.
///
/// Root-only (`euid == 0`); the path is resolved inside the current root, must name a directory
/// and is stored as a normalized absolute path without a trailing slash. A raw constant, not a
/// [`Syscall`] variant, so the existing exhaustive matches downstream stay intact.
pub const PROC_CHROOT: usize = 0x0D;

/// `msync(proc: Handle, addr: usize, len: usize, flags: usize) -> 0`.
///
/// Writes dirty pages of a shared file-backed mapping back through the pager without unmapping
/// the range. `flags` uses the POSIX `MS_ASYNC`/`MS_SYNC`/`MS_INVALIDATE` bits; `MS_SYNC` waits
/// for the PageOut. A raw constant, not a [`Syscall`] variant, so existing exhaustive matches
/// downstream stay intact.
pub const MSYNC: usize = 0x44;

/// `proc_set_personality(proc: Handle, personality: u32, handler: Handle) -> 0`.
///
/// This is a native-only control syscall in the `0x30` process-control range,
/// but it is deliberately not a [`Syscall`] variant: adding one would break
/// every exhaustive downstream match for a minimal ABI extension. Dispatch
/// checks this raw number before [`Syscall::from_usize`]. `proc` is
/// `SELF_PROC` or a process handle with `ADMIN`; a non-self target must have no
/// threads yet. A non-`Handle::NONE` handler must be a transferable scheme
/// file/server and is duplicated into `proc`.
pub const PROC_SET_PERSONALITY: usize = 0x34;

/// `thread_set_tls(thread: Handle, tp: usize) -> 0`.
///
/// Sets the per-thread user TLS base (`IA32_FS_BASE` on x86-64, `TPIDR_EL0`
/// on aarch64). `Handle::NONE` means the calling thread; otherwise the handle
/// needs `Rights::SIGNAL`. A raw constant, not a `Syscall` variant, so existing
/// exhaustive matches in the libc crates stay intact.
pub const THREAD_SET_TLS: usize = 0x23;

/// `thread_kick(thread: Handle, bits: u32) -> 0`.
///
/// ORs sticky kick bits into the per-thread word. `Handle::NONE` targets the
/// calling thread; otherwise the thread handle needs `Rights::SIGNAL`. Bit
/// meanings are the C58/PG-8 PROPOSED set (CHECK, CANCEL, SYNCCALL, TIMER);
/// this constant and the kernel do not raise signals.
pub const THREAD_KICK: usize = 0x81;

/// `proc_sigrecv_ex(out: *mut [u64; 6], flags: u32, deliverable: u64) -> signo` (raw 0x85).
///
/// The six-word output is `[signo, sender_pid, si_code, si_uid, si_value, 0]`. With
/// [`PROC_SIGRECV_DELIVERABLE`] the kernel selects only a queued signal whose bit is in
/// `deliverable`; every masked signal stays queued. Raw constant, not a `Syscall` variant.
pub const PROC_SIGRECV_EX: usize = 0x85;

/// `proc_sigqueue(target: pid, signo: i32, value: u64) -> 0` (raw 0x86).
///
/// Queues one `SI_QUEUE` record carrying `value` for `target`. Raw constant.
pub const PROC_SIGQUEUE: usize = 0x86;

/// `PROC_SIGRECV_EX` flag: select only a signal whose bit is in the deliverable mask.
pub const PROC_SIGRECV_DELIVERABLE: u32 = 2;

/// `sigaction(signo: i32, act: *const SigAction, old: *mut SigAction) -> 0` (raw 0x87).
///
/// Installs, replaces or reads the per-process handler for one signal. `act` is null to read
/// without changing; `old` is null to ignore the previous value. The structure layout is the
/// System V amd64 `struct sigaction` (see `tickets/CONTRACTS.md` C19 and the S160 offset table),
/// not the retired `SIG_SETUP` model. Raw constant, not a `Syscall` variant.
pub const SIGACTION: usize = 0x87;

/// `sigprocmask(how: i32, set: *const u64, old: *mut u64) -> 0` (raw 0x88).
///
/// POSIX `SIG_BLOCK`/`SIG_UNBLOCK`/`SIG_SETMASK` on the calling thread's mask. The mask is
/// per-thread; handlers are per-process. A signal that becomes unblocked and is pending is
/// delivered before the call returns to user mode. Raw constant.
pub const SIGPROCMASK: usize = 0x88;

/// `sigaltstack(ss: *const Stack, old: *mut Stack) -> 0` (raw 0x89).
///
/// Installs or reads the per-thread alternate signal stack (`SS_ONSTACK`/`SS_DISABLE`
/// semantics). Raw constant.
pub const SIGALTSTACK: usize = 0x89;

/// `thread_sigqueue(thread: Handle, signo: i32, value: u64) -> 0` (raw 0x8A).
///
/// Queues one `SI_QUEUE` record on the target thread's own queue (FreeBSD `td_sigqueue`).
/// `Handle::NONE` targets the calling thread; another thread needs `Rights::SIGNAL` and the
/// caller must pass the same permission test as `PROC_SIGNAL`. `signo == 0` is a pure
/// existence/permission check and queues nothing. Raw constant, not a `Syscall` variant.
pub const THREAD_SIGQUEUE: usize = 0x8A;

/// `sigpending(out: *mut u64) -> count` (raw 0x8B).
///
/// Writes the authoritative kernel pending mask for the calling thread (its thread-directed
/// queue ORed with the process queue, signal 1 is bit 0) and returns the number of queued
/// records. The output word uses the same bit layout as `sigprocmask`. Raw constant.
pub const SIGPENDING: usize = 0x8B;

/// `sigwait(set: *const u64, out: *mut [u64; 6], timeout_ns: u64, flags: u32) -> signo`
/// (raw 0x8C).
///
/// Atomically makes every signal in `set` deliverable for the calling thread, then consumes the
/// oldest matching pending record without running its handler. The temporary mask is restored
/// on return. `out` receives the same six words as `PROC_SIGRECV_EX`. `flags` bit 0
/// (`SIGWAIT_POLL`) returns `-EAGAIN` instead of blocking; `timeout_ns != 0` bounds the wait
/// (`-EAGAIN` on expiry). Raw constant.
pub const SIGWAIT: usize = 0x8C;

/// `SIGWAIT` flag: poll once and return `-EAGAIN` when no matching signal is pending.
pub const SIGWAIT_POLL: u32 = 1;

/// `thread_self() -> tid`.
///
/// Returns the calling thread's stable 30-bit, nonzero tid. A raw constant,
/// not a `Syscall` variant.
pub const THREAD_SELF: usize = 0x24;

/// `robust_list_set(head: usize, len: usize) -> 0`.
///
/// Registers the calling thread's Linux `robust_list_head`; `head == 0` and
/// `len == 0` clears it. Raw constant, not a `Syscall` variant.
pub const ROBUST_LIST_SET: usize = 0x26;

/// `handle_copy(dst_proc, src_handle, rights_mask) -> Handle`.
///
/// Copies a live source handle into `dst_proc`, requiring `ADMIN` on the
/// caller and the destination process. Raw constant; not a `Syscall` variant.
pub const HANDLE_COPY: usize = 0x54;

/// `handle_transfer_file(dest_file, handle, rights_mask) -> Handle`.
///
/// Installs a reduced duplicate of `handle` in the scheme server that owns
/// `dest_file` and returns the destination-valid handle, so a client can send a
/// shared PCM ring to a server without holding the server's process handle. The
/// file must be a scheme file the caller can read, the source must be
/// transferable, and the destination rights are the source rights intersected
/// with `rights_mask`. Raw constant; not a `Syscall` variant.
pub const HANDLE_TRANSFER_FILE: usize = 0x55;

/// `thread_exit_ex(code: i32, clear_addr: usize, unmap_addr: usize, unmap_len: usize) -> !`.
///
/// Stores zero to `clear_addr`, unmaps `[unmap_addr, unmap_addr + unmap_len)`,
/// wakes futex waiters on `clear_addr` and exits. Raw constant.
pub const THREAD_EXIT_EX: usize = 0x25;

/// `THREAD_CREATE` flag: the sixth argument points to a [`ThreadCreateEx`].
pub const THREAD_CREATE_EX: usize = 1;

/// Extended `THREAD_CREATE` payload (PG-2), passed through the sixth argument
/// when [`THREAD_CREATE_EX`] is set.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadCreateEx {
    /// User TLS base for the new thread (zero leaves it unset).
    pub tp: usize,
    /// Caller address that receives the new 30-bit tid (zero skips the write).
    pub tid_out: usize,
    /// New thread's clear-on-exit word (zero disables it).
    pub clear_tid_addr: usize,
}

const _: () = {
    assert!(core::mem::size_of::<ThreadCreateEx>() == 24);
    assert!(core::mem::align_of::<ThreadCreateEx>() == 8);
    assert!(core::mem::offset_of!(ThreadCreateEx, tp) == 0);
    assert!(core::mem::offset_of!(ThreadCreateEx, tid_out) == 8);
    assert!(core::mem::offset_of!(ThreadCreateEx, clear_tid_addr) == 16);
};

#[cfg(test)]
#[path = "syscall_ut.rs"]
mod tests;
