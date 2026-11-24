// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The exported loader symbols and the target entry glue.
//!
//! `_rtld_main` parses the C8 stack, self-relocates the loader, registers the already-mapped
//! main program and the loader itself, loads the `DT_NEEDED` closure, applies relocations and
//! hands control to `AT_ENTRY`. The `dl*` bodies forward to the one process-wide [`Loader`].

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use raven_libc_elf::{MemView, ObjId, ObjView};

use crate::dlfcn::{RTLD_DEFAULT, RTLD_NEXT};
use crate::loadobj::{LoadedObject, Loader, MAX_DEPS, MAX_OBJECTS, Name};
use crate::main_flow::register_main;
use crate::mapper::Mapped;
use crate::raw::applier::LiveApplier;
use crate::raw::mapper_real::RealPlatform;
use crate::raw::memview::LiveMem;
use crate::raw::{selfreloc, sys};
use crate::table::RtldTable;

/// The concrete target loader.
type TargetLoader = Loader<RealPlatform, LiveMem, LiveApplier>;

/// The loader's storage; it lives for the process lifetime and is written once at start.
struct LoaderCell(UnsafeCell<MaybeUninit<TargetLoader>>);

// SAFETY: only the single start path writes the cell and every later access is serialised by
// `LOADER_LOCK`.
unsafe impl Sync for LoaderCell {}

static CELL: LoaderCell = LoaderCell(UnsafeCell::new(MaybeUninit::uninit()));
static READY: AtomicBool = AtomicBool::new(false);
static LOADER_LOCK: AtomicBool = AtomicBool::new(false);

/// `AT_PHDR`.
const AT_PHDR: usize = 3;
/// `AT_PHENT`.
const AT_PHENT: usize = 4;
/// `AT_PHNUM`.
const AT_PHNUM: usize = 5;
/// `AT_BASE`.
const AT_BASE: usize = 7;
/// `AT_ENTRY`.
const AT_ENTRY: usize = 9;
/// `AT_EXECFN`.
const AT_EXECFN: usize = 31;
/// Initfs archive offset of the libc image (K139).
const AT_RAVEN_LIBC: usize = 0x7007;
/// The first `PT_LOAD` virtual address of the loader (`--image-base=0x1000`).
const SELF_IMAGE_BASE: usize = 0x1000;
/// The user address-space end.
const USER_END: usize = 0x0000_7FFF_FFFF_F000;

/// The start-stack values the loader needs.
struct StartAux<'a> {
    phdr: u64,
    phnum: u16,
    base: usize,
    entry: usize,
    exec_name: &'a [u8],
    libc_offset: u64,
}

fn fail(message: &[u8]) -> ! {
    sys::debug_write(message);
    sys::exit(127)
}

/// Read a NUL-terminated string from the start stack, bounded.
unsafe fn stack_string(mut address: usize) -> &'static [u8] {
    if address == 0 {
        return b"";
    }
    let mut length = 0usize;
    while length < 4096 {
        // SAFETY: the start stack is mapped and NUL-terminated at a small bound.
        if unsafe { *(address as *const u8) } == 0 {
            break;
        }
        address = address.wrapping_add(1);
        length = length.wrapping_add(1);
    }
    // SAFETY: the bytes were just scanned; the slice ends before the NUL.
    unsafe { core::slice::from_raw_parts((address - length) as *const u8, length) }
}

