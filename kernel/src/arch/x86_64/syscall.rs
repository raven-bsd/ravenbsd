// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `syscall` MSR setup and the assembly entry stub.

use core::mem::size_of;

use raven_abi::errno::Errno;
use raven_abi::signal::{self, ENTRY_SP, FaultRecord, MContext};
use raven_usercopy::{UserSlice, is_user_address};

use crate::arch::x86_64::gdt::{KERNEL_CS, KERNEL_SS, STAR_USER_BASE, USER_CS, USER_SS};
use crate::arch::x86_64::trapframe::TrapFrame;
use crate::arch::x86_64::usermode::{percpu_address, percpu_init};
use crate::process::{self, PendingSignal, SIG_DFL, SIG_IGN, SigAction};
use crate::syscall::thread;
use crate::usercopy::{copy_from_user, copy_to_user};

const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;
const IA32_GS_BASE: u32 = 0xC000_0101;
const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

/// Clear IF, TF, DF and AC on syscall entry.
const SYSCALL_FMASK: u64 = 0x0000_0000_0004_0700;

core::arch::global_asm!(
    r#"
    .section .text
    .global syscall_entry
    .type syscall_entry, @function
syscall_entry:
    swapgs
    mov qword ptr gs:[16], rsp
    mov rsp, qword ptr gs:[8]

    push 0x1b
    push qword ptr gs:[16]
    push r11
    push 0x23
    push rcx
    push 0
    push 0

    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, rsp
    call sched_syscall_enter
    mov rdi, rsp
    call syscall_dispatch
    call sched_syscall_exit

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    add rsp, 16

    mov rcx, [rsp]
    mov r11, [rsp + 16]

    /* Use r10 as the canonical-rip scratch, but preserve the user's r10 across
     * the check: it is a normal callee-saved-by-ABI register for the syscall
     * instruction and user code may keep live values in it. */
    push r10
    mov r10, rcx
    shr r10, 47
    cmp r10, 0
    je 1f
    cmp r10, 0x1ffff
    jne 2f
1:
    or r11, 0x200
    pop r10
    mov rsp, [rsp + 24]
    swapgs
    sysretq
2:
    pop r10
    mov rdi, rcx
    call syscall_bad_rip
    ud2
    .size syscall_entry, . - syscall_entry
"#
);

unsafe extern "C" {
    fn syscall_entry();
}

/// Configure the boot processor's syscall MSRs and per-CPU GS bases.
pub fn init() {
    percpu_init();
    init_msrs();
    wrmsr(IA32_GS_BASE, 0);
    wrmsr(IA32_KERNEL_GS_BASE, percpu_address());
}

/// Configure the syscall MSRs on an application processor.
///
/// `STAR`, `LSTAR`, `FMASK` and `EFER.SCE` are per-CPU state; an AP that enters ring 3 must
/// install them before its first user syscall.  The GS pair is already installed by `ap_start`'s
/// `install_gs`, and `enter_user` normalises it before ring 3.
pub fn init_cpu() {
    init_msrs();
}

/// Write `EFER.SCE`, `STAR`, `LSTAR`, `FMASK` and enable FSGSBASE on this CPU.
fn init_msrs() {
    let efer = rdmsr(IA32_EFER);
    wrmsr(IA32_EFER, efer | 1);
    let star = (u64::from(STAR_USER_BASE) << 48) | (u64::from(KERNEL_CS) << 32);
    wrmsr(IA32_STAR, star);
    wrmsr(IA32_LSTAR, syscall_entry as *const () as usize as u64);
    wrmsr(IA32_FMASK, SYSCALL_FMASK);
    enable_fsgsbase();
}

