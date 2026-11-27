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
const SA_SIGINFO: u32 = 4;
const SA_ONSTACK: u32 = 0x0800_0000;
const SA_RESTART: u32 = 0x1000_0000;
const EFAULT: isize = -14;
const EINVAL: isize = -22;
const CANARY: u8 = 0x5a;

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

const MASK_A: u64 = 0x2;
const MASK_B: u64 = 0x4;

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

fn sigaction(act: usize, old: usize) -> isize {
    sigaction_signo(SIGUSR1, act, old)
}

fn sigaction_signo(signo: i32, act: usize, old: usize) -> isize {
    // SAFETY: SIGACTION copies the 40-byte live requests at the caller-supplied addresses; the
    // kernel validates both ranges and a null pointer means the object is absent.
    unsafe { sys::syscall3(raven_abi::syscall::SIGACTION, signo as usize, act, old) }
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

fn request_is(address: usize, byte: u8) -> bool {
    (0..WIRE).all(|offset| {
        // SAFETY: every caller passes a live readable 40-byte slot.
        unsafe { ptr::read((address + offset) as *const u8) == byte }
    })
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

    // both: preinstall A, poison old, install distinct B with both pointers; the returned old is
    // A before any second query, then a separate object queries B. This observes the combined
    // call's own old output instead of overwriting it with a later query (S710-REV-01).
    {
        let act_region = GuardRegion::new().ok_or("both-act-map")?;
        let old_region = GuardRegion::new().ok_or("both-old-map")?;
        let query_region = GuardRegion::new().ok_or("both-query-map")?;
        let at = act_region.last_wire_slot();
        let old = old_region.last_wire_slot();
        let query = query_region.last_wire_slot();
        let mut a = KernelSigAction::zero();
        a.handler = handler_address();
        a.mask = [MASK_A, 0];
        a.flags = SA_SIGINFO;
        write_request(at, a);
        expect("both-preinstall", sigaction(at, 0) == 0)?;
        fill(old, 0xa5, WIRE);
        let mut b = KernelSigAction::zero();
        b.handler = handler_b_address();
        b.mask = [MASK_B, 0];
        b.flags = SA_ONSTACK;
        write_request(at, b);
        expect("both-install", sigaction(at, old) == 0)?;
        let returned = read_request(old);
        expect(
            "both-old",
            returned.handler == a.handler
                && returned.mask == [MASK_A, 0]
                && returned.flags == a.flags
                && returned.restorer == 0,
        )?;
        expect("both-query", sigaction(0, query) == 0)?;
        let installed = read_request(query);
        expect(
            "both-installed",
            installed.handler == b.handler
                && installed.mask == [MASK_B, 0]
                && installed.flags == b.flags,
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

    // nonzero flags and mask round-trip; canaries prove the 40-byte old and query slots and the
    // surrounding bytes do not move. The combined call's old output is observed before the query.
    {
        let base = sys::map_anonymous(PAGE).map_err(|_| "canary-map")?;
        fill(base, CANARY, PAGE);
        let at = 64;
        let old = 512;
        let query = 960;
        let mut a = KernelSigAction::zero();
        a.handler = handler_address();
        a.mask = [MASK_A, 0];
        a.flags = SA_SIGINFO;
        write_request(base + at, a);
        expect("canary-preinstall", sigaction(base + at, 0) == 0)?;
        fill(base + old, 0xa5, WIRE);
        let mut b = KernelSigAction::zero();
        b.handler = handler_b_address();
        b.mask = [MASK_B, 0];
        b.flags = SA_SIGINFO | SA_ONSTACK | SA_RESTART;
        write_request(base + at, b);
        let install = sigaction(base + at, base + old);
        let returned = read_request(base + old);
        let query_result = sigaction(0, base + query);
        let installed = read_request(base + query);
        let intact = canaries_intact(
            base,
            &[(at, at + WIRE), (old, old + WIRE), (query, query + WIRE)],
        );
        let old_fields = returned.handler == a.handler
            && returned.mask == [MASK_A, 0]
            && returned.flags == a.flags
            && returned.restorer == 0;
        let new_fields = installed.handler == b.handler
            && installed.mask == [MASK_B, 0]
            && installed.flags & (SA_SIGINFO | SA_ONSTACK | SA_RESTART)
                == SA_SIGINFO | SA_ONSTACK | SA_RESTART
            && installed.restorer == 0;
        expect("canary-install", install == 0)?;
        expect("canary-old", old_fields)?;
        expect("canary-query", query_result == 0)?;
        expect("canary-data", intact)?;
        if !new_fields {
            println!(
                "signal-raw: canary values handler=0x{:x} mask=0x{:x},0x{:x} flags=0x{:x} restorer=0x{:x}",
                installed.handler,
                installed.mask[0],
                installed.mask[1],
                installed.flags,
                installed.restorer
            );
        }
        expect("canary-flags-mask", new_fields)?;
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

    // failure path: a rejected or unreadable act leaves old byte-identical (S710-REV-02).
    {
        let base = sys::map_anonymous(PAGE).map_err(|_| "fail-map")?;
        fill(base, CANARY, PAGE);
        let at = 64;
        let old = 512;
        fill(base + old, 0xa5, WIRE);
        let mut bad = KernelSigAction::zero();
        bad.handler = handler_address();
        bad.flags = 0x2000_0000;
        write_request(base + at, bad);
        expect("fail-flags", sigaction(base + at, base + old) == EINVAL)?;
        expect("fail-flags-old", request_is(base + old, 0xa5))?;
        let mut kill = KernelSigAction::zero();
        kill.handler = handler_address();
        write_request(base + at, kill);
        expect(
            "fail-kill",
            sigaction_signo(9, base + at, base + old) == EINVAL,
        )?;
        expect("fail-kill-old", request_is(base + old, 0xa5))?;
        expect(
            "fail-unmapped",
            sigaction(0x0000_0fff_ffff_f000, base + old) == EFAULT,
        )?;
        expect("fail-unmapped-old", request_is(base + old, 0xa5))?;
        let _ = sys::unmap(Handle::SELF_PROC, base, PAGE);
    }

    // alias: act and old are the same address; the input B is snapshotted before the reply A
    // overwrites it, and the installed action must be B.
    {
        let region = GuardRegion::new().ok_or("alias-map")?;
        let query_region = GuardRegion::new().ok_or("alias-query-map")?;
        let slot = region.last_wire_slot();
        let query = query_region.last_wire_slot();
        let mut a = KernelSigAction::zero();
        a.handler = handler_address();
        a.mask = [MASK_A, 0];
        a.flags = SA_SIGINFO;
        write_request(slot, a);
        expect("alias-preinstall", sigaction(slot, 0) == 0)?;
        let mut b = KernelSigAction::zero();
        b.handler = handler_b_address();
        b.mask = [MASK_B, 0];
        b.flags = SA_ONSTACK;
        write_request(slot, b);
        expect("alias-install", sigaction(slot, slot) == 0)?;
        let returned = read_request(slot);
        expect(
            "alias-old",
            returned.handler == a.handler
                && returned.mask == [MASK_A, 0]
                && returned.flags == a.flags,
        )?;
        expect("alias-query", sigaction(0, query) == 0)?;
        let installed = read_request(query);
        expect(
            "alias-installed",
            installed.handler == b.handler
                && installed.mask == [MASK_B, 0]
                && installed.flags == b.flags,
        )?;
    }

    // installed mask: the unmaskable SIGKILL/SIGSTOP bits are removed before storage (S710-REV-03).
    {
        let region = GuardRegion::new().ok_or("mask-map")?;
        let old_region = GuardRegion::new().ok_or("mask-old-map")?;
        let at = region.last_wire_slot();
        let old = old_region.last_wire_slot();
        let mut action = KernelSigAction::zero();
        action.handler = handler_address();
        action.mask = [(1 << 8) | (1 << 18) | MASK_A, 0];
        action.flags = SA_SIGINFO;
        write_request(at, action);
        expect("mask-install", sigaction(at, 0) == 0)?;
        expect("mask-query", sigaction(0, old) == 0)?;
        let installed = read_request(old);
        expect("mask-sanitized", installed.mask == [MASK_A, 0])?;
    }

    Ok(())
}
