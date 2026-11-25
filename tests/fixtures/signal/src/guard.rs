// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Guard-page regression for the private `SIGACTION` wire (C19/S710).
//!
//! The kernel copies exactly the 40-byte `UserSigAction` request and writes exactly 40 bytes
//! back. A 152-byte C `struct sigaction` crossing this syscall is the historical defect: it
//! turns the flags at offset 24 into mask bytes and can overwrite the caller's adjacent memory.
//!
//! Every user object here is the private 40-byte request, never the public C struct. Read and
//! write objects are placed at the last slot before an inaccessible guard page, and a separate
//! writable copy is surrounded by canaries so an oversized write is observable as data damage.

use core::ptr;

use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_rt::println;
use raven_rt::sys;

const PAGE: usize = 4096;
const WIRE: usize = 40;
const SIGUSR1: i32 = 10;
const SIGKILL: i32 = 9;
const SIGSTOP: i32 = 19;
const SA_SIGINFO: u32 = 4;
const SA_ONSTACK: u32 = 0x0800_0000;
const SA_RESTART: u32 = 0x1000_0000;
/// A flag bit outside the donor `kern_sigaction` accepted set.
const SA_UNKNOWN: u32 = 0x2000_0000;
const EFAULT: isize = -14;
const EINVAL: isize = -22;
const CANARY: u8 = 0x5a;
const POISON: u8 = 0xa5;
const A_MASK: u64 = 0x0000_0000_0000_0002;
const B_MASK: u64 = 0x0000_0000_0000_0004;

/// The Raven 40-byte `SIGACTION` request (`kernel/src/syscall/thread.rs:UserSigAction`).
#[repr(C)]
#[derive(Clone, Copy)]
struct KernelSigAction {
    handler: u64,
    mask: [u64; 2],
    flags: u32,
    _pad: u32,
    restorer: u64,
}

const _: () = assert!(core::mem::size_of::<KernelSigAction>() == WIRE);
const _: () = assert!(core::mem::offset_of!(KernelSigAction, handler) == 0);
const _: () = assert!(core::mem::offset_of!(KernelSigAction, mask) == 8);
const _: () = assert!(core::mem::offset_of!(KernelSigAction, flags) == 24);
const _: () = assert!(core::mem::offset_of!(KernelSigAction, restorer) == 32);

impl KernelSigAction {
    const fn zero() -> Self {
        Self {
            handler: 0,
            mask: [0; 2],
            flags: 0,
            _pad: 0,
            restorer: 0,
        }
    }
}

extern "C" fn raw_handler(_sig: i32) {}
extern "C" fn raw_handler_b(_sig: i32) {}

fn handler_address() -> u64 {
    raw_handler as *const () as u64
}

fn handler_b_address() -> u64 {
    raw_handler_b as *const () as u64
}

/// The distinguishable action A installed before a combined call.
const fn action_a() -> KernelSigAction {
    KernelSigAction {
        handler: 0,
        mask: [A_MASK, 0],
        flags: SA_SIGINFO,
        _pad: 0,
        restorer: 0,
    }
}

/// The distinguishable action B installed by a combined call. The restorer is deliberately
/// nonzero: the kernel does not store it, so a reply that leaks the request restorer is visible.
const fn action_b() -> KernelSigAction {
    KernelSigAction {
        handler: 0,
        mask: [B_MASK, 0],
        flags: SA_SIGINFO | SA_ONSTACK | SA_RESTART,
        _pad: 0,
        restorer: 0xdead_beef_dead_beef,
    }
}

/// Whether all 40 bytes of a live slot still hold `byte`.
fn slot_is(address: usize, byte: u8) -> bool {
    for offset in 0..WIRE {
        // SAFETY: every call site passes a live 40-byte slot inside a mapped page.
        if unsafe { ptr::read((address + offset) as *const u8) } != byte {
            return false;
        }
    }
    true
}

/// One read/write page followed by an inaccessible guard page.
struct GuardRegion {
    base: usize,
    len: usize,
}

impl GuardRegion {
    fn new() -> Option<Self> {
        let len = PAGE * 2;
        let base = sys::map_anonymous(len).ok()?;
        // SAFETY: PROTECT takes scalar arguments; `base + PAGE` is the live second page of the
        // mapping and zero is a valid no-access protection.
        let protected = unsafe {
            sys::syscall4(
                Syscall::PROTECT as usize,
                Handle::SELF_PROC.0 as usize,
                base + PAGE,
                PAGE,
                0,
            )
        };
        if protected < 0 {
            let _ = sys::unmap(Handle::SELF_PROC, base, len);
            return None;
        }
        Some(Self { base, len })
    }

    /// The last 40-byte slot before the guard page.
    fn last_wire_slot(&self) -> usize {
        self.base + PAGE - WIRE
    }

    /// Start of the inaccessible second page.
    fn guard(&self) -> usize {
        self.base + PAGE
    }
}

