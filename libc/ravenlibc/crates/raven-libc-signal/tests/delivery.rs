// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The named delivery scenarios of ticket section 4.13. Each test has the ticket's name.
//!
//! The fake kernel (`common::fake`) supplies the simulated delivery frames; handlers are real
//! `extern "C"` functions called through the crate's `__rl_call_handler` asm.

#![allow(unsafe_code)]
#![cfg(target_arch = "x86_64")]
#![allow(
    unused_assignments,
    clippy::undocumented_unsafe_blocks,
    clippy::explicit_auto_deref,
    clippy::never_loop,
    clippy::identity_op,
    clippy::unnecessary_cast
)]

mod common;

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::fake::{FAKE_PID, HostSigKernel, Trace};
use raven_libc_abi::structs::{JmpBuf, SiginfoT, Sigval, StackT, UcontextT};
use raven_libc_signal::dispositions::{
    DispTable, Disposition, SA_NODEFER, SA_ONSTACK, SA_RESETHAND, SA_SIGINFO,
};
use raven_libc_signal::frame;
use raven_libc_signal::kernel::{FaultKind, FaultRecord, SigCaps};
use raven_libc_signal::sigjmp::{siglongjmp, sigsetjmp_with_mask};
use raven_libc_signal::{SigSet, kill, mask, raise, sigqueue, sigsuspend, sigtimedwait};

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy)]
struct Active {
    kernel: *const HostSigKernel,
    table: *const DispTable,
    jump_buffer: *mut JmpBuf,
    mask_word: *mut u64,
}

// SAFETY: the tests hold `TEST_LOCK`, so exactly one test uses the pointers at a time.
unsafe impl Send for Active {}

static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);

fn set_active(active: Active) {
    *ACTIVE.lock().unwrap() = Some(active);
}

