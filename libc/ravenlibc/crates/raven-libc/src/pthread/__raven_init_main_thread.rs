// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `__raven_init_main_thread`: build the initial TCB/TLS block for the C61 start path.
//!
//! The function is deliberately small and always linked: on x86_64 the stack protector reads
//! `%fs:0x28` in every protected function. C57 owns the layout and the TCB; this file only
//! parses the auxiliary vector/program headers and passes the executable's `PT_TLS` module.

use core::mem::size_of;

use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::init_main_thread;
use raven_libc_tls::TlsModuleIn;
use raven_libc_tls::tcb::{Pthread, tp_to_pthread};

const AT_NULL: usize = 0;
const AT_PHDR: usize = 3;
const AT_PHENT: usize = 4;
const AT_PHNUM: usize = 5;
const PT_NULL: u32 = 0;
const PT_PHDR: u32 = 6;
const PT_TLS: u32 = 7;
const MAX_AUXV_PAIRS: usize = 128;
const MAX_PHDRS: usize = 128;

/// The ELF64 program-header shape used by the loader ABI.
#[repr(C)]
struct ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

/// Read one `AT_*` value from the key/value array `start.rs` passes.
///
/// # Safety
/// `auxv` is the C8 auxiliary array, terminated by an `AT_NULL` pair.
unsafe fn auxv_value(auxv: *const usize, key: usize) -> Option<usize> {
    if auxv.is_null() {
        return None;
    }
    let mut index = 0usize;
    while index < MAX_AUXV_PAIRS {
        // SAFETY: the caller contract is the terminated auxiliary array; the index is bounded.
        let pair_key = unsafe { *auxv.add(index.wrapping_mul(2)) };
        // SAFETY: the value is the second word of the same pair.
        let pair_value = unsafe { *auxv.add(index.wrapping_mul(2).wrapping_add(1)) };
        if pair_key == AT_NULL {
            break;
        }
        if pair_key == key {
            return Some(pair_value);
        }
        index = index.wrapping_add(1);
    }
    None
}

/// Find the runtime address of the `PT_TLS` image, or `None` when the executable has none.
///
/// # Safety
/// The program-header table described by the three auxiliary values is mapped and readable.
unsafe fn tls_module(phdr: usize, phent: usize, phnum: usize) -> Option<TlsModuleIn> {
    if phdr == 0 || phent < size_of::<ProgramHeader>() || phnum == 0 || phnum > MAX_PHDRS {
        return None;
    }
    let table = phdr as *const u8;
    let mut base = 0usize;
    let mut index = 0usize;
    while index < phnum {
        let offset = index.wrapping_mul(phent);
        // SAFETY: the table has `phnum` entries of `phent` bytes each, and the bounds above
        // guarantee that the fixed ELF64 header fits.
        let entry = unsafe { &*(table.add(offset) as *const ProgramHeader) };
        if entry.p_type == PT_PHDR {
            base = phdr.wrapping_sub(entry.p_vaddr as usize);
            break;
        }
        index = index.wrapping_add(1);
    }
    index = 0;
    while index < phnum {
        let offset = index.wrapping_mul(phent);
        // SAFETY: as above.
        let entry = unsafe { &*(table.add(offset) as *const ProgramHeader) };
        if entry.p_type == PT_TLS {
            return Some(TlsModuleIn::new(
                base.wrapping_add(entry.p_vaddr as usize),
                entry.p_filesz as usize,
                entry.p_memsz as usize,
                entry.p_align as usize,
            ));
        }
        index = index.wrapping_add(1);
    }
    let _ = PT_NULL;
    None
}

/// Initialise the main thread's TCB and TLS image.
///
/// # Safety
/// Called once by `__raven_libc_start`; `auxv` is the terminated C8 auxiliary array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_init_main_thread(auxv: *const usize, canary: usize) {
    // SAFETY: the caller passes the start path's auxiliary vector.
    let phdr = unsafe { auxv_value(auxv, AT_PHDR) };
    // SAFETY: as above.
    let phent = unsafe { auxv_value(auxv, AT_PHENT) };
    // SAFETY: as above.
    let phnum = unsafe { auxv_value(auxv, AT_PHNUM) };
    let module = match (phdr, phent, phnum) {
        (Some(phdr), Some(phent), Some(phnum)) => {
            // SAFETY: the auxiliary values describe the mapped program-header table.
            unsafe { tls_module(phdr, phent, phnum) }
        }
        _ => None,
    };
    let modules = module.into_iter().collect::<heapless_vec::Vec1>();
    let kernel = SysKernel::new();
    let Ok(tp) = init_main_thread(&kernel, modules.as_slice(), 0, 0) else {
        return;
    };
    let tcb = tp_to_pthread(tp) as *mut Pthread;
    if !tcb.is_null() {
        // SAFETY: `init_main_thread` returned the TCB address it just built for this thread.
        unsafe { (*tcb).canary = canary };
    }
}

/// A one-element stack vector; avoids pulling `alloc` into the start path.
mod heapless_vec {
    use raven_libc_tls::TlsModuleIn;

    /// At most one module (the executable's `PT_TLS`).
    pub(crate) struct Vec1 {
        value: Option<TlsModuleIn>,
    }

    impl Vec1 {
        pub(crate) fn as_slice(&self) -> &[TlsModuleIn] {
            match &self.value {
                Some(value) => core::slice::from_ref(value),
                None => &[],
            }
        }
    }

    impl FromIterator<TlsModuleIn> for Vec1 {
        fn from_iter<I: IntoIterator<Item = TlsModuleIn>>(iter: I) -> Self {
            Self {
                value: iter.into_iter().next(),
            }
        }
    }
}