/// Parse the C8 stack and the auxiliary vector.
unsafe fn parse_start(sp: usize) -> Option<StartAux<'static>> {
    if sp == 0 || !sp.is_multiple_of(8) {
        return None;
    }
    // SAFETY: the kernel entered with the C8 stack pointer.
    let argc = unsafe { *(sp as *const usize) };
    if argc > 4096 {
        return None;
    }
    let argv = sp.checked_add(8)?;
    let envp = argv.checked_add((argc.saturating_add(1)).checked_mul(8)?)?;
    let mut cursor = envp;
    let mut env_count = 0usize;
    while env_count < 4096 {
        // SAFETY: the environment array is NUL-terminated.
        if unsafe { *(cursor as *const usize) } == 0 {
            break;
        }
        cursor = cursor.checked_add(8)?;
        env_count = env_count.saturating_add(1);
    }
    if env_count == 4096 {
        return None;
    }
    let mut aux = cursor.checked_add(8)?;
    let mut phdr = 0u64;
    let mut phent = 0usize;
    let mut phnum = 0usize;
    let mut base = 0usize;
    let mut entry = 0usize;
    let mut execfn = 0usize;
    let mut libc_offset = 0u64;
    let mut pairs = 0usize;
    loop {
        if pairs >= 128 {
            return None;
        }
        // SAFETY: the auxiliary vector is a terminated sequence of pairs.
        let key = unsafe { *(aux as *const usize) };
        // SAFETY: the value word follows the key.
        let value = unsafe { *((aux + 8) as *const usize) };
        if key == 0 {
            break;
        }
        match key {
            AT_PHDR => phdr = value as u64,
            AT_PHENT => phent = value,
            AT_PHNUM => phnum = value,
            AT_BASE => base = value,
            AT_ENTRY => entry = value,
            AT_EXECFN => execfn = value,
            AT_RAVEN_LIBC => libc_offset = value as u64,
            _ => {}
        }
        aux = aux.checked_add(16)?;
        pairs = pairs.saturating_add(1);
    }
    if phdr == 0 || phent != 56 || phnum == 0 || phnum > 128 || base == 0 || entry == 0 {
        return None;
    }
    let _ = phent;
    let argv0 = if argc > 0 {
        // SAFETY: `argc > 0` means the first argv word is present.
        unsafe { *(argv as *const usize) }
    } else {
        0
    };
    let name_pointer = if execfn != 0 { execfn } else { argv0 };
    // SAFETY: the chosen pointer is a NUL-terminated start-stack string.
    let exec_name = unsafe { stack_string(name_pointer) };
    Some(StartAux {
        phdr,
        phnum: u16::try_from(phnum).ok()?,
        base,
        entry,
        exec_name,
        libc_offset,
    })
}

fn loader() -> &'static mut TargetLoader {
    // SAFETY: the start path wrote the cell before publishing `READY`; the pointer stays valid.
    unsafe { (*CELL.0.get()).assume_init_mut() }
}

fn with_loader<R>(body: impl FnOnce(&mut TargetLoader) -> R) -> Option<R> {
    if !READY.load(Ordering::Acquire) {
        return None;
    }
    while LOADER_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let value = body(loader());
    LOADER_LOCK.store(false, Ordering::Release);
    Some(value)
}

#[inline(never)]
fn init_loader() -> &'static mut TargetLoader {
    // SAFETY: start runs once; no other access exists before `READY` is published.
    let cell = unsafe { &mut *CELL.0.get() };
    cell.write(Loader::new(RealPlatform, LiveMem, LiveApplier));
    READY.store(true, Ordering::Release);
    // SAFETY: the cell was just initialised.
    unsafe { cell.assume_init_mut() }
}

fn object_span(view: &ObjView, base: u64) -> u64 {
    view.segments
        .iter()
        .map(|segment| segment.vaddr.saturating_add(segment.memsz))
        .max()
        .unwrap_or(base)
        .saturating_sub(base)
}

