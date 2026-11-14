// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Process start: `__raven_libc_start`, the C8 stack parse and the fifteen ordered steps of
//! `donor-map/10` 3.12.2, costed by `donor-map/16` 3.A.9.1.
//!
//! The entry objects (`crt1.o`, `Scrt1.o`, `rcrt1.o`) call this one function with four
//! arguments (C61 4.2, RD-new 1):
//!
//! * `sp0`: the C8 stack (`CONTRACTS.md` C8);
//! * `rtld_fini`: the loader exit hook, or 0 (static and static-PIE);
//! * `iplt_start`, `iplt_end`: the static `IRELATIVE` range, or 0, 0.
//!
//! The function is split into a stage 1 that owns the parsed auxiliary vector and a stage 2
//! that runs the constructors and `main`, so the auxv frame does not stay live for the life of
//! the program (musl `__libc_start_main.c:77-97`).
//!
//! No work beyond the fifteen steps happens here (`BP-1`): no allocation, no file access, no
//! environment lookup, no locale, no stdio. Everything else starts lazily.

use core::ffi::{c_int, c_void};

use core::sync::atomic::{AtomicUsize, Ordering};

/// Maximum accepted `argc`; a larger word is a malformed C8 stack.
const MAX_ARGC: usize = 4096;
/// Maximum accepted environment entries; a larger array is malformed.
const MAX_ENV: usize = 4096;

/// `AT_EXECFN`: the executed path string.
const AT_EXECFN: usize = 31;
/// `AT_PAGESZ`: the page size.
const AT_PAGESZ: usize = 6;
/// `AT_CLKTCK`: clock ticks per second.
const AT_CLKTCK: usize = 17;
/// `AT_RANDOM`: 16 random bytes.
const AT_RANDOM: usize = 25;
/// `AT_SECURE`: the privileged-exec flag (never set on Raven, H-7).
const AT_SECURE: usize = 23;
/// `AT_HWCAP`: CPU feature bits for the AArch64 and RISC-V IFUNC argument.
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
const AT_HWCAP: usize = 16;
/// `AT_HWCAP2`: the second CPU feature word.
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
const AT_HWCAP2: usize = 26;
/// `AT_RAVEN_ABI`: the ABI revision the spawner wrote (`PG-50`).
const AT_RAVEN_ABI: usize = 0x7006;
/// First Raven-private key; the eight slots live at `key - 0x7000 + 56`.
const AT_RAVEN_FIRST: usize = 0x7001;
/// Last Raven-private key of the first block.
const AT_RAVEN_LAST: usize = 0x7007;
/// The Raven block starts at this slot of the 64-entry table.
const RAVEN_SLOT_BASE: usize = 56;
/// `AT_NULL`.
const AT_NULL: usize = 0;

/// `DT_*` and `PT_*` values are not used here; the static IRELATIVE range is a table of
/// `Elf64_Rela` records with `R_*_IRELATIVE` type numbers.
#[cfg(target_arch = "x86_64")]
const REL_IRELATIVE: usize = 37;
#[cfg(target_arch = "aarch64")]
const REL_IRELATIVE: usize = 1032;
#[cfg(target_arch = "riscv64")]
const REL_IRELATIVE: usize = 58;

/// `_IFUNC_ARG_HWCAP`: the AArch64/RISC-V "second argument is an `ifunc_arg_t`" bit.
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
const IFUNC_ARG_HWCAP: usize = 1 << 62;

/// The stack values stage 1 parsed.
struct StartStack {
    argc: usize,
    argv: *const *const u8,
    envp: *const *const u8,
    auxv: *const usize,
}

/// The page size from `AT_PAGESZ`, default 4096 (`C54` has no startup block yet; C61 RD-new).
static PAGE_SIZE: AtomicUsize = AtomicUsize::new(4096);
/// `CLK_TCK` from `AT_CLKTCK`, default 100.
static CLK_TCK: AtomicUsize = AtomicUsize::new(100);
/// The `AT_RANDOM` pointer, or 0.
static RANDOM_POINTER: AtomicUsize = AtomicUsize::new(0);