impl Drop for GuardRegion {
    fn drop(&mut self) {
        let _ = sys::unmap(Handle::SELF_PROC, self.base, self.len);
    }
}

fn sigaction_for(signo: i32, act: usize, old: usize) -> isize {
    // SAFETY: SIGACTION copies the 40-byte live requests at the caller-supplied addresses; the
    // kernel validates both ranges and a null pointer means the object is absent.
    unsafe { sys::syscall3(raven_abi::syscall::SIGACTION, signo as usize, act, old) }
}

fn sigaction(act: usize, old: usize) -> isize {
    sigaction_for(SIGUSR1, act, old)
}

fn write_request(address: usize, action: KernelSigAction) {
    // SAFETY: every call site passes an 8-aligned slot inside a live writable page and writes
    // exactly the 40-byte wire object.
    unsafe { ptr::write(address as *mut KernelSigAction, action) };
}

fn read_request(address: usize) -> KernelSigAction {
    // SAFETY: every call site passes an 8-aligned slot inside a live readable page filled by
    // the kernel with exactly the 40-byte wire object.
    unsafe { ptr::read(address as *const KernelSigAction) }
}

fn fill(address: usize, byte: u8, len: usize) {
    // SAFETY: the caller passes a live writable range inside one mapped page.
    unsafe { ptr::write_bytes(address as *mut u8, byte, len) };
}

fn canaries_intact(base: usize, holes: &[(usize, usize)]) -> bool {
    for offset in 0..PAGE {
        let inside = holes
            .iter()
            .any(|(start, end)| offset >= *start && offset < *end);
        if inside {
            continue;
        }
        // SAFETY: `offset` is inside the live page mapped by the caller.
        let byte = unsafe { ptr::read((base + offset) as *const u8) };
        if byte != CANARY {
            return false;
        }
    }
    true
}