/// Rewrite the address-valued `DT_*` entries of the main object to absolute addresses.
///
/// `dl_iterate_phdr` consumers (including the C73 dynamic case) read `DT_STRTAB` and
/// `DT_NEEDED` out of the mapped dynamic section and add only the string offset; the loader
/// keeps its own copy in [`ObjView`], so this only changes the on-image view.
fn absolutize_dynamic(loader: &TargetLoader, id: ObjId) {
    let Some(object) = loader.object(id) else {
        return;
    };
    let dynamic = object.view.dynamic;
    let bias = object.view.bias;
    for index in 0..256usize {
        let Some(offset) = index.checked_mul(16) else {
            break;
        };
        let Some(address) = dynamic.checked_add(u64::try_from(offset).unwrap_or(u64::MAX)) else {
            break;
        };
        let Some(bytes) = loader.mem.read(address, 16) else {
            break;
        };
        let Ok(entry) = raven_libc_elf::Dyn::parse(bytes) else {
            break;
        };
        if entry.tag == 0 {
            break;
        }
        let is_address = matches!(
            entry.tag,
            3 | 4
                | 5
                | 6
                | 7
                | 12
                | 13
                | 17
                | 0x24
                | 0x25
                | 0x26
                | 0x32
                | 0x6ffffef5
                | 0x6ffffff0
                | 0x6ffffffc
                | 0x6ffffffe
        );
        if is_address && let Some(place) = address.checked_add(8) {
            // SAFETY: the dynamic section is in a writable PT_LOAD and `address` came from
            // the mapped view.
            unsafe {
                (place as *mut u64).write_unaligned(bias.wrapping_add(entry.val));
            }
        }
    }
}

fn register_loader_object(loader: &mut TargetLoader, base: usize) -> Option<ObjId> {
    let header = base.checked_add(SELF_IMAGE_BASE)?;
    // SAFETY: the loader's first load segment is mapped.
    let bytes = unsafe { core::slice::from_raw_parts(header as *const u8, 64) };
    let ehdr = raven_libc_elf::Ehdr::parse(bytes).ok()?;
    let phdr_vaddr = u64::try_from(SELF_IMAGE_BASE)
        .ok()?
        .checked_add(ehdr.e_phoff)?;
    let view = ObjView::digest(
        &loader.mem,
        phdr_vaddr,
        ehdr.e_phnum,
        u64::try_from(base).ok()?,
    )
    .ok()?;
    let bias = u64::try_from(base).ok()?;
    let span = object_span(&view, bias);
    let mut object = LoadedObject {
        rpath: view.rpath,
        runpath: view.runpath,
        view,
        mapped: Mapped {
            base: bias,
            span,
            bias,
        },
        names: [Name::empty(); crate::loadobj::MAX_NAMES],
        name_count: 0,
        deps: [ObjId(u32::MAX); MAX_DEPS],
        dep_count: 0,
        refcount: 1,
        nodelete: true,
        tls_offset: 0,
        tls_size: 0,
        tls_initialized: true,
        relocated: true,
        ifunc_done: true,
        fini_done: true,
        dl_open: false,
    };
    object.add_name(b"ld-elf.so.1");
    let id = loader.insert(object)?;
    loader.scope.rtld = Some(id);
    Some(id)
}

/// The `_rtld_start` target helper; the assembly stub passes the C8 stack pointer.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_main(sp: usize) -> ! {
    // SAFETY: the entry stub passes the kernel-built C8 stack.
    let aux = match unsafe { parse_start(sp) } {
        Some(aux) => aux,
        None => fail(b"ld-elf.so.1: bad start stack\n"),
    };
    if selfreloc::relocate_self(aux.base).is_err() {
        fail(b"ld-elf.so.1: self-relocation failed\n");
    }
    crate::raw::memview::set_limit(USER_END);
    let loader = init_loader();
    if register_loader_object(loader, aux.base).is_none() {
        fail(b"ld-elf.so.1: cannot register the loader\n");
    }
    let main = match register_main(loader, aux.phdr, aux.phnum, aux.exec_name) {
        Ok(id) => id,
        Err(_) => fail(b"ld-elf.so.1: cannot register the main program\n"),
    };
    if aux.libc_offset != 0 && !crate::raw::libc::map_premapped(loader, aux.libc_offset) {
        sys::debug_write(b"ld-elf.so.1: libc pre-map failed; using the file server\n");
    }
    absolutize_dynamic(loader, main);
    if loader.load_needed_objects(main).is_err() {
        fail(b"ld-elf.so.1: cannot load a needed library\n");
    }
    if loader.assign_tls_offsets().is_err() {
        fail(b"ld-elf.so.1: TLS layout failed\n");
    }
    for index in 0..MAX_OBJECTS {
        let Ok(id) = u32::try_from(index) else {
            continue;
        };
        let id = ObjId(id);
        let Some(object) = loader.object(id) else {
            continue;
        };
        crate::raw::memview::set_tls_offset(id, object.tls_offset);
    }
    if let Err(error) = loader.relocate_all() {
        sys::debug_write(b"ld-elf.so.1: ");
        sys::debug_write(error.text().as_bytes());
        if !loader.error.as_bytes().is_empty() {
            sys::debug_write(b": ");
            sys::debug_write(loader.error.as_bytes());
        }
        sys::debug_write(b"\n");
        sys::exit(127)
    }
    if !loader.install_table(&TABLE) {
        fail(b"ld-elf.so.1: cannot install the dl table\n");
    }
    record_tls_modules(loader);
    // Dependency-first constructors run before the program entry; the main program's own
    // constructors are the libc start path's job.
    let order = loader.initial_order();
    for id in order.into_iter().flatten() {
        if Some(id) == loader.scope.rtld || id == loader.scope.main {
            continue;
        }
        let _ = loader.run_init(id);
    }
    // A bare TCB so `%fs:0` is valid before libc's start path runs; the libc initialiser
    // overwrites it with the real TCB when it succeeds.
    init_bootstrap_tls();
    // SAFETY: `sp` is the untouched C8 stack and `entry` is `AT_ENTRY` from the kernel.
    unsafe { crate::raw::arch::entry::enter(sp, aux.entry, 0) }
}