fn active() -> Active {
    ACTIVE.lock().unwrap().expect("active test context")
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
struct Record {
    sig: i32,
    live_mask: u64,
    uc_mask: u64,
    sp: usize,
    depth: usize,
    si_code: i32,
    si_pid: i32,
    si_value: u64,
    si_addr: usize,
    uc_pc: usize,
}

static LOG: Mutex<Vec<Record>> = Mutex::new(Vec::new());
static DEPTH: AtomicUsize = AtomicUsize::new(0);
static MAX_DEPTH: AtomicUsize = AtomicUsize::new(0);
static MODE: AtomicUsize = AtomicUsize::new(0);
static NESTED_ONCE: AtomicUsize = AtomicUsize::new(0);

fn clear_log() {
    LOG.lock().unwrap().clear();
    DEPTH.store(0, Ordering::SeqCst);
    MAX_DEPTH.store(0, Ordering::SeqCst);
    MODE.store(0, Ordering::SeqCst);
    NESTED_ONCE.store(0, Ordering::SeqCst);
}

fn push(record: Record) {
    LOG.lock().unwrap().push(record);
}

fn log() -> Vec<Record> {
    LOG.lock().unwrap().clone()
}

extern "C" fn record_handler(sig: i32) {
    let depth = DEPTH.fetch_add(1, Ordering::SeqCst) + 1;
    MAX_DEPTH.fetch_max(depth, Ordering::SeqCst);
    let active = active();
    // SAFETY: the pointers outlive the test and are used under `TEST_LOCK`.
    let kernel = unsafe { &*active.kernel };
    push(Record {
        sig,
        live_mask: kernel.mask(),
        uc_mask: 0,
        sp: raven_libc_signal::context::current_stack_pointer(),
        depth,
        si_code: 0,
        si_pid: 0,
        si_value: 0,
        si_addr: 0,
        uc_pc: 0,
    });
    if MODE.load(Ordering::SeqCst) == 1 && NESTED_ONCE.swap(1, Ordering::SeqCst) == 0 {
        // SAFETY: `active` holds the live table/kernel for this test.
        let table = unsafe { &*active.table };
        raise(kernel, table, sig).unwrap();
    }
    if MODE.load(Ordering::SeqCst) == 2 && NESTED_ONCE.swap(1, Ordering::SeqCst) == 0 {
        let table = unsafe { &*active.table };
        raven_libc_signal::raw::pending_or(
            kernel.pointer(),
            raven_libc_signal::critical::signal_bit(sig),
        );
        kernel.kick_self(raven_libc_signal::kernel::KICK_CHECK);
        let _ = kernel.safe_point(table, None, None);
    }
    DEPTH.fetch_sub(1, Ordering::SeqCst);
}

extern "C" fn info_handler(sig: i32, info: *const SiginfoT, context: *mut UcontextT) {
    let active = active();
    // SAFETY: the entry passes live objects for the duration of the call.
    let kernel = unsafe { &*active.kernel };
    let record = unsafe {
        if info.is_null() || context.is_null() {
            return;
        }
        Record {
            sig,
            live_mask: kernel.mask(),
            uc_mask: frame::uc_sigmask_word(&*context),
            sp: raven_libc_signal::context::current_stack_pointer(),
            depth: 1,
            si_code: frame::si_code(&*info),
            si_pid: frame::si_pid(&*info),
            si_value: frame::si_value(&*info).bytes[0],
            si_addr: frame::si_addr(&*info),
            uc_pc: frame::ctx_pc(&raw::read_mcontext(&*context)),
        }
    };
    push(record);
    if MODE.load(Ordering::SeqCst) == 3 && !context.is_null() {
        // Handler rewrites the interrupted pc (COPY-OUT check).
        let machine = unsafe { raven_libc_signal::raw::read_mcontext(&*context) };
        let mut machine = machine;
        frame::set_ctx_pc(&mut machine, 0xbeef);
        unsafe { raven_libc_signal::raw::copy_mcontext_in(&machine, &mut *context) };
    }
}

use raven_libc_signal::raw;

extern "C" fn longjmp_handler(_sig: i32) {
    let active = active();
    // SAFETY: the buffer was captured in the test frame that is still alive.
    unsafe {
        siglongjmp(&*active.jump_buffer, 1);
    }
}

static RESETHAND_SEEN: AtomicUsize = AtomicUsize::new(usize::MAX);

extern "C" fn resethand_handler(sig: i32) {
    let active = active();
    // SAFETY: the table is live under `TEST_LOCK`.
    let table = unsafe { &*active.table };
    RESETHAND_SEEN.store(table.read(sig).handler, Ordering::SeqCst);
}

extern "C" fn clobber_errno(_sig: i32) {
    raven_libc_sys::errno::set_errno(9);
}

fn cancel_hook(pc: &mut usize, _bits: u32) -> bool {
    *pc = 0xca11;
    true
}

fn install(
    kernel: &HostSigKernel,
    table: &DispTable,
    sig: i32,
    handler: usize,
    flags: u32,
    mask_word: u64,
) {
    let mut action = Disposition::handler(handler);
    action.flags = flags;
    action.mask = mask_word;
    table.write(sig, action);
    mask::set_mask(kernel, 0);
}

fn setup() -> (Box<HostSigKernel>, Box<DispTable>) {
    let kernel = Box::new(HostSigKernel::new());
    let table = Box::new(DispTable::new());
    mask::set_mask(&*kernel, 0);
    let active = Active {
        kernel: &*kernel,
        table: &*table,
        jump_buffer: core::ptr::null_mut(),
        mask_word: core::ptr::null_mut(),
    };
    set_active(active);
    (kernel, table)
}

#[test]
fn plain_handler() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        10,
        record_handler as *const () as usize,
        0,
        0,
    );
    raise(&*kernel, &*table, 10).unwrap();
    let records = log();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].sig, 10);
}

#[test]
fn masked_then_unblocked() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        12,
        record_handler as *const () as usize,
        0,
        0,
    );
    mask::set_mask(&*kernel, raven_libc_signal::critical::signal_bit(12));
    raise(&*kernel, &*table, 12).unwrap();
    assert!(log().is_empty(), "handler ran while masked");
    let mut set = SigSet::empty();
    set.add(12).unwrap();
    raven_libc_signal::sigprocmask(
        &*kernel,
        raven_libc_abi::generated::signal::SIG_UNBLOCK,
        Some(&set),
        None,
    )
    .unwrap();
    kernel.safe_point(&*table, None, None);
    assert_eq!(log().len(), 1, "handler must run at unblock");
}