/// Enable user `rdfsbase`/`wrfsbase` when the CPU advertises FSGSBASE.
///
/// The TLS qtest uses `rdfsbase` to observe the base that the context switch
/// installed; the kernel's authoritative source remains the `Thread` field.
fn enable_fsgsbase() {
    let leaf = core::arch::x86_64::__cpuid_count(7, 0);
    if leaf.ebx & 1 == 0 {
        return;
    }
    let mut cr4: u64;
    // SAFETY: ring-0 CR4 read.
    unsafe {
        core::arch::asm!(
            "mov {}, cr4",
            out(reg) cr4,
            options(nomem, nostack, preserves_flags),
        );
    }
    if cr4 & (1 << 16) == 0 {
        cr4 |= 1 << 16;
        // SAFETY: FSGSBASE was advertised by CPUID; this only opens the
        // per-thread base instructions to user mode.
        unsafe {
            core::arch::asm!(
                "mov cr4, {}",
                in(reg) cr4,
                options(nostack, preserves_flags),
            );
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn syscall_bad_rip(rip: u64) -> ! {
    crate::process::bad_sysret(rip)
}

/// User-visible RFLAGS bits that `SIGRETURN` may restore. IOPL, NT, VM and the
/// reserved high bits are never taken from the user context; IF and bit 1 are
/// forced on because ring 3 cannot clear them on this kernel.
const USER_RFLAGS_MASK: u64 = 0x0000_0000_0024_0fd7;
/// RFLAGS value forced on return to user mode (bit 1 and IF).
const USER_RFLAGS_SET: u64 = 0x202;

/// `SA_ONSTACK`: run the handler on the thread's alternate signal stack.
const SA_ONSTACK: u32 = 0x0800_0000;
/// `SA_NODEFER`: do not add the delivered signal to the handler mask.
const SA_NODEFER: u32 = 0x4000_0000;
/// `SA_RESETHAND`: reset the action to `SIG_DFL` before the handler runs.
const SA_RESETHAND: u32 = 0x8000_0000;
/// POSIX `SIGILL` (the ABI carries it as a fault kind, not a process constant).
const SIGILL: u32 = 4;

/// `mov $SIGRETURN, %eax; syscall; ud2`.
///
/// The return address in every C19 frame points here. The kernel maps these bytes in a fixed
/// read/execute page in every user address space at exec, init and spawn; no userland code
/// registers a trampoline. The immediate comes from the ABI constant, never a literal.
pub const SIGCODE_SIZE: usize = 9;
/// The kernel-installed sigcode image.
pub const SIGCODE: [u8; SIGCODE_SIZE] = {
    let number = (raven_abi::signal::SIGRETURN as u32).to_le_bytes();
    [
        0xb8, number[0], number[1], number[2], number[3], 0x0f, 0x05, 0x0f, 0x0b,
    ]
};

/// C `ucontext_t` size on x86-64 (`raven-libc-abi::structs::UcontextT`).
const UCONTEXT_SIZE: usize = 936;
/// C `siginfo_t` size (`raven-libc-abi::structs::SiginfoT`).
const SIGINFO_SIZE: usize = 128;
/// `sf_ra` at the frame base; the handler's `ret` lands on the sigcode.
const SIGFRAME_RA_OFFSET: usize = 0;
/// `sf_si` follows the return address.
const SIGFRAME_SI_OFFSET: usize = SIGFRAME_RA_OFFSET + 8;
/// `sf_uc` follows the siginfo.
const SIGFRAME_UC_OFFSET: usize = SIGFRAME_SI_OFFSET + SIGINFO_SIZE;
/// Whole frame written below `sp - 128`.
const SIGFRAME_SIZE: usize = SIGFRAME_UC_OFFSET + UCONTEXT_SIZE;
/// `ucontext_t.uc_link`.
const UC_LINK_OFFSET: usize = 8;
/// `ucontext_t.uc_stack`.
const UC_STACK_OFFSET: usize = 16;
/// `ucontext_t.uc_mcontext`.
const UC_MCONTEXT_OFFSET: usize = 40;
/// `ucontext_t.uc_sigmask` (word 37 of the x86-64 C layout).
const UC_SIGMASK_OFFSET: usize = 37 * 8;

const _: () = {
    assert!(SIGFRAME_SIZE == 1072);
    assert!(SIGFRAME_UC_OFFSET == 136);
    assert!(UC_MCONTEXT_OFFSET + core::mem::size_of::<MContext>() <= UC_SIGMASK_OFFSET);
    assert!(UC_SIGMASK_OFFSET + 8 * 16 + 8 * 64 == UCONTEXT_SIZE);
};

/// One queued C19 record rendered into the C `siginfo_t` layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInfo {
    /// Signal number.
    pub signo: u32,
    /// `si_code`.
    pub code: u32,
    /// Sender pid (`si_pid`).
    pub sender: u64,
    /// Sender uid (`si_uid`).
    pub uid: u32,
    /// Queued `sigval` (`si_value`).
    pub value: u64,
}

/// User addresses of the frame pieces handed to the handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameLayout {
    /// Frame base; the handler's `rsp` and `[rsp]` is the sigcode return address.
    pub base: u64,
    /// `&siginfo_t` (`rsi`).
    pub siginfo: u64,
    /// `&ucontext_t` (`rdx`).
    pub ucontext: u64,
}

/// Scratch buffer for one frame; a static keeps the 1 KiB image off the bounded kernel stack.
static SIGNAL_FRAME: spin::Mutex<[u8; SIGFRAME_SIZE]> = spin::Mutex::new([0u8; SIGFRAME_SIZE]);

fn put_u32(buffer: &mut [u8], offset: usize, value: u32) -> Result<(), Errno> {
    let slot = buffer
        .get_mut(offset..offset.saturating_add(4))
        .ok_or(Errno::EFAULT)?;
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn put_u64(buffer: &mut [u8], offset: usize, value: u64) -> Result<(), Errno> {
    let slot = buffer
        .get_mut(offset..offset.saturating_add(8))
        .ok_or(Errno::EFAULT)?;
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

/// Frame base for a handler entered as if by a `call`: `rsp` is 8 mod 16 and `[rsp]` is the
/// return address. `top` is `sp - 128` on the normal stack or the top of the alternate stack.
#[must_use]
pub fn frame_base(top: u64) -> Option<u64> {
    let bottom = top.checked_sub(SIGFRAME_SIZE as u64)?;
    if bottom & 0xf == 8 {
        Some(bottom)
    } else {
        (bottom & !0xf).checked_sub(8)
    }
}

/// Render one C19 frame into `buffer`.
///
/// The layout is the NetBSD `sigframe_siginfo` order (`sf_ra`, `sf_si`, `sf_uc`) with the C36
/// `mcontext_t` and the libc x86-64 `ucontext_t` field offsets. All addresses in the frame are
/// absolute user addresses.
#[allow(clippy::too_many_arguments)] // one frame has eight independent ABI inputs
pub fn build_sigframe(
    buffer: &mut [u8],
    base: u64,
    sigcode: u64,
    context: &MContext,
    mask: u64,
    info: FrameInfo,
    uc_link: u64,
    altstack: (u64, u64, u32),
) -> Result<FrameLayout, Errno> {
    buffer.fill(0);
    put_u64(buffer, SIGFRAME_RA_OFFSET, sigcode)?;
    put_u32(buffer, SIGFRAME_SI_OFFSET, info.signo)?;
    put_u32(buffer, SIGFRAME_SI_OFFSET + 8, info.code)?;
    put_u64(
        buffer,
        SIGFRAME_SI_OFFSET + 16,
        (info.sender & 0xffff_ffff) | (u64::from(info.uid) << 32),
    )?;
    put_u64(buffer, SIGFRAME_SI_OFFSET + 24, info.value)?;

    put_u64(buffer, SIGFRAME_UC_OFFSET + UC_LINK_OFFSET, uc_link)?;
    put_u64(buffer, SIGFRAME_UC_OFFSET + UC_STACK_OFFSET, altstack.0)?;
    put_u64(
        buffer,
        SIGFRAME_UC_OFFSET + UC_STACK_OFFSET + 8,
        u64::from(altstack.2),
    )?;
    put_u64(
        buffer,
        SIGFRAME_UC_OFFSET + UC_STACK_OFFSET + 16,
        altstack.1,
    )?;
    put_u64(buffer, SIGFRAME_UC_OFFSET + UC_SIGMASK_OFFSET, mask)?;
    // SAFETY: `context` is a live, fully initialised `MContext`; the byte view covers exactly
    // `size_of::<MContext>()` bytes and is only read for the copy into the frame buffer.
    let source = unsafe {
        core::slice::from_raw_parts(
            context as *const MContext as *const u8,
            core::mem::size_of::<MContext>(),
        )
    };
    let destination = buffer
        .get_mut(
            SIGFRAME_UC_OFFSET.saturating_add(UC_MCONTEXT_OFFSET)
                ..SIGFRAME_UC_OFFSET
                    .saturating_add(UC_MCONTEXT_OFFSET)
                    .saturating_add(core::mem::size_of::<MContext>()),
        )
        .ok_or(Errno::EFAULT)?;
    destination.copy_from_slice(source);

    Ok(FrameLayout {
        base,
        siginfo: base
            .checked_add(SIGFRAME_SI_OFFSET as u64)
            .ok_or(Errno::EFAULT)?,
        ucontext: base
            .checked_add(SIGFRAME_UC_OFFSET as u64)
            .ok_or(Errno::EFAULT)?,
    })
}

/// Scratch context for one delivery; serialised because the copy is short and must not enlarge
/// the 20 KiB kernel stack by a 256-byte frame per call.
static SIGNAL_CONTEXT: spin::Mutex<MContext> = spin::Mutex::new(MContext::zero());

/// Snapshot the interrupted user state in the C36 `mcontext_t` layout.
#[allow(clippy::indexing_slicing)] // Register indices are compile-time constants within `gregs`.
pub fn context_from_frame(frame: &TrapFrame, context: &mut MContext) {
    context.gregs = [0; 23];
    context.fpregs = 0;
    context.reserved = [0; 8];
    context.gregs[signal::REG_R8] = frame.r8;
    context.gregs[signal::REG_R9] = frame.r9;
    context.gregs[signal::REG_R10] = frame.r10;
    context.gregs[signal::REG_R11] = frame.r11;
    context.gregs[signal::REG_R12] = frame.r12;
    context.gregs[signal::REG_R13] = frame.r13;
    context.gregs[signal::REG_R14] = frame.r14;
    context.gregs[signal::REG_R15] = frame.r15;
    context.gregs[signal::REG_RDI] = frame.rdi;
    context.gregs[signal::REG_RSI] = frame.rsi;
    context.gregs[signal::REG_RBP] = frame.rbp;
    context.gregs[signal::REG_RBX] = frame.rbx;
    context.gregs[signal::REG_RDX] = frame.rdx;
    context.gregs[signal::REG_RAX] = frame.rax;
    context.gregs[signal::REG_RCX] = frame.rcx;
    context.gregs[signal::REG_RSP] = frame.rsp;
    context.gregs[signal::REG_RIP] = frame.rip;
    context.gregs[signal::REG_EFL] = frame.rflags;
    context.gregs[signal::REG_CSGSFS] = frame.cs & 0xffff;
    context.gregs[signal::REG_ERR] = frame.error_code;
    context.gregs[signal::REG_TRAPNO] = frame.vector;
}

/// Apply a `SIGRETURN` context to the user frame, forcing user segments and a
/// safe RFLAGS image.
#[allow(clippy::indexing_slicing)] // Register indices are compile-time constants within `gregs`.
pub fn restore_signal_context(frame: &mut TrapFrame, context: &MContext) {
    frame.r8 = context.gregs[signal::REG_R8];
    frame.r9 = context.gregs[signal::REG_R9];
    frame.r10 = context.gregs[signal::REG_R10];
    frame.r11 = context.gregs[signal::REG_R11];
    frame.r12 = context.gregs[signal::REG_R12];
    frame.r13 = context.gregs[signal::REG_R13];
    frame.r14 = context.gregs[signal::REG_R14];
    frame.r15 = context.gregs[signal::REG_R15];
    frame.rdi = context.gregs[signal::REG_RDI];
    frame.rsi = context.gregs[signal::REG_RSI];
    frame.rbp = context.gregs[signal::REG_RBP];
    frame.rbx = context.gregs[signal::REG_RBX];
    frame.rdx = context.gregs[signal::REG_RDX];
    frame.rax = context.gregs[signal::REG_RAX];
    frame.rcx = context.gregs[signal::REG_RCX];
    frame.rsp = context.gregs[signal::REG_RSP];
    frame.rip = context.gregs[signal::REG_RIP];
    frame.rflags = (context.gregs[signal::REG_EFL] & USER_RFLAGS_MASK) | USER_RFLAGS_SET;
    frame.cs = u64::from(USER_CS);
    frame.ss = u64::from(USER_SS);
    frame.error_code = 0;
    frame.vector = 0;
}

/// Restore a `SIGRETURN` context from the active C19 frame or the legacy registered area.
pub fn sigreturn_frame(frame: &mut TrapFrame) -> Result<(), Errno> {
    let current = crate::sched::current_thread().ok_or(Errno::ESRCH)?;
    let user_context = current
        .signal_frame_uc
        .load(core::sync::atomic::Ordering::Acquire);
    if user_context != 0 {
        if current
            .signal_depth
            .load(core::sync::atomic::Ordering::Acquire)
            == 0
        {
            return Err(Errno::EINVAL);
        }
        let mut context = SIGNAL_CONTEXT.lock();
        // SAFETY: `context` is a live `MContext`; the mutable byte view is exact and is not
        // aliased while `copy_from_user` runs.
        let destination = unsafe {
            core::slice::from_raw_parts_mut(
                &mut *context as *mut MContext as *mut u8,
                core::mem::size_of::<MContext>(),
            )
        };
        let source = UserSlice::new(
            user_context
                .checked_add(UC_MCONTEXT_OFFSET as u64)
                .ok_or(Errno::EINVAL)?,
            core::mem::size_of::<MContext>() as u64,
        )?;
        copy_from_user(destination, source)?;
        let mut mask = 0u64;
        // SAFETY: `mask` is a live local; the mutable byte view covers exactly eight bytes.
        let destination = unsafe {
            core::slice::from_raw_parts_mut(&mut mask as *mut u64 as *mut u8, size_of::<u64>())
        };
        copy_from_user(
            destination,
            UserSlice::new(
                user_context
                    .checked_add(UC_SIGMASK_OFFSET as u64)
                    .ok_or(Errno::EINVAL)?,
                8,
            )?,
        )?;
        let mut link = 0u64;
        // SAFETY: `link` is a live local; the mutable byte view covers exactly eight bytes.
        let destination = unsafe {
            core::slice::from_raw_parts_mut(&mut link as *mut u64 as *mut u8, size_of::<u64>())
        };
        copy_from_user(
            destination,
            UserSlice::new(
                user_context
                    .checked_add(UC_LINK_OFFSET as u64)
                    .ok_or(Errno::EINVAL)?,
                8,
            )?,
        )?;
        restore_signal_context(frame, &context);
        current.sigmask.store(
            mask & !((1u64 << (process::status::SIGKILL - 1))
                | (1u64 << (process::status::SIGSTOP - 1))),
            core::sync::atomic::Ordering::Release,
        );
        current.signal_frame_uc.store(
            if link == 0 || is_user_address(link) {
                link
            } else {
                0
            },
            core::sync::atomic::Ordering::Release,
        );
        current
            .signal_depth
            .fetch_sub(1, core::sync::atomic::Ordering::AcqRel);
        return Ok(());
    }
    let mut context = SIGNAL_CONTEXT.lock();
    thread::sigreturn(&mut context)?;
    restore_signal_context(frame, &context);
    Ok(())
}

/// Enter the registered entry with `bits` in `rdi`, the optional fault record
/// pointer in `rsi` and the current user signal mask in `rdx`.
fn enter_signal(
    frame: &mut TrapFrame,
    due: &thread::SignalEntry,
    bits: u64,
    second: u64,
    mask: u64,
) {
    let stack = if due.flags & ENTRY_SP != 0 {
        frame.rsp
    } else {
        match frame.rsp.checked_sub(128) {
            Some(stack) => stack & !0xf,
            None => process::exit_process(Errno::EFAULT.to_ret() as i32),
        }
    };
    if !is_user_address(stack) {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    }
    frame.rdi = bits;
    frame.rsi = second;
    frame.rdx = mask;
    frame.rip = due.entry;
    frame.rsp = stack;
}

/// Apply a C19 default action. Ignore/continue are consumed without a frame; terminate/core end
/// the process with the POSIX signal status.
fn apply_c19_default(signal: PendingSignal) {
    use process::status::{DefaultAction, signal_status};

    match process::status::default_action(signal.signo) {
        Some(DefaultAction::Terminate) => {
            process::exit_process(0i32.saturating_sub(signal.signo as i32))
        }
        Some(DefaultAction::Core) => {
            process::exit_process(0i32.saturating_sub(signal_status(signal.signo, true)))
        }
        _ => {}
    }
}

/// Build and enter one C19 frame for `signal`.
fn enter_c19_signal(
    frame: &mut TrapFrame,
    caller: &alloc::sync::Arc<process::Process>,
    current: &alloc::sync::Arc<crate::sched::thread::Thread>,
    signal: PendingSignal,
    action: SigAction,
) -> bool {
    use core::sync::atomic::Ordering;

    let handler = action.handler;
    if handler == SIG_DFL || handler == SIG_IGN || !is_user_address(handler as u64) {
        process::exit_process(0i32.saturating_sub(SIGILL as i32));
    }
    let old_mask = current.sigmask.load(Ordering::Acquire);
    let alt_sp = current.altstack_sp.load(Ordering::Acquire);
    let alt_size = current.altstack_size.load(Ordering::Acquire);
    let alt_flags = current.altstack_flags.load(Ordering::Acquire);
    let on_altstack = action.flags & SA_ONSTACK != 0
        && alt_size != 0
        && alt_flags & process::SS_DISABLE == 0
        && !(frame.rsp >= alt_sp && frame.rsp < alt_sp.saturating_add(alt_size));
    let top = if on_altstack {
        alt_sp.saturating_add(alt_size)
    } else {
        match frame.rsp.checked_sub(128) {
            Some(top) => top,
            None => process::exit_process(Errno::EFAULT.to_ret() as i32),
        }
    };
    let Some(base) = frame_base(top) else {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    };
    if UserSlice::new(base, SIGFRAME_SIZE as u64).is_err() {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    }
    let sigcode = caller.sigcode_addr.load(Ordering::Acquire);
    if sigcode == 0 || !is_user_address(sigcode) {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    }

    let old_context = current.signal_frame_uc.load(Ordering::Acquire);
    let info = FrameInfo {
        signo: signal.signo,
        code: signal.code,
        sender: signal.sender,
        uid: signal.uid,
        value: signal.value,
    };
    let layout = {
        let context = {
            let mut context = SIGNAL_CONTEXT.lock();
            context_from_frame(frame, &mut context);
            *context
        };
        let mut buffer = SIGNAL_FRAME.lock();
        let layout = match build_sigframe(
            &mut buffer[..],
            base,
            sigcode,
            &context,
            old_mask,
            info,
            old_context,
            (alt_sp, alt_size, alt_flags),
        ) {
            Ok(layout) => layout,
            Err(_) => process::exit_process(Errno::EFAULT.to_ret() as i32),
        };
        if copy_to_user(
            match UserSlice::new(base, SIGFRAME_SIZE as u64) {
                Ok(destination) => destination,
                Err(_) => process::exit_process(Errno::EFAULT.to_ret() as i32),
            },
            &buffer[..],
        )
        .is_err()
        {
            process::exit_process(Errno::EFAULT.to_ret() as i32);
        }
        layout
    };

    let bit = 1u64.wrapping_shl(signal.signo.saturating_sub(1) & 63);
    let mut next_mask = old_mask | action.mask.first().copied().unwrap_or(0);
    if action.flags & SA_NODEFER == 0 {
        next_mask |= bit;
    }
    next_mask &=
        !((1u64 << (process::status::SIGKILL - 1)) | (1u64 << (process::status::SIGSTOP - 1)));
    current.sigmask.store(next_mask, Ordering::Release);
    current
        .signal_frame_uc
        .store(layout.ucontext, Ordering::Release);
    current.signal_depth.fetch_add(1, Ordering::AcqRel);
    if action.flags & SA_RESETHAND != 0 {
        let _ = caller
            .sigacts
            .lock()
            .install(signal.signo, SigAction::DEFAULT);
    }

    frame.rdi = u64::from(signal.signo);
    frame.rsi = layout.siginfo;
    frame.rdx = layout.ucontext;
    frame.rip = handler as u64;
    frame.rsp = layout.base;
    frame.rflags = (frame.rflags & USER_RFLAGS_MASK) | USER_RFLAGS_SET;
    frame.cs = u64::from(USER_CS);
    frame.ss = u64::from(USER_SS);
    frame.error_code = 0;
    frame.vector = 0;
    true
}

/// Deliver the oldest unmasked C19 process signal. Returns true when a signal was consumed (a
/// handler was entered or the default/ignore action was applied).
fn deliver_c19(frame: &mut TrapFrame) -> bool {
    use core::sync::atomic::Ordering;

    let Some(caller) = process::current() else {
        return false;
    };
    if !caller.c19_armed() {
        return false;
    }
    let Some(current) = crate::sched::current_thread() else {
        return false;
    };
    let mask = current.sigmask.load(Ordering::Acquire);
    let Some(signal) = caller.take_deliverable_signal(mask) else {
        if current.signal_entry.load(Ordering::Acquire) == 0 {
            current
                .kick_bits
                .fetch_and(!signal::KICK_CHECK, Ordering::AcqRel);
        }
        return false;
    };
    if current.signal_entry.load(Ordering::Acquire) == 0
        && !caller.has_pending_signal_where(|pending| {
            mask & 1u64.wrapping_shl(pending.signo.saturating_sub(1) & 63) == 0
        })
    {
        current
            .kick_bits
            .fetch_and(!signal::KICK_CHECK, Ordering::AcqRel);
    }
    let action = caller
        .sigacts
        .lock()
        .get(signal.signo)
        .unwrap_or(SigAction::DEFAULT);
    if action.handler == SIG_IGN {
        return true;
    }
    if action.handler == SIG_DFL {
        apply_c19_default(signal);
        return true;
    }
    enter_c19_signal(frame, &caller, &current, signal, action)
}

/// Deliver a pending C19 signal, or the legacy `SIGRESEND`/kick entry.
///
/// Called after the syscall result is in the frame and before `sysretq`, so the saved context is
/// exactly the interrupted user state.
pub fn deliver_signal(frame: &mut TrapFrame) {
    if deliver_c19(frame) {
        return;
    }
    if let Some(due) = thread::signal_resend_due() {
        let fault_pointer = {
            let mut context = SIGNAL_CONTEXT.lock();
            context_from_frame(frame, &mut context);
            thread::signal_commit(&due, &context, None)
        };
        enter_signal(frame, &due, u64::from(due.bits), fault_pointer, due.mask);
        return;
    }
    let Some(due) = thread::signal_due() else {
        return;
    };
    let fault_pointer = {
        let mut context = SIGNAL_CONTEXT.lock();
        context_from_frame(frame, &mut context);
        thread::signal_commit(&due, &context, None)
    };
    enter_signal(frame, &due, u64::from(due.bits), fault_pointer, due.mask);
}

/// Deliver a synchronous user fault to the signal entry.
///
/// Returns false when the thread has no usable `SIG_SETUP` (no registration,
/// already inside a signal); the caller then takes the default-action path.
pub fn deliver_fault(frame: &mut TrapFrame, kind: u32, addr: u64) -> bool {
    let Some(due) = thread::signal_fault_due() else {
        return false;
    };
    let record = FaultRecord {
        kind,
        addr: addr as usize,
        pc: frame.rip as usize,
    };
    let fault_pointer = {
        let mut context = SIGNAL_CONTEXT.lock();
        context_from_frame(frame, &mut context);
        thread::signal_commit(&due, &context, Some(&record))
    };
    enter_signal(frame, &due, 0, fault_pointer, due.mask);
    true
}

/// Default action for a user fault with no signal entry: terminate with the
/// negative signal number used by the exit-status convention.
pub fn terminate_fault(kind: u32) -> ! {
    let status = match kind {
        signal::FAULT_DIVIDE => -8,
        signal::FAULT_ILLEGAL => -4,
        signal::FAULT_MISALIGN => -7,
        signal::FAULT_BREAKPOINT => -5,
        _ => -11,
    };
    crate::kprintln!("process: user fault kind={kind} status={status}");
    process::exit_current(status)
}

fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: ring-0 MSR read; the caller supplies architectural MSR numbers.
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags),
        );
    }
    (u64::from(high) << 32) | u64::from(low)
}

fn wrmsr(msr: u32, value: u64) {
    // SAFETY: ring-0 MSR write; the caller supplies architectural MSR numbers and values.
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags),
        );
    }
}

const _: () = {
    assert!(KERNEL_CS == 0x08);
    assert!(KERNEL_SS == 0x10);
    assert!(USER_CS == 0x23);
    assert!(USER_SS == 0x1B);
    assert!(SYSCALL_FMASK & 0x200 != 0);
};