/// One dynamic TLS module template (`dlopen`ed objects are allocated lazily).
struct TlsModule {
    image: AtomicUsize,
    filesz: AtomicUsize,
    memsz: AtomicUsize,
    align: AtomicUsize,
    present: AtomicBool,
}

impl TlsModule {
    const fn new() -> Self {
        Self {
            image: AtomicUsize::new(0),
            filesz: AtomicUsize::new(0),
            memsz: AtomicUsize::new(0),
            align: AtomicUsize::new(1),
            present: AtomicBool::new(false),
        }
    }
}

static TLS_MODULES: [TlsModule; 16] = [const { TlsModule::new() }; 16];
static TLS_LOCK: AtomicBool = AtomicBool::new(false);

fn record_tls_modules(loader: &TargetLoader) {
    for index in 0..MAX_OBJECTS {
        let Ok(id) = u32::try_from(index) else {
            continue;
        };
        let Some(object) = loader.object(ObjId(id)) else {
            continue;
        };
        let Some(tls) = object.view.tls else {
            continue;
        };
        let module = id.saturating_add(1) as usize;
        if let Some(slot) = TLS_MODULES.get(module.wrapping_sub(1)) {
            slot.image.store(tls.vaddr as usize, Ordering::Release);
            slot.filesz
                .store(usize::try_from(tls.filesz).unwrap_or(0), Ordering::Release);
            slot.memsz
                .store(usize::try_from(tls.memsz).unwrap_or(0), Ordering::Release);
            slot.align.store(
                usize::try_from(tls.align).unwrap_or(1).max(1),
                Ordering::Release,
            );
            slot.present.store(true, Ordering::Release);
        }
    }
}

fn align_up(value: usize, align: usize) -> usize {
    let mask = align.max(1).wrapping_sub(1);
    value.wrapping_add(mask) & !mask
}

fn map_anon(len: usize) -> usize {
    let len = len
        .checked_add(4095)
        .map(|value| value & !4095)
        .unwrap_or(0);
    if len == 0 {
        return 0;
    }
    // SAFETY: MAP with the anonymous-object marker and the hint zero asks the kernel for a
    // private range.
    let result = unsafe { sys::syscall6(sys::MAP, 0, u32::MAX as usize, 0, 0, len, 0b11) };
    if result < 0 { 0 } else { result as usize }
}

