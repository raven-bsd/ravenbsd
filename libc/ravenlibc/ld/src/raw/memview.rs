// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `MemView` and `RelocApplier` over live target memory.

use core::sync::atomic::{AtomicUsize, Ordering};

use raven_libc_elf::{ApplyError, MemView, RelocApplier};

/// The highest address the loader has mapped (for the raw view's sanity check).
static LIMIT: AtomicUsize = AtomicUsize::new(0);

/// Static TLS TPOFF bases indexed by object id.
static TLS_OFFSETS: [AtomicUsize; 64] = [const { AtomicUsize::new(0) }; 64];

/// One aarch64 dynamic `TLSDESC` argument: the `__tls_get_addr` `{module, offset}` pair.
#[repr(C)]
struct TlsDescIndex {
    module: AtomicUsize,
    offset: AtomicUsize,
}

impl TlsDescIndex {
    const fn new() -> Self {
        Self {
            module: AtomicUsize::new(0),
            offset: AtomicUsize::new(0),
        }
    }
}

static TLS_DESC_INDICES: [TlsDescIndex; 64] = [const { TlsDescIndex::new() }; 64];
static TLS_DESC_NEXT: AtomicUsize = AtomicUsize::new(0);

// A trivial `TLSDESC` resolver: the descriptor's second word is the TP-relative offset the
// caller adds to the thread pointer. The ABI passes the descriptor address in `x0` (aarch64)
// or `rdi` (x86_64), so load and return the argument word.
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".text",
    ".globl __rl_tlsdesc_return",
    ".type __rl_tlsdesc_return,%function",
    "__rl_tlsdesc_return:",
    "ldr x0, [x0, #8]",
    "ret",
    ".size __rl_tlsdesc_return, .-__rl_tlsdesc_return",
    ".section .note.GNU-stack,\"\",%progbits",
);

// The aarch64 dynamic `TLSDESC` resolver: the descriptor argument points at a
// `{module, offset}` pair. `__tls_get_addr` allocates and initialises the calling thread's block
// (copying the module's `.tdata` on first use); the caller then adds TP to the returned offset.
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".text",
    ".globl __rl_tlsdesc_dynamic",
    ".type __rl_tlsdesc_dynamic,%function",
    "__rl_tlsdesc_dynamic:",
    "stp x29, x30, [sp, #-16]!",
    "ldr x0, [x0, #8]",
    "bl __tls_get_addr",
    "mrs x1, tpidr_el0",
    "sub x0, x0, x1",
    "ldp x29, x30, [sp], #16",
    "ret",
    ".size __rl_tlsdesc_dynamic, .-__rl_tlsdesc_dynamic",
    ".section .note.GNU-stack,\"\",%progbits",
);

#[cfg(target_arch = "aarch64")]
unsafe extern "C" {
    fn __rl_tlsdesc_dynamic(descriptor: *const u64) -> u64;
}

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".text",
    ".globl __rl_tlsdesc_return",
    ".type __rl_tlsdesc_return,@function",
    "__rl_tlsdesc_return:",
    "mov rax, [rdi + 8]",
    "ret",
    ".size __rl_tlsdesc_return, .-__rl_tlsdesc_return",
    ".section .note.GNU-stack,\"\",@progbits",
);

#[cfg(target_arch = "riscv64")]
extern "C" fn __rl_tlsdesc_return(_descriptor: *const u64) -> u64 {
    0
}

#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
unsafe extern "C" {
    fn __rl_tlsdesc_return(descriptor: *const u64) -> u64;
}

/// Record one object's static TLS offset for the relocation applier.
pub fn set_tls_offset(id: raven_libc_elf::ObjId, offset: usize) {
    if let Some(slot) = TLS_OFFSETS.get(usize::try_from(id.0).unwrap_or(usize::MAX)) {
        slot.store(offset, Ordering::Release);
    }
}

/// Record the top of the mapped address space.
pub fn set_limit(top: usize) {
    LIMIT.store(top, Ordering::Relaxed);
}

