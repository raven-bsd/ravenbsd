// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 `rt_sigframe` construction and restoration.
//!
//! The layout follows the documented Linux x86-64 ABI: the restorer return address at `sp`,
//! followed by `ucontext`, followed by `siginfo`, with the floating-point area above them and
//! the 128-byte red zone above the frame. Field offsets are covered by tests against
//! `linux-raw-sys`.

use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_linux_abi::structs::x86_64::SigAction;

const FRAME_SIZE: usize = 440;
const SIGINFO_OFFSET: usize = 312;
const UCONTEXT_OFFSET: usize = 8;
const MCONTEXT_OFFSET: usize = 48;
const UCONTEXT_MASK_OFFSET: usize = 304;
const RED_ZONE: usize = 128;
const FP_ALIGN: usize = 64;

/// Total size of the restorer + `ucontext` + `siginfo` frame.
pub const FRAME_BYTES: usize = FRAME_SIZE;
/// Offset of `siginfo` inside the frame.
pub const SIGINFO_AT: usize = SIGINFO_OFFSET;
/// Offset of `ucontext` inside the frame.
pub const UCONTEXT_AT: usize = UCONTEXT_OFFSET;
/// Offset of `uc_mcontext` inside the frame.
pub const MCONTEXT_AT: usize = MCONTEXT_OFFSET;
/// Offset of `uc_sigmask` inside the frame.
pub const UCONTEXT_MASK_AT: usize = UCONTEXT_MASK_OFFSET;

/// The 128-byte `siginfo_t` payload written into the frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SigInfo {
    /// Signal number.
    pub si_signo: i32,
    /// Error number.
    pub si_errno: i32,
    /// Signal code.
    pub si_code: i32,
    /// Alignment padding.
    pub _pad: i32,
    /// Signal-specific payload.
    pub _sifields: [u8; 112],
    __align: [u64; 0],
}

impl Default for SigInfo {
    fn default() -> Self {
        Self::new(0, 0, 0, [0; 112])
    }
}

impl SigInfo {
    /// Build a `siginfo_t` from its public fields.
    pub fn new(si_signo: i32, si_errno: i32, si_code: i32, fields: [u8; 112]) -> Self {
        Self {
            si_signo,
            si_errno,
            si_code,
            _pad: 0,
            _sifields: fields,
            __align: [],
        }
    }
}

const _: () = assert!(core::mem::size_of::<SigInfo>() == 128);
const _: () = assert!(core::mem::align_of::<SigInfo>() == 8);

/// `SA_ONSTACK` from the Linux UAPI.
pub const SA_ONSTACK: u64 = 0x0800_0000;
/// `SA_RESTORER` from the Linux UAPI.
pub const SA_RESTORER: u64 = 0x0400_0000;
/// `SS_DISABLE` for an alternate signal stack.
pub const SS_DISABLE: i32 = 2;

/// General-purpose register state saved in a signal frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Regs {
    /// r8.
    pub r8: u64,
    /// r9.
    pub r9: u64,
    /// r10.
    pub r10: u64,
    /// r11.
    pub r11: u64,
    /// r12.
    pub r12: u64,
    /// r13.
    pub r13: u64,
    /// r14.
    pub r14: u64,
    /// r15.
    pub r15: u64,
    /// rdi.
    pub rdi: u64,
    /// rsi.
    pub rsi: u64,
    /// rbp.
    pub rbp: u64,
    /// rbx.
    pub rbx: u64,
    /// rdx.
    pub rdx: u64,
    /// rax.
    pub rax: u64,
    /// rcx.
    pub rcx: u64,
    /// rsp.
    pub rsp: u64,
    /// rip.
    pub rip: u64,
    /// rflags.
    pub rflags: u64,
    /// cs.
    pub cs: u16,
    /// gs.
    pub gs: u16,
    /// fs.
    pub fs: u16,
    /// ss (stored in the x86-64 `__pad0` slot).
    pub ss: u16,
    /// error code.
    pub err: u64,
    /// trap number.
    pub trapno: u64,
    /// old signal mask in the sigcontext.
    pub oldmask: u64,
    /// cr2.
    pub cr2: u64,
}