#[test]
fn sa_nodefer() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    MODE.store(1, Ordering::SeqCst);
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        10,
        record_handler as *const () as usize,
        SA_NODEFER,
        0,
    );
    raise(&*kernel, &*table, 10).unwrap();
    assert_eq!(log().len(), 2, "nested delivery with SA_NODEFER");
    assert_eq!(MAX_DEPTH.load(Ordering::SeqCst), 2);

    clear_log();
    MODE.store(1, Ordering::SeqCst);
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        10,
        record_handler as *const () as usize,
        0,
        0,
    );
    raise(&*kernel, &*table, 10).unwrap();
    assert_eq!(log().len(), 2, "pending then delivered after return");
    assert_eq!(
        MAX_DEPTH.load(Ordering::SeqCst),
        1,
        "without SA_NODEFER it must not nest"
    );
}

#[test]
fn sa_resethand() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    RESETHAND_SEEN.store(usize::MAX, Ordering::SeqCst);
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        15,
        resethand_handler as *const () as usize,
        SA_RESETHAND,
        0,
    );
    raise(&*kernel, &*table, 15).unwrap();
    assert_eq!(
        RESETHAND_SEEN.load(Ordering::SeqCst),
        0,
        "the disposition must be SIG_DFL before the handler runs"
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        raise(&*kernel, &*table, 15).unwrap();
    }));
    assert!(result.is_err(), "the second raise must apply SIG_DFL");
}

#[test]
fn sa_onstack() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    let buffer = vec![0u8; 32 * 1024];
    let base = buffer.as_ptr() as usize;
    let stack = StackT {
        ss_sp: base as *mut core::ffi::c_void,
        ss_flags: 0,
        ss_size: 32 * 1024,
    };
    raven_libc_signal::sigaltstack(&*kernel, Some(&stack), None).unwrap();
    install(
        &*kernel,
        &*table,
        10,
        record_handler as *const () as usize,
        SA_ONSTACK,
        0,
    );
    raise(&*kernel, &*table, 10).unwrap();
    let sp = log()[0].sp;
    assert!(sp >= base && sp < base + 32 * 1024, "handler sp {sp:#x}");
}

#[test]
fn sa_siginfo_fields() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        40,
        info_handler as *const () as usize,
        SA_SIGINFO,
        0,
    );
    sigqueue(&*kernel, FAKE_PID, 40, Sigval { bytes: [0xabc] }).unwrap();
    kernel.safe_point(&*table, None, None);
    let records = log();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].sig, 40);
    assert_eq!(records[0].si_code, -1, "SI_QUEUE");
    assert_eq!(records[0].si_value, 0xabc);
    assert_eq!(records[0].si_pid, FAKE_PID);
}

#[test]
fn mask_in_handler() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    let sa_mask = 0x2u64;
    install(
        &*kernel,
        &*table,
        10,
        info_handler as *const () as usize,
        SA_SIGINFO,
        sa_mask,
    );
    let old = 0x4u64;
    mask::set_mask(&*kernel, old);
    raise(&*kernel, &*table, 10).unwrap();
    let record = log()[0];
    assert_eq!(record.uc_mask, old, "uc_sigmask is the interrupted mask");
    let expected = old | sa_mask | raven_libc_signal::critical::signal_bit(10);
    assert_eq!(record.live_mask, expected);
}

#[test]
fn handler_edits_pc() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    MODE.store(3, Ordering::SeqCst);
    let (kernel, table) = setup();
    let mut context = common::fake::FakeCtx::new(0x1000, 0);
    let stack = vec![0u8; 32 * 1024];
    context.sp = stack.as_ptr() as usize + 32 * 1024 - 1024;
    kernel.set_context(context);
    install(
        &*kernel,
        &*table,
        10,
        info_handler as *const () as usize,
        SA_SIGINFO,
        0,
    );
    kernel.kick_self(raven_libc_signal::kernel::KICK_CHECK);
    raven_libc_signal::raw::pending_or(
        kernel.pointer(),
        raven_libc_signal::critical::signal_bit(10),
    );
    kernel.safe_point(&*table, None, None);
    assert_eq!(
        kernel.context().pc,
        0xbeef,
        "COPY-OUT must carry the handler's pc edit"
    );
}