fn expect(name: &'static str, condition: bool) -> Result<(), &'static str> {
    if condition {
        println!("signal-raw: {name} ok");
        Ok(())
    } else {
        println!("signal-raw: {name} failed");
        Err(name)
    }
}

/// Run every guarded raw-wire row. Returns the first failed row name.
pub fn run() -> Result<(), &'static str> {
    // new-only: the 40-byte request is readable, a 152-byte read would enter the guard page.
    {
        let region = GuardRegion::new().ok_or("new-only-map")?;
        let at = region.last_wire_slot();
        let mut action = KernelSigAction::zero();
        action.handler = handler_address();
        action.flags = SA_SIGINFO;
        write_request(at, action);
        expect("new-only", sigaction(at, 0) == 0)?;
    }

    // old-only: the 40-byte write is writable, a 152-byte write would enter the guard page.
    {
        let region = GuardRegion::new().ok_or("old-only-map")?;
        let old = region.last_wire_slot();
        expect("old-only", sigaction(0, old) == 0)?;
        let previous = read_request(old);
        expect(
            "old-only-fields",
            previous.handler == handler_address() && previous.flags & SA_SIGINFO != 0,
        )?;
    }

    // both: preinstall a distinguishable action A, poison the old slot, then install a distinct
    // action B with `act` and `old` both non-NULL. The combined call must return A in the same
    // call, before any second query; a separate query must then observe B.
    {
        let a_region = GuardRegion::new().ok_or("both-a-map")?;
        let act_region = GuardRegion::new().ok_or("both-act-map")?;
        let old_region = GuardRegion::new().ok_or("both-old-map")?;
        let query_region = GuardRegion::new().ok_or("both-query-map")?;
        let mut a = action_a();
        a.handler = handler_address();
        write_request(a_region.last_wire_slot(), a);
        expect("both-preinstall", sigaction(a_region.last_wire_slot(), 0) == 0)?;

        let at = act_region.last_wire_slot();
        let old = old_region.last_wire_slot();
        let query = query_region.last_wire_slot();
        fill(old, POISON, WIRE);
        let mut b = action_b();
        b.handler = handler_b_address();
        write_request(at, b);
        expect("both-install", sigaction(at, old) == 0)?;
        // Inspect the combined call's own old output before it can be overwritten by a query.
        let returned = read_request(old);
        expect(
            "both-returned-old",
            returned.handler == handler_address()
                && returned.mask == [A_MASK, 0]
                && returned.flags == SA_SIGINFO
                && returned.restorer == 0,
        )?;
        // The installed disposition is B; query it into a separate object.
        expect("both-query", sigaction(0, query) == 0)?;
        let installed = read_request(query);
        expect(
            "both-installed-new",
            installed.handler == handler_b_address()
                && installed.mask == [B_MASK, 0]
                && installed.flags == SA_SIGINFO | SA_ONSTACK | SA_RESTART
                && installed.restorer == 0,
        )?;
    }

    // neither: both pointers null is a successful no-op.
    expect("neither", sigaction(0, 0) == 0)?;

    // invalid pointers: the inaccessible guard page and an unmapped address must be EFAULT.
    {
        let region = GuardRegion::new().ok_or("invalid-act-map")?;
        expect("invalid-new-guard", sigaction(region.guard(), 0) == EFAULT)?;
        expect("invalid-old-guard", sigaction(0, region.guard()) == EFAULT)?;
        expect(
            "invalid-new-unmapped",
            sigaction(0x0000_0fff_ffff_f000, 0) == EFAULT,
        )?;
    }

    // failure side effects: a rejected action must return the exact errno and leave the caller's
    // old buffer byte-unchanged. FreeBSD `sys_sigaction` copies the old action out only when the
    // whole call succeeds (`kern_sig.c:952-958`), and `kern_sigaction:801-805` rejects an invalid
    // action before the old action is captured.
    {
        let act_region = GuardRegion::new().ok_or("fail-act-map")?;
        let old_region = GuardRegion::new().ok_or("fail-old-map")?;
        let at = act_region.last_wire_slot();
        let old = old_region.last_wire_slot();

        // unknown flag on a caught handler
        let mut flagged = KernelSigAction::zero();
        flagged.handler = handler_address();
        flagged.flags = SA_UNKNOWN;
        write_request(at, flagged);
        fill(old, POISON, WIRE);
        expect("fail-flag-errno", sigaction(at, old) == EINVAL)?;
        expect("fail-flag-old-unchanged", slot_is(old, POISON))?;

        // SIGKILL with a handler
        let mut kill = KernelSigAction::zero();
        kill.handler = handler_address();
        write_request(at, kill);
        fill(old, POISON, WIRE);
        expect("fail-sigkill-errno", sigaction_for(SIGKILL, at, old) == EINVAL)?;
        expect("fail-sigkill-old-unchanged", slot_is(old, POISON))?;

        // SIGSTOP with a handler
        fill(old, POISON, WIRE);
        expect("fail-sigstop-errno", sigaction_for(SIGSTOP, at, old) == EINVAL)?;
        expect("fail-sigstop-old-unchanged", slot_is(old, POISON))?;

        // unmapped act pointer
        fill(old, POISON, WIRE);
        expect(
            "fail-unmapped-errno",
            sigaction(0x0000_0fff_ffff_f000, old) == EFAULT,
        )?;
        expect("fail-unmapped-old-unchanged", slot_is(old, POISON))?;
    }

    // Combined-call canary row: preinstall A, poison the old slot, install B with both pointers,
    // and prove the returned old is A before any query. Canaries prove neither 40-byte object nor
    // the bytes beyond them move; a 152-byte write would damage the canary after each slot.
    {
        let base = sys::map_anonymous(PAGE).map_err(|_| "canary-map")?;
        fill(base, CANARY, PAGE);
        let a_at = 64;
        let at = 320;
        let old = 512;
        let query = 832;
        let mut a = action_a();
        a.handler = handler_address();
        write_request(base + a_at, a);
        let preinstall = sigaction(base + a_at, 0);
        let mut b = action_b();
        b.handler = handler_b_address();
        write_request(base + at, b);
        fill(base + old, POISON, WIRE);
        let install = sigaction(base + at, base + old);
        let returned = read_request(base + old);
        let query_result = sigaction(0, base + query);
        let installed = read_request(base + query);
        let intact = canaries_intact(
            base,
            &[
                (a_at, a_at + WIRE),
                (at, at + WIRE),
                (old, old + WIRE),
                (query, query + WIRE),
            ],
        );
        let returned_a = returned.handler == handler_address()
            && returned.mask == [A_MASK, 0]
            && returned.flags == SA_SIGINFO
            && returned.restorer == 0;
        let installed_b = installed.handler == handler_b_address()
            && installed.mask == [B_MASK, 0]
            && installed.flags == SA_SIGINFO | SA_ONSTACK | SA_RESTART
            && installed.restorer == 0;
        if !returned_a || !installed_b {
            println!(
                "signal-raw: canary values returned handler=0x{:x} mask=0x{:x},0x{:x} flags=0x{:x} restorer=0x{:x}",
                returned.handler,
                returned.mask[0],
                returned.mask[1],
                returned.flags,
                returned.restorer
            );
        }
        expect("canary-preinstall", preinstall == 0)?;
        expect("canary-install", install == 0)?;
        expect("canary-query", query_result == 0)?;
        expect("canary-data", intact)?;
        expect("canary-returned-old", returned_a)?;
        expect("canary-installed-new", installed_b)?;
        let _ = sys::unmap(Handle::SELF_PROC, base, PAGE);
    }

    // Query the installed SA_ONSTACK disposition back (step 4 readback) without raising it.
    {
        let region = GuardRegion::new().ok_or("readback-map")?;
        let old = region.last_wire_slot();
        expect("readback-onstack", sigaction(0, old) == 0)?;
        let previous = read_request(old);
        expect(
            "readback-onstack-flags",
            previous.flags & (SA_SIGINFO | SA_ONSTACK | SA_RESTART)
                == SA_SIGINFO | SA_ONSTACK | SA_RESTART,
        )?;
    }

    Ok(())
}