/// Alternate signal stack (`stack_t`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AltStack {
    /// Stack base.
    pub sp: usize,
    /// `SS_DISABLE`/`SS_ONSTACK` flags.
    pub flags: i32,
    /// Stack size.
    pub size: usize,
}

/// A constructed signal frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Frame bytes starting at [`Frame::new_sp`].
    pub bytes: Vec<u8>,
    /// Address of the restorer return address; `(new_sp + 8) % 16 == 0`.
    pub new_sp: usize,
    /// Handler entry point.
    pub handler_rip: usize,
}

/// Build an x86-64 `rt_sigframe`.
///
/// `user_sp` is the interrupted user stack pointer. When `SA_ONSTACK` is set and a usable
/// alternate stack is supplied, its top is used instead. The frame is placed below the 128-byte
/// red zone; `fpstate_len` bytes are reserved above it for the floating-point state.
#[allow(clippy::too_many_arguments)] // Mirrors the documented rt_sigframe inputs.
pub fn build_rt_sigframe(
    regs: &Regs,
    sig: u32,
    siginfo: &SigInfo,
    action: &SigAction,
    old_mask: u64,
    altstack: Option<AltStack>,
    user_sp: usize,
    fpstate_len: usize,
) -> Frame {
    let usable_altstack = altstack.filter(|stack| stack.flags & SS_DISABLE == 0);
    let selected_top = if action.sa_flags & SA_ONSTACK != 0 {
        usable_altstack
            .and_then(|stack| stack.sp.checked_add(stack.size))
            .unwrap_or(user_sp)
    } else {
        user_sp
    };
    let stack_end = selected_top.saturating_sub(RED_ZONE);
    let fpstate_addr = if fpstate_len == 0 {
        0
    } else {
        align_down(stack_end.saturating_sub(fpstate_len), FP_ALIGN).unwrap_or(0)
    };
    let frame_bottom = if fpstate_addr == 0 {
        stack_end.saturating_sub(FRAME_SIZE)
    } else {
        fpstate_addr.saturating_sub(FRAME_SIZE)
    };
    let new_sp = align_down(frame_bottom, 16).unwrap_or(0).saturating_sub(8);

    let mut bytes = vec![0u8; FRAME_SIZE];
    put_u64(&mut bytes, 0, action.sa_restorer as u64);

    put_u64(&mut bytes, UCONTEXT_OFFSET, 0);
    put_u64(&mut bytes, UCONTEXT_OFFSET + 8, 0);
    let stack = altstack.unwrap_or(AltStack {
        sp: 0,
        flags: SS_DISABLE,
        size: 0,
    });
    put_u64(&mut bytes, UCONTEXT_OFFSET + 16, stack.sp as u64);
    put_u32(&mut bytes, UCONTEXT_OFFSET + 24, stack.flags as u32);
    put_u64(&mut bytes, UCONTEXT_OFFSET + 32, stack.size as u64);

    put_regs(&mut bytes, regs);
    put_u64(&mut bytes, MCONTEXT_OFFSET + 168, old_mask);
    put_u64(&mut bytes, MCONTEXT_OFFSET + 184, fpstate_addr as u64);
    put_u64(&mut bytes, UCONTEXT_MASK_OFFSET, old_mask);

    put_u32(&mut bytes, SIGINFO_OFFSET, sig);
    put_u32(&mut bytes, SIGINFO_OFFSET + 4, siginfo.si_errno as u32);
    put_u32(&mut bytes, SIGINFO_OFFSET + 8, siginfo.si_code as u32);
    bytes[SIGINFO_OFFSET + 16..SIGINFO_OFFSET + 128].copy_from_slice(&siginfo._sifields);

    Frame {
        bytes,
        new_sp,
        handler_rip: action.sa_handler,
    }
}