/// The process page size recorded at start.
#[allow(dead_code)] // consumed by the platform and allocator crates as they gain startup wiring
pub(crate) fn page_size() -> usize {
    PAGE_SIZE.load(Ordering::Relaxed)
}

/// The process clock tick rate recorded at start.
#[allow(dead_code)] // consumed by the time crate as it gains startup wiring
pub(crate) fn clk_tck() -> usize {
    CLK_TCK.load(Ordering::Relaxed)
}

/// The `AT_RANDOM` pointer recorded at start, or 0.
#[allow(dead_code)] // consumed by the random and stack-protector crates
pub(crate) fn random_pointer() -> usize {
    RANDOM_POINTER.load(Ordering::Relaxed)
}

unsafe extern "C" {
    /// The one main-thread initialisation of C64 (`src/pthread/__raven_init_main_thread.rs`).
    fn __raven_init_main_thread(auxv: *const usize, canary: usize);

    /// The program entry point linked into the executable.
    fn main(argc: c_int, argv: *const *const u8, envp: *const *const u8) -> c_int;

    /// `yes`/`no` owner of the environment block; C41 provides the storage.
    static mut environ: *mut *mut u8;
    /// The full program name.
    static mut __progname_full: *const u8;
    /// The basename of the program name.
    static mut __progname: *const u8;
    /// The GNU full program name.
    static mut program_invocation_name: *const u8;
    /// The GNU basename of the program name.
    static mut program_invocation_short_name: *const u8;

    /// The weak no-op constructor (absent in the modern toolchain).
    fn _init();
}

// The constructor/finaliser symbols are linker-defined; `.weak` makes a missing array empty
// instead of a link error (the `A-ld-20` convention: an absent entry is 0, never called).
// `__rl_init_hook` reads the `_init` address in assembly: a Rust comparison of a weak
// function symbol is folded to "non-null" by the optimiser.
core::arch::global_asm!(
    ".weak __preinit_array_start",
    ".weak __preinit_array_end",
    ".weak __init_array_start",
    ".weak __init_array_end",
    ".weak _init",
);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".text",
    ".globl __rl_init_hook",
    ".type __rl_init_hook,@function",
    "__rl_init_hook:",
    "mov _init@GOTPCREL(%rip),%rax",
    "ret",
    options(att_syntax),
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".text",
    ".globl __rl_init_hook",
    ".type __rl_init_hook,%function",
    "__rl_init_hook:",
    "adrp x0, _init",
    "add x0, x0, :lo12:_init",
    "ret",
);

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    ".text",
    ".globl __rl_init_hook",
    ".type __rl_init_hook,%function",
    "__rl_init_hook:",
    "lla a0, _init",
    "ret",
);

unsafe extern "C" {
    /// The runtime address of the weak `_init`, or 0 when absent.
    fn __rl_init_hook() -> usize;
}