#[test]
fn nested_kick() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    MODE.store(2, Ordering::SeqCst);
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        10,
        record_handler as *const () as usize,
        SA_NODEFER,
        0,
    );
    raise(&*kernel, &*table, 10).unwrap();
    assert_eq!(log().len(), 2, "both deliveries complete");
    assert_eq!(MAX_DEPTH.load(Ordering::SeqCst), 2);
}

#[test]
fn longjmp_out() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        10,
        longjmp_handler as *const () as usize,
        0,
        0,
    );
    let mut buffer = JmpBuf::ZERO;
    let mut word = 0x55u64;
    {
        let mut active = ACTIVE.lock().unwrap();
        if let Some(active) = active.as_mut() {
            active.jump_buffer = &mut buffer;
            active.mask_word = &mut word;
        }
    }
    let value = sigsetjmp_with_mask(&mut buffer, 1, &mut word);
    if value == 0 {
        word = 0x1234;
        raise(&*kernel, &*table, 10).unwrap();
        panic!("longjmp did not leave the handler");
    }
    assert_eq!(value, 1);
    assert_eq!(word, 0x55, "siglongjmp must restore the saved mask");
    // A new signal after the jump still works (no stale area).
    clear_log();
    install(
        &*kernel,
        &*table,
        11,
        record_handler as *const () as usize,
        0,
        0,
    );
    raise(&*kernel, &*table, 11).unwrap();
    assert_eq!(log()[0].sig, 11);
}

#[test]
fn spurious_kick() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    kernel.kick_self(raven_libc_signal::kernel::KICK_CHECK);
    let point = kernel.safe_point(&*table, None, None);
    assert!(point.spurious);
    assert!(point.terminated.is_none());
    assert!(kernel.trace().contains(&Trace::Sigreturn));
    assert!(log().is_empty());
}

#[test]
fn rt_order() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        5,
        record_handler as *const () as usize,
        0,
        0,
    );
    for sig in [5, 35, 36, 40] {
        install(
            &*kernel,
            &*table,
            sig,
            record_handler as *const () as usize,
            0,
            0,
        );
    }
    for sig in [36, 35, 5, 40] {
        let mut info = SiginfoT::ZERO;
        info.si_signo = sig;
        kernel.push_pending(sig, info);
    }
    kernel.kick_self(raven_libc_signal::kernel::KICK_CHECK);
    kernel.safe_point(&*table, None, None);
    let order: Vec<i32> = log().iter().map(|record| record.sig).collect();
    assert_eq!(order, vec![5, 35, 36, 40]);
}

#[test]
fn fault_segv() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        11,
        info_handler as *const () as usize,
        SA_SIGINFO,
        0,
    );
    let fault = FaultRecord {
        kind: FaultKind::PageNomap as u32,
        addr: 0xdead_0000,
        pc: 0x1000,
    };
    kernel.kick_self(raven_libc_signal::kernel::KICK_CHECK);
    kernel.safe_point(&*table, None, Some(fault));
    let record = log()[0];
    assert_eq!(record.sig, 11);
    assert_eq!(record.si_code, 1, "SEGV_MAPERR");
    assert_eq!(record.si_addr, 0xdead_0000);
}

#[test]
fn fault_blocked() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        11,
        record_handler as *const () as usize,
        0,
        0,
    );
    mask::set_mask(&*kernel, raven_libc_signal::critical::signal_bit(11));
    let fault = FaultRecord {
        kind: FaultKind::PagePerm as u32,
        addr: 1,
        pc: 2,
    };
    kernel.kick_self(raven_libc_signal::kernel::KICK_CHECK);
    kernel.safe_point(&*table, None, Some(fault));
    assert_eq!(log().len(), 1, "a fault ignores the mask");
}