/// Restore registers and the signal mask from an x86-64 `rt_sigframe`.
pub fn restore_rt_sigframe(bytes: &[u8]) -> Result<(Regs, u64), Errno> {
    if bytes.len() < FRAME_SIZE {
        return Err(Errno::EINVAL);
    }
    let regs = get_regs(bytes)?;
    let mask = get_u64(bytes, UCONTEXT_MASK_OFFSET)?;
    Ok((regs, mask))
}

fn put_regs(bytes: &mut [u8], regs: &Regs) {
    for (index, value) in [
        regs.r8,
        regs.r9,
        regs.r10,
        regs.r11,
        regs.r12,
        regs.r13,
        regs.r14,
        regs.r15,
        regs.rdi,
        regs.rsi,
        regs.rbp,
        regs.rbx,
        regs.rdx,
        regs.rax,
        regs.rcx,
        regs.rsp,
        regs.rip,
        regs.rflags,
    ]
    .iter()
    .enumerate()
    {
        put_u64(bytes, MCONTEXT_OFFSET + index * 8, *value);
    }
    put_u16(bytes, MCONTEXT_OFFSET + 144, regs.cs);
    put_u16(bytes, MCONTEXT_OFFSET + 146, regs.gs);
    put_u16(bytes, MCONTEXT_OFFSET + 148, regs.fs);
    put_u16(bytes, MCONTEXT_OFFSET + 150, regs.ss);
    put_u64(bytes, MCONTEXT_OFFSET + 152, regs.err);
    put_u64(bytes, MCONTEXT_OFFSET + 160, regs.trapno);
    put_u64(bytes, MCONTEXT_OFFSET + 168, regs.oldmask);
    put_u64(bytes, MCONTEXT_OFFSET + 176, regs.cr2);
}

fn get_regs(bytes: &[u8]) -> Result<Regs, Errno> {
    let mut values = [0u64; 18];
    for (index, value) in values.iter_mut().enumerate() {
        *value = get_u64(bytes, MCONTEXT_OFFSET + index * 8)?;
    }
    Ok(Regs {
        r8: values[0],
        r9: values[1],
        r10: values[2],
        r11: values[3],
        r12: values[4],
        r13: values[5],
        r14: values[6],
        r15: values[7],
        rdi: values[8],
        rsi: values[9],
        rbp: values[10],
        rbx: values[11],
        rdx: values[12],
        rax: values[13],
        rcx: values[14],
        rsp: values[15],
        rip: values[16],
        rflags: values[17],
        cs: get_u16(bytes, MCONTEXT_OFFSET + 144)?,
        gs: get_u16(bytes, MCONTEXT_OFFSET + 146)?,
        fs: get_u16(bytes, MCONTEXT_OFFSET + 148)?,
        ss: get_u16(bytes, MCONTEXT_OFFSET + 150)?,
        err: get_u64(bytes, MCONTEXT_OFFSET + 152)?,
        trapno: get_u64(bytes, MCONTEXT_OFFSET + 160)?,
        oldmask: get_u64(bytes, MCONTEXT_OFFSET + 168)?,
        cr2: get_u64(bytes, MCONTEXT_OFFSET + 176)?,
    })
}

fn align_down(value: usize, alignment: usize) -> Option<usize> {
    value.checked_sub(value % alignment)
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    let end = offset + 2;
    bytes[offset..end].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    let end = offset + 4;
    bytes[offset..end].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    let end = offset + 8;
    bytes[offset..end].copy_from_slice(&value.to_le_bytes());
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, Errno> {
    let end = offset.checked_add(2).ok_or(Errno::EINVAL)?;
    let slice = bytes.get(offset..end).ok_or(Errno::EINVAL)?;
    let array: [u8; 2] = slice.try_into().map_err(|_| Errno::EINVAL)?;
    Ok(u16::from_le_bytes(array))
}

fn get_u64(bytes: &[u8], offset: usize) -> Result<u64, Errno> {
    let end = offset.checked_add(8).ok_or(Errno::EINVAL)?;
    let slice = bytes.get(offset..end).ok_or(Errno::EINVAL)?;
    let array: [u8; 8] = slice.try_into().map_err(|_| Errno::EINVAL)?;
    Ok(u64::from_le_bytes(array))
}