/// Install a minimal TCB/DTV so `%fs:` is valid before libc's start path runs.
fn init_bootstrap_tls() {
    let tcb = map_anon(4096);
    let dtv = map_anon(16);
    if tcb == 0 || dtv == 0 {
        return;
    }
    // SAFETY: both mappings are private and writable; the TCB's first word is its self pointer
    // and the second is the DTV pointer (x86-64 `Pthread` ABI).
    unsafe {
        (tcb as *mut usize).write(tcb);
        *(dtv as *mut usize) = 0;
        *((tcb + 8) as *mut usize) = dtv;
    }
    // SAFETY: THREAD_SET_TLS with `Handle::NONE` targets the calling thread.
    let _ = unsafe { sys::syscall2(sys::THREAD_SET_TLS, u32::MAX as usize, tcb) };
}

fn allocate_tls_block(module: usize) -> Option<usize> {
    let slot = TLS_MODULES.get(module.checked_sub(1)?)?;
    if !slot.present.load(Ordering::Acquire) {
        return None;
    }
    let filesz = slot.filesz.load(Ordering::Acquire);
    let memsz = slot.memsz.load(Ordering::Acquire);
    let align = slot.align.load(Ordering::Acquire).max(1);
    let size = align_up(memsz, align);
    let total = align_up(size.checked_add(align)?, 4096);
    let mapping = map_anon(total);
    if mapping == 0 {
        return None;
    }
    let block = align_up(mapping, align);
    if filesz != 0 {
        // SAFETY: the module image is mapped for `filesz` bytes and the block is writable.
        unsafe {
            core::ptr::copy_nonoverlapping(
                slot.image.load(Ordering::Acquire) as *const u8,
                block as *mut u8,
                filesz.min(memsz),
            );
        }
    }
    Some(block)
}

/// Grow the calling thread's DTV and install `block` at `module`.
fn publish_dtv(thread_pointer: usize, module: usize, block: usize) -> bool {
    // x86-64 `struct Pthread` has `dtv` at offset 8; the DTV words are the musl flexible
    // array (word 0 is the module count).
    let slot = (thread_pointer + 8) as *mut usize;
    // SAFETY: `slot` is the current thread's live `dtv` field.
    let old = unsafe { *slot };
    let old_count = if old == 0 {
        0
    } else {
        // SAFETY: a published DTV points at word 0 of a live array.
        unsafe { *(old as *const usize) }
    };
    let count = old_count.max(module);
    let bytes = count
        .checked_add(1)
        .and_then(|words| words.checked_mul(8))
        .unwrap_or(0);
    if bytes == 0 {
        return false;
    }
    let new = map_anon(bytes);
    if new == 0 {
        return false;
    }
    // SAFETY: the new array has `count + 1` zeroed words.
    unsafe {
        core::ptr::write_bytes(new as *mut u8, 0, bytes);
        if old != 0 {
            core::ptr::copy_nonoverlapping(old as *const usize, new as *mut usize, old_count + 1);
        }
        *(new as *mut usize) = count;
        *((new as *mut usize).add(module)) = block;
        *slot = new;
    }
    true
}

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

/// `__tls_get_addr`.
#[unsafe(no_mangle)]
pub extern "C" fn __tls_get_addr(index: *mut usize) -> *mut u8 {
    if index.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the TLS descriptor holds `{ module, offset }`.
    let module = unsafe { *index };
    // SAFETY: the offset is the second word.
    let offset = unsafe { *index.add(1) };
    if module == 0 || module > TLS_MODULES.len() {
        return core::ptr::null_mut();
    }
    let tp = thread_pointer();
    if tp == 0 {
        return core::ptr::null_mut();
    }
    // SAFETY: `tp + 8` is the `dtv` field of the live TCB.
    let dtv = unsafe { *((tp + 8) as *const usize) };
    if dtv != 0 {
        // SAFETY: word 0 is the count and `module` is bounded by the table.
        let count = unsafe { *(dtv as *const usize) };
        if module <= count {
            // SAFETY: the DTV has `count` entries after word 0.
            let entry = unsafe { *((dtv as *const usize).add(module)) };
            if entry != 0 {
                return entry.wrapping_add(offset) as *mut u8;
            }
        }
    }
    while TLS_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    // Re-check under the lock: another thread may have grown this DTV.
    // SAFETY: `tp + 8` is the `dtv` field of the live TCB.
    let dtv = unsafe { *((tp + 8) as *const usize) };
    let mut result = core::ptr::null_mut();
    if dtv != 0 {
        // SAFETY: word 0 of a published DTV is its module count.
        let count = unsafe { *(dtv as *const usize) };
        if module <= count {
            // SAFETY: the DTV has `count` entries after word 0.
            let entry = unsafe { *((dtv as *const usize).add(module)) };
            if entry != 0 {
                result = entry.wrapping_add(offset) as *mut u8;
            }
        }
    }
    if result.is_null()
        && let Some(block) = allocate_tls_block(module)
        && publish_dtv(tp, module, block)
    {
        result = block.wrapping_add(offset) as *mut u8;
    }
    TLS_LOCK.store(false, Ordering::Release);
    result
}

