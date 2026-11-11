// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fault-safe copies between kernel buffers and user addresses.
//!
//! No user pointer is dereferenced in Rust. The byte copies are assembly routines bracketed by a
//! fixup table; if the exception handler sees a faulting instruction inside one of those brackets
//! and a user fault address, it redirects the instruction pointer to the fixup label, which
//! returns `-EFAULT` to the Rust wrapper.
//!
//! * x86-64 uses `rep movsb`; a missing user page raises a page fault that the trap handler fixes
//!   up.
//! * aarch64 uses the unprivileged `ldtrb`/`sttrb` accesses. They are subject to the EL0
//!   permission checks regardless of `PAN`/`SCTLR_EL1.SPAN`, so usercopy needs neither `stac`/
//!   `clac` nor a PAN toggle: a bad user page raises a same-EL data abort that is fixed up.

use raven_abi::errno::Errno;
use raven_usercopy::{UserSlice, is_user_address};

/// Length of one fixup table entry: `(start_ip, end_ip, fixup_ip)`.
const FIXUP_ENTRY_BYTES: u64 = 24;

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::vectors::TrapFrame;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::trapframe::TrapFrame;

unsafe extern "C" {
    fn copy_from_user_asm(destination: *mut u8, source: *const u8, length: usize) -> isize;
    fn copy_to_user_asm(destination: *mut u8, source: *const u8, length: usize) -> isize;

    static __usercopy_fixup_start: u8;
    static __usercopy_fixup_end: u8;
}

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global copy_from_user_asm
    .type copy_from_user_asm, @function
copy_from_user_asm:
    cld
    mov rcx, rdx
copy_from_user_start:
    rep movsb
copy_from_user_end:
    xor eax, eax
    ret
copy_from_user_fault:
    mov rax, -14
    ret
    .size copy_from_user_asm, . - copy_from_user_asm

    .global copy_to_user_asm
    .type copy_to_user_asm, @function
copy_to_user_asm:
    cld
    mov rcx, rdx
copy_to_user_start:
    rep movsb
copy_to_user_end:
    xor eax, eax
    ret
copy_to_user_fault:
    mov rax, -14
    ret
    .size copy_to_user_asm, . - copy_to_user_asm

    .section .rodata.usercopy_fixup, "a"
    .global __usercopy_fixup_start
__usercopy_fixup_start:
    .quad copy_from_user_start
    .quad copy_from_user_end
    .quad copy_from_user_fault
    .quad copy_to_user_start
    .quad copy_to_user_end
    .quad copy_to_user_fault
    .global __usercopy_fixup_end
__usercopy_fixup_end:
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text

    /* x0 = kernel destination, x1 = user source, x2 = length. Only the `ldtrb` can fault. */
    .global copy_from_user_asm
    .type copy_from_user_asm, %function
copy_from_user_asm:
    mov x9, #0
1:
    cmp x9, x2
    b.hs 3f
    add x11, x1, x9
copy_from_user_start:
    ldtrb w10, [x11]
copy_from_user_end:
    strb w10, [x0, x9]
    add x9, x9, #1
    b 1b
3:
    mov x0, #0
    ret
copy_from_user_fault:
    mov x0, #14
    neg x0, x0
    ret
    .size copy_from_user_asm, . - copy_from_user_asm

    /* x0 = user destination, x1 = kernel source, x2 = length. Only the `sttrb` can fault. */
    .global copy_to_user_asm
    .type copy_to_user_asm, %function
copy_to_user_asm:
    mov x9, #0
1:
    cmp x9, x2
    b.hs 3f
    ldrb w10, [x1, x9]
    add x11, x0, x9
copy_to_user_start:
    sttrb w10, [x11]
copy_to_user_end:
    add x9, x9, #1
    b 1b
3:
    mov x0, #0
    ret
copy_to_user_fault:
    mov x0, #14
    neg x0, x0
    ret
    .size copy_to_user_asm, . - copy_to_user_asm

    .section .rodata.usercopy_fixup, "a"
    .global __usercopy_fixup_start
__usercopy_fixup_start:
    .quad copy_from_user_start
    .quad copy_from_user_end
    .quad copy_from_user_fault
    .quad copy_to_user_start
    .quad copy_to_user_end
    .quad copy_to_user_fault
    .global __usercopy_fixup_end
__usercopy_fixup_end:
"#
);

/// Copy user bytes into a kernel buffer.
pub fn copy_from_user(destination: &mut [u8], source: UserSlice) -> Result<(), Errno> {
    let length = usize::try_from(source.length()).map_err(|_| Errno::EFAULT)?;
    if length > destination.len() {
        return Err(Errno::EINVAL);
    }
    if length == 0 {
        return Ok(());
    }
    // SAFETY: the range was validated by `UserSlice`; the assembly routine can fault on a missing
    // user page, in which case the fixup table redirects it to `copy_from_user_fault`.
    let result = unsafe {
        copy_from_user_asm(
            destination.as_mut_ptr(),
            source.address() as *const u8,
            length,
        )
    };
    if result < 0 {
        Err(Errno::EFAULT)
    } else {
        Ok(())
    }
}