/// A `MemView` over live memory.
pub struct LiveMem;

impl MemView for LiveMem {
    fn read(&self, vaddr: u64, len: usize) -> Option<&[u8]> {
        if len == 0 {
            return Some(&[]);
        }
        let start = usize::try_from(vaddr).ok()?;
        let end = start.checked_add(len)?;
        let limit = LIMIT.load(Ordering::Relaxed);
        if limit != 0 && end > limit {
            return None;
        }
        // SAFETY: the loader only asks for ranges C52's digest accepted from a PT_LOAD; the
        // limit check above rejects a range above the reserved image.
        Some(unsafe { core::slice::from_raw_parts(start as *const u8, len) })
    }

    fn cstr(&self, vaddr: u64) -> Option<&[u8]> {
        let start = usize::try_from(vaddr).ok()?;
        let limit = LIMIT.load(Ordering::Relaxed);
        let max = if limit > start {
            limit.saturating_sub(start).min(1 << 20)
        } else {
            1 << 20
        };
        let mut length = 0usize;
        while length < max {
            // SAFETY: the loader asks only for strings inside a validated object; the bound
            // stops the scan at the end of the user address space (or 1 MiB).
            if unsafe { *(start as *const u8).add(length) } == 0 {
                // SAFETY: the bytes scanned above are readable; the slice ends before the NUL.
                return Some(unsafe { core::slice::from_raw_parts(start as *const u8, length) });
            }
            length = length.saturating_add(1);
        }
        None
    }
}

/// The current thread's TLS base (`%fs:0`, `TPIDR_EL0` or `tp`).
fn thread_pointer() -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        let value: usize;
        // SAFETY: `%fs:0` is the current thread's TCB pointer.
        unsafe {
            core::arch::asm!(
                "mov {value}, qword ptr fs:[0]",
                value = out(reg) value,
                options(nostack, preserves_flags)
            );
        }
        value
    }
    #[cfg(target_arch = "aarch64")]
    {
        let value: usize;
        // SAFETY: `TPIDR_EL0` is the current thread's TLS base.
        unsafe {
            core::arch::asm!("mrs {}, tpidr_el0", out(reg) value, options(nostack));
        }
        value
    }
    #[cfg(target_arch = "riscv64")]
    {
        let value: usize;
        // SAFETY: the thread pointer register is the TLS base.
        unsafe {
            core::arch::asm!("mv {}, tp", out(reg) value, options(nostack));
        }
        value
    }
}

/// A `RelocApplier` over live memory.
pub struct LiveApplier;

impl RelocApplier for LiveApplier {
    fn write64(&mut self, addr: u64, val: u64) -> Result<(), ApplyError> {
        let address = usize::try_from(addr).map_err(|_| ApplyError::OutOfRange)?;
        // SAFETY: `apply_object` validated the relocation offset against the PT_LOAD.
        unsafe {
            (address as *mut u64).write_unaligned(val);
        }
        Ok(())
    }

    fn write32(&mut self, addr: u64, val: u32) -> Result<(), ApplyError> {
        let address = usize::try_from(addr).map_err(|_| ApplyError::OutOfRange)?;
        // SAFETY: `apply_object` validated the relocation offset against the PT_LOAD.
        unsafe {
            (address as *mut u32).write_unaligned(val);
        }
        Ok(())
    }

    fn read64(&self, addr: u64) -> Option<u64> {
        let address = usize::try_from(addr).ok()?;
        // SAFETY: `apply_object` validated the relocation offset against the PT_LOAD.
        Some(unsafe { (address as *const u64).read_unaligned() })
    }

    fn call_ifunc(
        &mut self,
        resolver: u64,
        _arg: &raven_libc_elf::IfuncArg,
    ) -> Result<u64, ApplyError> {
        let address = usize::try_from(resolver).map_err(|_| ApplyError::BadIfunc)?;
        // SAFETY: the resolver is an STT_GNU_IFUNC definition in a validated executable segment.
        let result = unsafe { core::mem::transmute::<usize, extern "C" fn() -> usize>(address)() };
        u64::try_from(result).map_err(|_| ApplyError::BadIfunc)
    }