/// The one start function of every program.
///
/// # Safety
///
/// `sp0` must be the C8 stack pointer the kernel or `spawn` built and the two `iplt` words the
/// static `IRELATIVE` range of the link (or 0, 0). Only `crt1.o`, `Scrt1.o` and `rcrt1.o` call
/// this, with exactly those values.
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn __raven_libc_start(
    sp0: *const usize,
    rtld_fini: usize,
    iplt_start: usize,
    iplt_end: usize,
) -> ! {
    // STEP 1: parse the C8 stack; a broken argv terminator is a hard start error.
    // SAFETY: the entry object passes the C8 stack pointer; the parser bounds every read.
    let stack = match unsafe { parse_stack(sp0) } {
        Some(stack) => stack,
        None => {
            // SAFETY: DEBUG_WRITE is the kernel's diagnostic sink and the message is static.
            let _ = raven_libc_sys::sys::debug_write(b"libc: bad argv\n");
            raven_libc_sys::sys::exit(127)
        }
    };

    // STEP 2: `environ` is the environment array itself, never a copy.
    // SAFETY: the parser found the NUL-terminated environment array of the C8 stack.
    unsafe {
        environ = stack.envp as *mut *mut u8;
    }

    // STEP 3: fill the 64-slot auxiliary table on the stack; the Raven keys use the eight
    // slots at 56..64 (`key - 0x7000 + 56`).
    let mut aux = [0usize; 64];
    // SAFETY: the parser found the `(key, value)` pairs and their `(0, 0)` terminator.
    unsafe {
        fill_auxv(stack.auxv, &mut aux);
    }

    // STEP 4: the five program-name aliases point into `argv`; no string is copied.
    // SAFETY: `argv[0]` is a NUL-terminated C8 string when `argc > 0`, and `AT_EXECFN` is one
    // otherwise; the fallback is a static empty string.
    unsafe {
        set_program_names(&stack, &aux);
    }

    // STEP 5: page size, clock ticks and the random pointer, in that order.
    let pagesz = if aux_value(&aux, AT_PAGESZ) == 0 {
        4096
    } else {
        aux_value(&aux, AT_PAGESZ)
    };
    PAGE_SIZE.store(pagesz, Ordering::Relaxed);
    CLK_TCK.store(
        if aux_value(&aux, AT_CLKTCK) == 0 {
            100
        } else {
            aux_value(&aux, AT_CLKTCK)
        },
        Ordering::Relaxed,
    );
    RANDOM_POINTER.store(aux_value(&aux, AT_RANDOM), Ordering::Relaxed);
    let mut canary = 0usize;
    if aux_value(&aux, AT_RANDOM) != 0 {
        // SAFETY: `AT_RANDOM` names 16 readable bytes on the initial stack.
        canary = unsafe { *(aux_value(&aux, AT_RANDOM) as *const usize) };
    }

    // STEP 6: a static non-PIE executable owns its `IRELATIVE` range; `Scrt1` and `rcrt1` pass
    // 0, 0 because the loader or the self-relocator already applied it.
    // SAFETY: the linker produced the range and `ld.lld` defines the two bounds.
    unsafe {
        apply_static_irelative(iplt_start, iplt_end, &aux);
    }

    // STEP 7: the main thread; after this call `errno` and TLS work (C64).
    // SAFETY: `auxv` is the C8 auxiliary array and `canary` its first `AT_RANDOM` word.
    unsafe {
        __raven_init_main_thread(stack.auxv, canary);
    }

    // STEP 8: the global stack-protector word (a plain store; no entropy syscall, R-17).
    // The shell owns the symbol; this is the once-per-process write of the start path.
    crate::stack_guard::set(canary);

    // STEP 9: the Raven ABI revision; absent is accepted (a static first process), a mismatch
    // is a hard error with a fixed diagnostic.
    if aux_value(&aux, raven_slot(AT_RAVEN_ABI)) != 0
        && aux_value(&aux, raven_slot(AT_RAVEN_ABI)) != raven_libc_sys::ABI_VERSION as usize
    {
        // SAFETY: DEBUG_WRITE writes the static message to the kernel diagnostic sink.
        let _ = raven_libc_sys::sys::debug_write(b"libc: bad ABI\n");
        raven_libc_sys::sys::exit(127)
    }

    // STEP 10: only `AT_SECURE` needs the standard descriptors; Raven never sets it (H-7).
    if aux_value(&aux, AT_SECURE) != 0 {
        ensure_standard_fds();
    }

    // STEP 11: stdio, locale, time zone, resolver, nss, the allocator and regex are lazy:
    // nothing runs here (`BP-1`, `R-17`, `R-19`, `R-28`).

    // Steps 12-15 run in stage 2 so this frame, with the auxiliary table, is gone before the
    // program's constructors and `main`.
    // SAFETY: the C8 arrays outlive the process and `rtld_fini` is the loader's hook or 0.
    unsafe { stage2(stack.argc, stack.argv, stack.envp, rtld_fini) }
}

/// Install every strong hook of the shell.
///
/// `__raven_libc_start` calls this once before the constructors. Programs that bring their own
/// `_start` (the hostrun fixtures) call it directly. The hook cells are function pointers, so no
/// allocation, file access or environment lookup happens here (`BP-1`, `R-17`).
#[unsafe(no_mangle)]
pub extern "C" fn __raven_libc_init() {
    crate::hooks::install_all();
}