/// Copy kernel bytes into a user address.
pub fn copy_to_user(destination: UserSlice, source: &[u8]) -> Result<(), Errno> {
    if u64::try_from(source.len()).map_err(|_| Errno::EINVAL)? > destination.length() {
        return Err(Errno::EINVAL);
    }
    if source.is_empty() {
        return Ok(());
    }
    // SAFETY: the destination range was validated by `UserSlice`; the write can fault and is
    // redirected by the fixup table to `copy_to_user_fault`.
    let result = unsafe {
        copy_to_user_asm(
            destination.address() as *mut u8,
            source.as_ptr(),
            source.len(),
        )
    };
    if result < 0 {
        Err(Errno::EFAULT)
    } else {
        Ok(())
    }
}

/// Copy a NUL-terminated user string into `buffer`, returning the UTF-8 view.
#[allow(dead_code)] // Path-taking syscalls added after S08 use this helper.
pub fn copy_cstr_from_user(address: u64, buffer: &mut [u8]) -> Result<&str, Errno> {
    if buffer.is_empty() {
        return Err(Errno::EINVAL);
    }
    let mut index = 0usize;
    while index < buffer.len() {
        let slice = UserSlice::new(address.checked_add(index as u64).ok_or(Errno::EFAULT)?, 1)?;
        let Some(byte) = buffer.get_mut(index) else {
            return Err(Errno::EFAULT);
        };
        copy_from_user(core::slice::from_mut(byte), slice)?;
        if *byte == 0 {
            let bytes = buffer.get(..index).ok_or(Errno::EFAULT)?;
            return core::str::from_utf8(bytes).map_err(|_| Errno::EINVAL);
        }
        index = index.saturating_add(1);
    }
    Err(Errno::ENAMETOOLONG)
}

/// Direction of the user access guarded by one fixup entry.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    /// The kernel reads from the user (`copy_from_user`).
    Read,
    /// The kernel writes to the user (`copy_to_user`).
    Write,
}

/// Return the fixup address and access direction when `return_address` is inside a usercopy
/// critical section.
///
/// The table is emitted in the order `copy_from_user`, `copy_to_user`; the parity of the entry
/// index therefore selects the direction.
fn fixup_for(return_address: u64) -> Option<(u64, Access)> {
    let start = core::ptr::addr_of!(__usercopy_fixup_start) as u64;
    let end = core::ptr::addr_of!(__usercopy_fixup_end) as u64;
    if end <= start {
        return None;
    }
    let count = end.saturating_sub(start) / FIXUP_ENTRY_BYTES;
    let mut index = 0u64;
    while index < count {
        let entry = start.saturating_add(index.saturating_mul(FIXUP_ENTRY_BYTES));
        // SAFETY: the linker-emitted table is an array of three u64 values per entry; the index is
        // bounded by the table size computed from its start and end symbols.
        let (entry_start, entry_end, fixup) = unsafe {
            (
                core::ptr::read_unaligned(entry as *const u64),
                core::ptr::read_unaligned(entry.saturating_add(8) as *const u64),
                core::ptr::read_unaligned(entry.saturating_add(16) as *const u64),
            )
        };
        if return_address >= entry_start && return_address < entry_end {
            let access = if index.is_multiple_of(2) {
                Access::Read
            } else {
                Access::Write
            };
            return Some((fixup, access));
        }
        index = index.saturating_add(1);
    }
    None
}

/// Handle a page fault that happened inside a usercopy critical section.
///
/// A write into a private copy-on-write mapping is logically writable: resolve the COW and let
/// the faulting instruction retry.  A read fault or a genuine permission violation is redirected
/// to the fixup label, which returns `EFAULT`.  Kernel-origin faults never reach this function
/// (the caller only calls it for a user address inside a usercopy range).
///
/// Returns true when the frame was either retried or moved to the fixup label.
#[cfg(target_arch = "x86_64")]
pub fn handle_page_fault(frame: &mut TrapFrame, cr2: u64) -> bool {
    if !is_user_address(cr2) {
        return false;
    }
    let Some((fixup, access)) = fixup_for(frame.rip) else {
        return false;
    };
    if access == Access::Write && crate::process::handle_page_fault(cr2) {
        // RIP is unchanged: the store/rep movsb retries against the now-private page.
        return true;
    }
    frame.rip = fixup;
    true
}

/// Handle an aarch64 same-EL data abort inside a usercopy critical section.
///
/// Same policy as the x86-64 handler: resolve COW for a write, otherwise redirect to the fixup
/// label so the routine delivers `-EFAULT` to its Rust wrapper.
#[cfg(target_arch = "aarch64")]
pub fn handle_fault_aarch64(frame: &mut TrapFrame) -> bool {
    if !is_user_address(frame.far) {
        return false;
    }
    let Some((fixup, access)) = fixup_for(frame.elr) else {
        return false;
    };
    if access == Access::Write && crate::process::handle_page_fault(frame.far) {
        return true;
    }
    frame.elr = fixup;
    true
}

const _: () = assert!(FIXUP_ENTRY_BYTES == 24);
