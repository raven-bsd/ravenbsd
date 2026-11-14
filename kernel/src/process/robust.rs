// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-thread robust futex lists (PG-5).
//!
//! `ROBUST_LIST_SET` registers a Linux `robust_list_head` per thread. At thread
//! exit, process exit and `PROC_TERMINATE` the kernel walks the circular list
//! through usercopy, bounded to 2048 entries: it sets `OWNER_DIED`, clears the
//! owner tid and wakes one waiter on the futex word. A fault is reported to the
//! caller; it never reads kernel memory and never panics.

use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::futex;
use raven_usercopy::{UserSlice, is_user_address};
use spin::Mutex;

use crate::process::{self, Process};
use crate::sched;
use crate::usercopy::{copy_from_user, copy_to_user};

/// Linux `struct robust_list_head` size on the 64-bit ABI.
pub const HEAD_SIZE: usize = 24;
/// Maximum entries examined by one walk.
pub const MAX_ENTRIES: usize = 2048;

struct Entry {
    process_id: u64,
    thread_id: u64,
    head: u64,
    len: u64,
}

static LISTS: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

/// `ROBUST_LIST_SET(head, len)` for the calling thread.
pub fn set(head: u64, len: u64) -> Result<(), Errno> {
    let thread = sched::current_thread().ok_or(Errno::ESRCH)?;
    let process = process::current().ok_or(Errno::ESRCH)?;
    if head == 0 {
        if len != 0 {
            return Err(Errno::EINVAL);
        }
        remove_thread(thread.id);
        return Ok(());
    }
    if len != HEAD_SIZE as u64 || head & 7 != 0 || !is_user_address(head) {
        return Err(Errno::EINVAL);
    }
    UserSlice::new(head, HEAD_SIZE as u64).map_err(|_| Errno::EINVAL)?;
    let mut lists = LISTS.lock();
    lists.retain(|entry| entry.thread_id != thread.id);
    lists.push(Entry {
        process_id: process.id,
        thread_id: thread.id,
        head,
        len,
    });
    Ok(())
}

fn remove_thread(thread_id: u64) {
    LISTS.lock().retain(|entry| entry.thread_id != thread_id);
}

fn take_thread(thread_id: u64) -> Option<Entry> {
    let mut lists = LISTS.lock();
    lists
        .iter()
        .position(|entry| entry.thread_id == thread_id)
        .map(|index| lists.remove(index))
}

fn take_process(process_id: u64) -> Option<Entry> {
    let mut lists = LISTS.lock();
    lists
        .iter()
        .position(|entry| entry.process_id == process_id)
        .map(|index| lists.remove(index))
}

/// Walk the calling thread's robust list, then drop its registration.
pub fn walk_thread() -> Result<(), Errno> {
    let Some(thread) = sched::current_thread() else {
        return Ok(());
    };
    let Some(entry) = take_thread(thread.id) else {
        return Ok(());
    };
    let process = process::current().ok_or(Errno::ESRCH)?;
    walk_entry(entry.head, entry.len, &process)
}

/// Walk every registered robust list of `process` (process termination).
///
/// The target address space is activated for the duration of each walk; the
/// kernel stack and code are mapped in every user root. Faults stop the walk
/// and are ignored because the process is already terminal.
pub fn walk_process(process: &Arc<Process>) {
    while let Some(entry) = take_process(process.id) {
        let root = process
            .address_space
            .lock()
            .as_ref()
            .map(|space| space.root().0);
        let Some(root) = root else {
            continue;
        };
        with_address_space(root, || {
            let _ = walk_entry(entry.head, entry.len, process);
        });
    }
}

fn walk_entry(head: u64, _len: u64, process: &Arc<Process>) -> Result<(), Errno> {
    let offset = read_i64(head.checked_add(8).ok_or(Errno::EFAULT)?)?;
    let mut next = read_u64(head)?;
    let mut count = 0usize;
    while next != head && count < MAX_ENTRIES {
        if next == 0 || next & 7 != 0 || !is_user_address(next) {
            return Err(Errno::EINVAL);
        }
        let following = read_u64(next)?;
        let address = (next as i64).checked_add(offset).ok_or(Errno::EFAULT)? as u64;
        if address & 3 != 0 || !is_user_address(address) {
            return Err(Errno::EINVAL);
        }
        let word = read_u32(address)?;
        let word = (word & !futex::OWNER_TID) | futex::OWNER_DIED;
        write_u32(address, word)?;
        let _ = crate::syscall::futex::wake_process_address(process, address, 1);
        next = following;
        count = count.saturating_add(1);
    }
    Ok(())
}

fn read_u64(address: u64) -> Result<u64, Errno> {
    let mut bytes = [0u8; 8];
    copy_from_user(&mut bytes, UserSlice::new(address, 8)?)?;
    Ok(u64::from_ne_bytes(bytes))
}

fn read_i64(address: u64) -> Result<i64, Errno> {
    let mut bytes = [0u8; 8];
    copy_from_user(&mut bytes, UserSlice::new(address, 8)?)?;
    Ok(i64::from_ne_bytes(bytes))
}

fn read_u32(address: u64) -> Result<u32, Errno> {
    let mut bytes = [0u8; 4];
    copy_from_user(&mut bytes, UserSlice::new(address, 4)?)?;
    Ok(u32::from_ne_bytes(bytes))
}

fn write_u32(address: u64, value: u32) -> Result<(), Errno> {
    copy_to_user(UserSlice::new(address, 4)?, &value.to_ne_bytes())
}

#[cfg(target_arch = "x86_64")]
fn with_address_space<R>(root: u64, function: impl FnOnce() -> R) -> R {
    let _guard = crate::sync::critical();
    let saved: u64;
    // SAFETY: CR3 selects another process's user root; kernel mappings are shared in the upper
    // half, the current kernel stack is mapped there, and the target is not running user code.
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) saved, options(nomem, nostack, preserves_flags));
        core::arch::asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags));
    }
    let result = function();
    // SAFETY: restoring the saved root returns to the caller's address space.
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) saved, options(nostack, preserves_flags));
    }
    result
}

#[cfg(target_arch = "aarch64")]
fn with_address_space<R>(root: u64, function: impl FnOnce() -> R) -> R {
    let _guard = crate::sync::critical();
    let saved: u64;
    // SAFETY: TTBR0 selects another process's user root; TTBR1 (kernel) is unchanged and the
    // target is not running user code.
    unsafe {
        core::arch::asm!("mrs {}, ttbr0_el1", out(reg) saved, options(nomem, nostack, preserves_flags));
    }
    crate::arch::mmu::switch_ttbr0(root, 0);
    let result = function();
    crate::arch::mmu::switch_ttbr0(saved, ((saved >> 48) & 0xff) as u16);
    result
}

const _: () = assert!(HEAD_SIZE == 24);
const _: () = assert!(MAX_ENTRIES == 2048);
