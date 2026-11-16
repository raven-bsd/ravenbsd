// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `clone` framing: flag validation, the child Linux frame and the x86-64 trampoline page.
//!
//! B21a makes the kernel deliver the saved user frame beside each trap. A `clone` handler queues
//! the frame and the six Linux arguments; the trap loop builds a page containing a small
//! RIP-relative trampoline and a copy of the child frame, maps it executable into the target, and
//! creates a native thread at the trampoline. The trampoline loads every general register, sets
//! `rsp`, and jumps to the parent's return RIP without touching the stack, so the child resumes
//! exactly as if the syscall had returned `0` there.

use raven_abi::errno::Errno;
use raven_linux_abi::flags::clone as flags;
use raven_proto_linux::{FRAME_GPRS, LinuxFrame};

use crate::table::{CloneRequest, PAGE};

/// Signal mask carried in the low byte of the clone flags.
const CSIGNAL: u64 = 0xff;

/// Flags this server implements. Anything else is `EINVAL`, as on Linux.
pub const SUPPORTED: u64 = CSIGNAL
    | flags::CLONE_VM
    | flags::CLONE_FS
    | flags::CLONE_FILES
    | flags::CLONE_SIGHAND
    | flags::CLONE_THREAD
    | flags::CLONE_SYSVSEM
    | flags::CLONE_SETTLS
    | flags::CLONE_PARENT_SETTID
    | flags::CLONE_CHILD_CLEARTID
    | flags::CLONE_CHILD_SETTID;

/// Whether a validated clone creates a thread in this process or a forked child process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneKind {
    /// `CLONE_VM|CLONE_THREAD`: a native thread sharing the address space.
    Thread,
    /// A fork-like clone: a native `PROC_FORK` child inheriting a copy of the address space.
    Fork,
}

/// Validate the flag word and stack argument.
///
/// Linux rejects `CLONE_THREAD` without `CLONE_VM`/`CLONE_SIGHAND`, a sharing flag without
/// `CLONE_VM`, a zero or misaligned thread stack, and any unknown bit. Fork-like clones get a COW
/// copy of the address space; the child-set-tid write has no server path into that copy, so that
/// combination is rejected instead of faked.
pub fn classify(request: &CloneRequest) -> Result<CloneKind, Errno> {
    let flags = request.flags;
    if flags & CSIGNAL > 64 || flags & !SUPPORTED != 0 {
        return Err(Errno::EINVAL);
    }
    let shares_vm = flags & flags::CLONE_VM != 0;
    let thread = flags & flags::CLONE_THREAD != 0;
    if thread {
        if !shares_vm || flags & flags::CLONE_SIGHAND == 0 {
            return Err(Errno::EINVAL);
        }
        if request.child_stack == 0 || !request.child_stack.is_multiple_of(16) {
            return Err(Errno::EINVAL);
        }
        return Ok(CloneKind::Thread);
    }
    if shares_vm
        || flags
            & (flags::CLONE_SIGHAND | flags::CLONE_FS | flags::CLONE_FILES | flags::CLONE_SYSVSEM)
            != 0
    {
        return Err(Errno::EINVAL);
    }
    if flags & flags::CLONE_CHILD_SETTID != 0 {
        return Err(Errno::EINVAL);
    }
    if request.child_stack != 0 && !request.child_stack.is_multiple_of(16) {
        return Err(Errno::EINVAL);
    }
    Ok(CloneKind::Fork)
}

/// Build the frame the child resumes from: the parent's registers with `rax = 0` and the child
/// stack. A fork-like clone with no explicit stack keeps the captured (COW-copied) parent stack.
pub fn child_frame(request: &CloneRequest, kind: CloneKind) -> Result<LinuxFrame, Errno> {
    let mut frame = request.frame;
    if frame.rip == 0 || frame.rsp == 0 {
        return Err(Errno::EINVAL);
    }
    frame.gprs[0] = 0;
    match kind {
        CloneKind::Thread => {
            if request.child_stack == 0 {
                return Err(Errno::EINVAL);
            }
            frame.rsp = request.child_stack;
        }
        CloneKind::Fork => {
            if request.child_stack != 0 {
                frame.rsp = request.child_stack;
            }
        }
    }
    Ok(frame)
}