fn handle_id(handle: *mut u8) -> Option<ObjId> {
    let value = handle as usize;
    if value == 0 {
        None
    } else if value == usize::MAX {
        Some(RTLD_NEXT)
    } else {
        u32::try_from(value.wrapping_sub(1)).ok().map(ObjId)
    }
}

fn encode_handle(id: ObjId) -> *mut u8 {
    (usize::try_from(id.0)
        .unwrap_or(usize::MAX - 1)
        .wrapping_add(1)) as *mut u8
}

fn name_bytes<'a>(name: *const u8) -> Option<&'a [u8]> {
    if name.is_null() {
        return None;
    }
    let mut length = 0usize;
    while length < 4096 {
        // SAFETY: the caller passes a NUL-terminated C string.
        if unsafe { *name.add(length) } == 0 {
            // SAFETY: the bytes were scanned; the slice ends before the NUL.
            return Some(unsafe { core::slice::from_raw_parts(name, length) });
        }
        length = length.saturating_add(1);
    }
    None
}

/// `dlopen`.
#[unsafe(no_mangle)]
pub extern "C" fn dlopen(name: *const u8, flags: i32) -> *mut u8 {
    let Some(name) = name_bytes(name) else {
        return core::ptr::null_mut();
    };
    match with_loader(|loader| {
        let existed = loader.by_name(name).is_some();
        match loader.dlopen(name, flags) {
            Ok(id) => {
                record_tls_modules(loader);
                if !existed {
                    let _ = loader.run_init(id);
                }
                encode_handle(id)
            }
            Err(_) => core::ptr::null_mut(),
        }
    }) {
        Some(value) => value,
        None => core::ptr::null_mut(),
    }
}

/// `dlclose`.
#[unsafe(no_mangle)]
pub extern "C" fn dlclose(handle: *mut u8) -> i32 {
    let Some(id) = handle_id(handle) else {
        return -1;
    };
    with_loader(|loader| loader.dlclose(id)).unwrap_or(-1)
}

/// `dlsym`.
#[unsafe(no_mangle)]
pub extern "C" fn dlsym(handle: *mut u8, name: *const u8, caller: usize) -> *mut u8 {
    let Some(name) = name_bytes(name) else {
        return core::ptr::null_mut();
    };
    let handle = if handle as usize == 0 {
        Some(RTLD_DEFAULT)
    } else {
        handle_id(handle)
    };
    with_loader(|loader| {
        loader
            .dlsym(handle, name, caller)
            .map_or(core::ptr::null_mut(), |value| value as *mut u8)
    })
    .unwrap_or(core::ptr::null_mut())
}

/// `dlvsym`.
#[unsafe(no_mangle)]
pub extern "C" fn dlvsym(
    handle: *mut u8,
    name: *const u8,
    version: *const u8,
    _caller: usize,
) -> *mut u8 {
    let Some(name) = name_bytes(name) else {
        return core::ptr::null_mut();
    };
    let Some(version) = name_bytes(version) else {
        return core::ptr::null_mut();
    };
    let handle = if handle as usize == 0 {
        Some(RTLD_DEFAULT)
    } else {
        handle_id(handle)
    };
    with_loader(|loader| {
        loader
            .dlvsym(handle, name, version)
            .map_or(core::ptr::null_mut(), |value| value as *mut u8)
    })
    .unwrap_or(core::ptr::null_mut())
}