#[test]
fn fault_default() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    let fault = FaultRecord {
        kind: FaultKind::PageNomap as u32,
        addr: 1,
        pc: 2,
    };
    kernel.kick_self(raven_libc_signal::kernel::KICK_CHECK);
    let point = kernel.safe_point(&*table, None, Some(fault));
    assert_eq!(point.terminated, Some((11, true)));
    assert!(kernel.trace().contains(&Trace::Terminate(11, true)));
}

#[test]
fn cancel_bit() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    raven_libc_signal::raw::set_cancel_hook(Some(cancel_hook));
    let context = common::fake::FakeCtx::new(0x1000, 0);
    let stack = vec![0u8; 16 * 1024];
    let mut context = context;
    context.sp = stack.as_ptr() as usize + 16 * 1024 - 1024;
    kernel.set_context(context);
    kernel.kick_self(raven_libc_signal::kernel::KICK_CANCEL);
    kernel.safe_point(&*table, None, None);
    assert_eq!(kernel.context().pc, 0xca11);
    raven_libc_signal::raw::set_cancel_hook(None);
}

#[test]
fn critical_defer() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        10,
        record_handler as *const () as usize,
        0,
        0,
    );
    raven_libc_signal::critical_enter(&*kernel);
    raise(&*kernel, &*table, 10).unwrap();
    assert!(log().is_empty());
    kernel.kick_self(raven_libc_signal::kernel::KICK_CHECK);
    kernel.safe_point(&*table, None, None);
    assert!(
        log().is_empty(),
        "entry must defer inside the critical section"
    );
    raven_libc_signal::critical_exit(&*kernel, &*table);
    kernel.safe_point(&*table, None, None);
    assert_eq!(log().len(), 1, "the replay must deliver at the safe point");
}

#[test]
fn errno_saved() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        12,
        clobber_errno as *const () as usize,
        0,
        0,
    );
    raven_libc_sys::errno::set_errno(123);
    raise(&*kernel, &*table, 12).unwrap();
    assert_eq!(raven_libc_sys::errno::errno(), 123);
}

#[test]
fn sigsuspend_atomic() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        10,
        record_handler as *const () as usize,
        0,
        0,
    );
    raven_libc_signal::raw::pending_or(
        kernel.pointer(),
        raven_libc_signal::critical::signal_bit(10),
    );
    let word = sigsuspend(&*kernel, &*table, &SigSet::empty());
    assert_eq!(word, raven_libc_signal::EINTR_ERR);
    assert_eq!(log().len(), 1);
}

#[test]
fn sigtimedwait_consume() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, table) = setup();
    install(
        &*kernel,
        &*table,
        10,
        record_handler as *const () as usize,
        0,
        0,
    );
    raven_libc_signal::raw::pending_or(
        kernel.pointer(),
        raven_libc_signal::critical::signal_bit(10),
    );
    let timeout = raven_libc_abi::structs::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let result = sigtimedwait(&*kernel, &SigSet::singleton(10), None, Some(timeout)).unwrap();
    assert_eq!(result, 10);
    assert!(log().is_empty(), "sigtimedwait must not run the handler");
}

#[test]
fn kill_forward() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let (kernel, _table) = setup();
    kill(&*kernel, 99, 15).unwrap();
    assert!(kernel.trace().contains(&Trace::ProcKill(99, 15)));
}

#[test]
fn no_proc_kill() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_log();
    let caps = SigCaps {
        proc_kill: false,
        ..SigCaps::HOST
    };
    let kernel = HostSigKernel::with_caps(caps);
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    set_active(Active {
        kernel: &kernel,
        table: &table,
        jump_buffer: core::ptr::null_mut(),
        mask_word: core::ptr::null_mut(),
    });
    install(
        &kernel,
        &table,
        10,
        record_handler as *const () as usize,
        0,
        0,
    );
    assert_eq!(kill(&kernel, 99, 15), Err(raven_libc_signal::ENOSYS_ERR));
    raise(&kernel, &table, 10).unwrap();
    assert_eq!(log().len(), 1);
}