    fn tls_module_id(&self, obj: raven_libc_elf::ObjId) -> u64 {
        u64::from(obj.0).saturating_add(1)
    }

    fn tls_static_offset(&mut self, obj: raven_libc_elf::ObjId) -> Result<i64, ApplyError> {
        let offset = TLS_OFFSETS
            .get(usize::try_from(obj.0).unwrap_or(usize::MAX))
            .map_or(0, |slot| slot.load(Ordering::Acquire));
        i64::try_from(offset).map_err(|_| ApplyError::OutOfRange)
    }

    fn set_tls_offset(&mut self, obj: raven_libc_elf::ObjId, offset: usize) {
        crate::raw::memview::set_tls_offset(obj, offset);
    }

    fn init_static_tls(
        &mut self,
        obj: raven_libc_elf::ObjId,
        image: u64,
        filesz: u64,
    ) -> Result<(), ApplyError> {
        if image == 0 || filesz == 0 {
            return Ok(());
        }
        let offset = self.tls_static_offset(obj)?;
        if offset == 0 {
            return Ok(());
        }
        let thread_pointer = thread_pointer();
        if thread_pointer == 0 {
            return Ok(());
        }
        let offset = usize::try_from(offset).map_err(|_| ApplyError::OutOfRange)?;
        let block = raven_libc_tls::layout::block_start(
            raven_libc_tls::consts::TARGET_ABI,
            thread_pointer,
            offset,
        );
        let len = usize::try_from(filesz).map_err(|_| ApplyError::OutOfRange)?;
        // SAFETY: `image` is this module's mapped TLS template and `filesz` is its validated
        // initialised size; `block` is the current thread's reserved static TLS block at the
        // offset the loader just assigned. No other thread shares this TLS block.
        unsafe {
            core::ptr::copy_nonoverlapping(image as *const u8, block as *mut u8, len);
        }
        Ok(())
    }

    fn tls_desc_alloc(
        &mut self,
        obj: raven_libc_elf::ObjId,
        sym_value: u64,
        addend: i64,
    ) -> Result<raven_libc_elf::TlsDesc, ApplyError> {
        #[cfg(target_arch = "aarch64")]
        {
            let index = TLS_DESC_NEXT.fetch_add(1, Ordering::Relaxed);
            let slot = TLS_DESC_INDICES.get(index).ok_or(ApplyError::OutOfRange)?;
            let module =
                usize::try_from(self.tls_module_id(obj)).map_err(|_| ApplyError::OutOfRange)?;
            let offset = usize::try_from(sym_value.wrapping_add_signed(addend))
                .map_err(|_| ApplyError::OutOfRange)?;
            slot.module.store(module, Ordering::Release);
            slot.offset.store(offset, Ordering::Release);
            return Ok(raven_libc_elf::TlsDesc {
                resolver: __rl_tlsdesc_dynamic as *const () as u64,
                arg: core::ptr::addr_of!(slot.module) as u64,
            });
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            let offset = self.tls_static_offset(obj)?;
            let arg =
                crate::tls::desc_argument(crate::arch::target_arch(), sym_value, addend, offset);
            Ok(raven_libc_elf::TlsDesc {
                resolver: __rl_tlsdesc_return as *const () as u64,
                arg,
            })
        }
    }

    fn copy(&mut self, dst: u64, src: u64, len: u64) -> Result<(), ApplyError> {
        let dst = usize::try_from(dst).map_err(|_| ApplyError::BadCopy)?;
        let src = usize::try_from(src).map_err(|_| ApplyError::BadCopy)?;
        let len = usize::try_from(len).map_err(|_| ApplyError::BadCopy)?;
        // SAFETY: both ranges were validated by the relocation walk.
        unsafe {
            core::ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, len);
        }
        Ok(())
    }
}