/// `dlerror`.
#[unsafe(no_mangle)]
pub extern "C" fn dlerror() -> *const u8 {
    with_loader(|loader| {
        loader
            .dlerror()
            .map_or(core::ptr::null(), |bytes| bytes.as_ptr())
    })
    .unwrap_or(core::ptr::null())
}

/// `dladdr`.
#[unsafe(no_mangle)]
pub extern "C" fn dladdr(address: usize, info: *mut u8) -> i32 {
    if info.is_null() {
        return 0;
    }
    let Some(id) = with_loader(|loader| loader.dladdr(address)).flatten() else {
        return 0;
    };
    let Some((base, name)) = with_loader(|loader| {
        loader.object(id).map(|object| {
            (
                object.mapped.base as usize,
                object
                    .names
                    .first()
                    .map_or(core::ptr::null(), |name| name.as_bytes().as_ptr()),
            )
        })
    })
    .flatten() else {
        return 0;
    };
    // `Dl_info` is `{ fname, fbase, sname, saddr }`.
    // SAFETY: the caller supplies a writable `Dl_info` as four pointer-sized words.
    unsafe {
        let words = info as *mut usize;
        *words = name as usize;
        *words.add(1) = base;
        *words.add(2) = 0;
        *words.add(3) = 0;
    }
    1
}

/// `dlinfo`.
#[unsafe(no_mangle)]
pub extern "C" fn dlinfo(_handle: *mut u8, _request: i32, _argument: *mut u8) -> i32 {
    -1
}

/// `dl_iterate_phdr`.
#[unsafe(no_mangle)]
pub extern "C" fn dl_iterate_phdr(
    callback: Option<extern "C" fn(*mut u8, usize, *mut u8) -> i32>,
    data: *mut u8,
) -> i32 {
    let Some(callback) = callback else {
        return 0;
    };
    with_loader(|loader| {
        let mut result = 0;
        let ids = loader.iterate_ids();
        for id in ids.into_iter().flatten() {
            let Some(info) = loader.dl_phdr_info(id) else {
                continue;
            };
            let mut copy = info;
            let status = callback(
                &mut copy as *mut crate::phdr::DlPhdrInfo as *mut u8,
                core::mem::size_of::<crate::phdr::DlPhdrInfo>(),
                data,
            );
            if status != 0 {
                result = status;
                break;
            }
        }
        result
    })
    .unwrap_or(0)
}

/// `_rtld_error_str`.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_error_str() -> *const u8 {
    with_loader(|loader| loader.error.as_bytes().as_ptr()).unwrap_or(core::ptr::null())
}

/// `_rtld_allocate_tls`.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_allocate_tls(old: *mut u8, _new_size: usize) -> *mut u8 {
    old
}

/// `_rtld_free_tls`.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_free_tls(_ptr: *mut u8, _size: usize) -> i32 {
    0
}

/// `_rtld_get_stack_prot`.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_get_stack_prot() -> usize {
    0
}

/// `_rtld_set_dlerror_slot`.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_set_dlerror_slot(_slot: *mut u8) {}

/// `r_debug_state`.
#[unsafe(no_mangle)]
pub extern "C" fn r_debug_state() {}

/// `_r_debug_postinit`.
#[unsafe(no_mangle)]
pub extern "C" fn _r_debug_postinit() {}

/// `_rtld_version`.
#[unsafe(no_mangle)]
pub static _rtld_version: u32 = 1;

/// The installed forwarding table.
static TABLE: RtldTable = RtldTable {
    version: RtldTable::VERSION,
    size: RtldTable::SIZE,
    dlopen,
    dlclose,
    dlsym,
    dlvsym,
    dlerror,
    dladdr,
    dlinfo,
    dl_iterate_phdr,
};

/// The Raven private install hook (a libc export, not a loader export).
pub const INSTALL: &[u8] = b"__rl_dl_install";

/// Keep the unused public constants reachable for the export-set gate.
pub const RTLD_DEFAULT_ID: ObjId = RTLD_DEFAULT;