/// Steps 12-15: constructors, `errno`, `main`, `exit`.
///
/// # Safety
///
/// `argv` and `envp` come from the C8 stack parse and stay mapped for the process lifetime.
#[inline(never)]
unsafe fn stage2(
    argc: usize,
    argv: *const *const u8,
    envp: *const *const u8,
    rtld_fini: usize,
) -> ! {
    // STEP 12: the constructor arrays run below; the step-13 registration is made first so the
    // finaliser is the oldest exit entry and therefore runs last (10-13).
    // STEP 13: register the finaliser: the `.fini_array` runner for a static executable, or
    // the loader's exit hook when the loader passed one (G-12). The shell registers through its
    // own registry; `__cxa_atexit` remains the C ABI entry for compiler-generated objects.
    // The strong exit-handler hook of C60's weak cell is installed first: without it the cell
    // keeps its no-op dummy and `exit` would skip every registered handler.
    __raven_libc_init();
    if rtld_fini == 0 {
        // The `__rl_fini_array` callback has the C `__cxa_atexit` shape; the registry is the
        // shell's process instance and accepts a NULL object handle.
        let _ = crate::exit_registry::register_cxa(
            Some(crate::hooks::fini_array::__rl_fini_array),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
    } else {
        // SAFETY: the loader passes a `void (*)(void)`; the callback is never called with the
        // argument word, so the cast is the psABI convention of `crt1`.
        let hook: extern "C" fn(*mut c_void) = unsafe { core::mem::transmute(rtld_fini) };
        let _ = crate::exit_registry::register_cxa(
            Some(hook),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
    }
    // STEP 12 (continued): `.preinit_array`, `_init` and `.init_array`, each entry called as
    // `fn(argc, argv, envp)`; entries 0, 1 and -1 are placeholders (A-ld-20).
    // SAFETY: the linker symbols bound the arrays and every entry came from a constructor
    // section of this executable; the weak `_init` is 0 when absent.
    unsafe {
        run_constructor_arrays(argc, argv, envp);
    }

    // STEP 14: the initial `errno` value is zero.
    raven_libc_sys::errno::set_errno(0);

    // STEP 15: run `main` and leave through the stdlib `exit`.
    // SAFETY: `main` is the program's entry point with the C signature, and `argv`/`envp` are
    // the C8 arrays.
    let code = unsafe { main(argc as c_int, argv, envp) };
    crate::stdlib::exit::exit(code)
}

/// Parse the C8 stack at `sp0`; `None` means a malformed stack.
///
/// # Safety
///
/// `sp0` must point at a readable C8 stack or be null/misaligned.
unsafe fn parse_stack(sp0: *const usize) -> Option<StartStack> {
    if sp0.is_null() || !(sp0 as usize).is_multiple_of(core::mem::align_of::<usize>()) {
        return None;
    }
    // SAFETY: `sp0` is non-null and aligned and names the first stack word.
    let argc = unsafe { *sp0 };
    if argc > MAX_ARGC {
        return None;
    }
    let argv = sp0.wrapping_add(1) as *const *const u8;
    // SAFETY: the C8 layout has `argc` argv words followed by the NULL terminator.
    if !unsafe { *argv.add(argc) }.is_null() {
        return None;
    }
    let envp = argv.wrapping_add(argc.wrapping_add(1));
    let mut count = 0usize;
    while count < MAX_ENV {
        // SAFETY: the environment array is NUL-terminated; the scan stops at that word.
        if unsafe { *envp.add(count) }.is_null() {
            break;
        }
        count = count.wrapping_add(1);
    }
    if count == MAX_ENV {
        return None;
    }
    let auxv = envp.wrapping_add(count.wrapping_add(1)) as *const usize;
    Some(StartStack {
        argc,
        argv,
        envp,
        auxv,
    })
}

/// Zero the table and copy every `(key, value)` pair the self-relocator and start need.
///
/// # Safety
///
/// `auxv` must be the C8 auxiliary array terminated by `(0, 0)`.
unsafe fn fill_auxv(auxv: *const usize, table: &mut [usize; 64]) {
    let mut index = 0usize;
    loop {
        // SAFETY: the auxv array is a terminated sequence of pairs.
        let key = unsafe { *auxv.add(index) };
        if key == AT_NULL {
            break;
        }
        // SAFETY: the value word follows the key.
        let value = unsafe { *auxv.add(index.wrapping_add(1)) };
        if key < table.len() {
            if let Some(slot) = table.get_mut(key) {
                *slot = value;
            }
        } else if (AT_RAVEN_FIRST..=AT_RAVEN_LAST).contains(&key) {
            let slot = key.wrapping_sub(0x7000).wrapping_add(RAVEN_SLOT_BASE);
            if let Some(cell) = table.get_mut(slot) {
                *cell = value;
            }
        }
        index = index.wrapping_add(2);
    }
}

/// One table value; absent keys read as 0.
fn aux_value(table: &[usize; 64], key: usize) -> usize {
    match table.get(key) {
        Some(value) => *value,
        None => 0,
    }
}

/// The table slot of a Raven key (`0x7001..=0x7007` -> 57..=63).
fn raven_slot(key: usize) -> usize {
    key.wrapping_sub(0x7000).wrapping_add(RAVEN_SLOT_BASE)
}

/// Set the five program-name aliases from `argv[0]`, else `AT_EXECFN`, else `""`.
///
/// # Safety
///
/// The chosen pointer must name a NUL-terminated C8 string or be 0.
unsafe fn set_program_names(stack: &StartStack, aux: &[usize; 64]) {
    static EMPTY: [u8; 1] = [0];
    let mut name = EMPTY.as_ptr();
    if stack.argc > 0 {
        // SAFETY: `argc > 0`, so `argv[0]` is a readable C8 string pointer.
        let candidate = unsafe { *stack.argv };
        if !candidate.is_null() {
            name = candidate;
        }
    } else if aux_value(aux, AT_EXECFN) != 0 {
        name = aux_value(aux, AT_EXECFN) as *const u8;
    }
    // SAFETY: `name` is NUL-terminated by the C8 contract.
    let short = unsafe { basename(name) };
    // SAFETY: the shell owns these symbols and start runs once.
    unsafe {
        __progname_full = name;
        __progname = short;
        program_invocation_name = name;
        program_invocation_short_name = short;
    }
}

/// The basename of a NUL-terminated path (a pointer into the same string).
///
/// # Safety
///
/// `name` must point at a readable NUL-terminated string.
unsafe fn basename(name: *const u8) -> *const u8 {
    let mut last = name;
    let mut cursor = name;
    loop {
        // SAFETY: the string is NUL-terminated by the C8 contract.
        let byte = unsafe { *cursor };
        if byte == 0 {
            break;
        }
        if byte == b'/' {
            last = cursor.wrapping_add(1);
        }
        cursor = cursor.wrapping_add(1);
    }
    last
}

/// Apply the static `IRELATIVE` range of a non-PIE executable.
///
/// # Safety
///
/// `start..end` must be the `Elf64_Rela` range `ld.lld` produced for this link, or `0, 0`.
unsafe fn apply_static_irelative(start: usize, end: usize, aux: &[usize; 64]) {
    #[cfg(target_arch = "x86_64")]
    let _ = aux;
    if start == 0 || end <= start {
        return;
    }
    let mut address = start;
    while address < end {
        // SAFETY: the range is an `Elf64_Rela` array and `address` is one entry inside it.
        let r_offset = unsafe { *(address as *const usize) };
        // SAFETY: `r_info` is the second word of the entry.
        let r_info = unsafe { *((address.wrapping_add(8)) as *const usize) };
        // SAFETY: `r_addend` is the third word of the entry.
        let r_addend = unsafe { *((address.wrapping_add(16)) as *const usize) };
        if r_info & 0x7fff_ffff == REL_IRELATIVE {
            let where_ = r_offset as *mut usize;
            #[cfg(target_arch = "x86_64")]
            {
                // SAFETY: a static non-PIE link stores the absolute resolver address; the
                // x86-64 psABI calls an IRELATIVE resolver with no arguments.
                let resolver: extern "C" fn() -> usize = unsafe { core::mem::transmute(r_addend) };
                let value = resolver();
                // SAFETY: `r_offset` names a writable word of the image.
                unsafe {
                    *where_ = value;
                }
            }
            #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
            {
                #[repr(C)]
                struct IfuncArg {
                    size: u64,
                    hwcap: u64,
                    hwcap2: u64,
                    hwcap3: u64,
                    hwcap4: u64,
                }
                let hwcap = aux_value(&aux, AT_HWCAP) as u64;
                let argument = IfuncArg {
                    size: core::mem::size_of::<IfuncArg>() as u64,
                    hwcap,
                    hwcap2: aux_value(&aux, AT_HWCAP2) as u64,
                    hwcap3: 0,
                    hwcap4: 0,
                };
                // SAFETY: the psABI passes `(hwcap | _IFUNC_ARG_HWCAP, &ifunc_arg_t)` for a
                // static non-PIE IRELATIVE entry and the resolver is in the image.
                let resolver: extern "C" fn(usize, *const IfuncArg) -> usize =
                    unsafe { core::mem::transmute(r_addend) };
                let value = resolver(hwcap as usize | IFUNC_ARG_HWCAP, &argument);
                // SAFETY: `r_offset` names a writable word of the image.
                unsafe {
                    *where_ = value;
                }
            }
        }
        address = address.wrapping_add(24);
    }
}

/// Call `.preinit_array`, `_init` and `.init_array` in order with `(argc, argv, envp)`.
///
/// # Safety
///
/// The linker symbols bound the arrays; every entry must be a function of this executable.
unsafe fn run_constructor_arrays(argc: usize, argv: *const *const u8, envp: *const *const u8) {
    unsafe extern "C" {
        static __preinit_array_start: usize;
        static __preinit_array_end: usize;
        static __init_array_start: usize;
        static __init_array_end: usize;
    }
    let preinit_start = core::ptr::addr_of!(__preinit_array_start);
    let preinit_end = core::ptr::addr_of!(__preinit_array_end);
    // SAFETY: the linker bounds the `.preinit_array` section of this executable.
    unsafe {
        run_array(preinit_start, preinit_end, argc, argv, envp);
    }
    // SAFETY: the assembly helper reads the weak `_init` address without the optimiser's
    // non-null assumption; a zero result means the toolchain emitted no legacy constructor.
    if unsafe { __rl_init_hook() } != 0 {
        // SAFETY: a non-zero weak `_init` is the executable's own legacy constructor.
        unsafe {
            _init();
        }
    }
    let init_start = core::ptr::addr_of!(__init_array_start);
    let init_end = core::ptr::addr_of!(__init_array_end);
    // SAFETY: the linker bounds the `.init_array` section of this executable.
    unsafe {
        run_array(init_start, init_end, argc, argv, envp);
    }
}

/// One constructor array, skipping the `0`, `1` and `-1` placeholders (`A-ld-20`).
///
/// # Safety
///
/// `start..end` must be one linker-bounded constructor array.
unsafe fn run_array(
    start: *const usize,
    end: *const usize,
    argc: usize,
    argv: *const *const u8,
    envp: *const *const u8,
) {
    let mut cursor = start;
    while cursor < end {
        // SAFETY: `cursor` is inside the array bounded by `end`.
        let entry = unsafe { *cursor };
        if entry != 0 && entry != 1 && entry != usize::MAX {
            // SAFETY: a non-placeholder entry is a `fn(argc, argv, envp)` constructor.
            let constructor: extern "C" fn(c_int, *const *const u8, *const *const u8) =
                unsafe { core::mem::transmute(entry) };
            constructor(argc as c_int, argv, envp);
        }
        cursor = cursor.wrapping_add(1);
    }
}

/// Ensure descriptors 0, 1 and 2 are open (`AT_SECURE` only; never on Raven).
#[cold]
#[inline(never)]
fn ensure_standard_fds() {
    // The fd table is the platform crate's (C55); a closed standard descriptor is replaced by
    // `/dev/null`, exactly the rule of `donor-map/10` 3.12.2 step 10. This branch is only
    // reachable when `AT_SECURE` is set, which the Raven spawner never does (H-7).
    let platform = crate::ffi::platform::global();
    let mut fd = 0i32;
    while fd < 3 {
        if platform
            .fcntl(fd, raven_libc_abi::fcntl::F_GETFD, 0)
            .is_err()
        {
            let _ = platform.open(b"/dev/null", raven_libc_abi::fcntl::O_RDWR as u32, 0);
        }
        fd = fd.wrapping_add(1);
    }
}