/// Byte offset of the saved frame inside the trampoline page.
pub const FRAME_OFFSET: usize = 0x100;
/// Number of qwords stored beside the trampoline: the general registers, RSP and RIP.
pub const FRAME_WORDS: usize = FRAME_GPRS + 2;
/// Index of RSP in the saved frame table.
pub const RSP_INDEX: usize = FRAME_GPRS;
/// Index of RIP in the saved frame table.
pub const RIP_INDEX: usize = FRAME_GPRS + 1;
/// Length of the generated code before the frame table.
pub const CODE_LEN: usize = FRAME_GPRS * 7 + 7 + 6;

/// `mov r64, [rip+disp32]` prefixes and ModRM bytes, in `LinuxFrame::gprs` order.
const GPR_MOVS: [(u8, u8); FRAME_GPRS] = [
    (0x48, 0x05), // rax
    (0x48, 0x1d), // rbx
    (0x48, 0x0d), // rcx
    (0x48, 0x15), // rdx
    (0x48, 0x35), // rsi
    (0x48, 0x3d), // rdi
    (0x48, 0x2d), // rbp
    (0x4c, 0x05), // r8
    (0x4c, 0x0d), // r9
    (0x4c, 0x15), // r10
    (0x4c, 0x1d), // r11
    (0x4c, 0x25), // r12
    (0x4c, 0x2d), // r13
    (0x4c, 0x35), // r14
    (0x4c, 0x3d), // r15
];

/// Build one executable page: trampoline code plus the saved child frame.
///
/// The code uses only RIP-relative loads, so it never depends on its initial registers or stack.
/// It ends with `jmp qword ptr [rip+disp32]` rather than push/ret, which preserves the eight bytes
/// below the child's RSP (the SysV red zone).
#[must_use]
pub fn trampoline_page(frame: &LinuxFrame) -> [u8; PAGE as usize] {
    let mut page = [0u8; PAGE as usize];
    let mut position = 0usize;
    for (index, (rex, modrm)) in GPR_MOVS.iter().enumerate() {
        let target = FRAME_OFFSET + index * 8;
        let disp = displacement(target, position + 7);
        page[position] = *rex;
        page[position + 1] = 0x8b;
        page[position + 2] = *modrm;
        page[position + 3..position + 7].copy_from_slice(&disp.to_le_bytes());
        position += 7;
    }
    // mov rsp, [rip+disp32]
    let disp = displacement(FRAME_OFFSET + RSP_INDEX * 8, position + 7);
    page[position..position + 3].copy_from_slice(&[0x48, 0x8b, 0x25]);
    page[position + 3..position + 7].copy_from_slice(&disp.to_le_bytes());
    position += 7;
    // jmp qword ptr [rip+disp32]
    let disp = displacement(FRAME_OFFSET + RIP_INDEX * 8, position + 6);
    page[position..position + 2].copy_from_slice(&[0xff, 0x25]);
    page[position + 2..position + 6].copy_from_slice(&disp.to_le_bytes());
    position += 6;
    debug_assert_eq!(position, CODE_LEN);

    let mut words = [0u64; FRAME_WORDS];
    words[..FRAME_GPRS].copy_from_slice(&frame.gprs);
    words[RSP_INDEX] = frame.rsp;
    words[RIP_INDEX] = frame.rip;
    for (index, word) in words.iter().enumerate() {
        let offset = FRAME_OFFSET + index * 8;
        page[offset..offset + 8].copy_from_slice(&word.to_le_bytes());
    }
    page
}

fn displacement(target: usize, next_instruction: usize) -> i32 {
    i32::try_from(target as i64 - next_instruction as i64).unwrap_or(0)
}

#[cfg(test)]
#[path = "clone_ut.rs"]
mod tests;
