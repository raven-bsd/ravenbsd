// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD first-stage direct-boot loader (S89).
//!
//! QEMU starts this ELF through its PVH option ROM. The 32-bit entry in `entry.S` builds a
//! temporary identity map, enters long mode and calls [`rust_entry`]. The Rust part consumes the
//! `-initrd` payload (header + kernel ELF + initfs), places the kernel image, converts the PVH
//! memory map into `raven_bootinfo::MemoryRegion`s, builds the final HHDM/kernel page tables,
//! fills `BootInfo` and jumps to the existing kernel `_start`.
//!
//! The UEFI loader and the kernel entry are untouched.

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![warn(missing_docs)]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::mem::size_of;
use core::panic::PanicInfo;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};

use raven_bootinfo::{
    BOOTINFO_MAGIC, BOOTINFO_VERSION, BootInfo, MemoryKind, MemoryRegion, PhysRange, arch,
};
use raven_direct::{
    ACPI_HEADER_SIZE, ACPI_RSDP_SIZE, HVM_START_MAGIC, MAX_MEMORY_MAP, MemmapEntry, Module,
    PAYLOAD_HEADER_SIZE, StartInfo, TrackedRange, append_mcfg, build_memory_map, find_free_range,
    parse_memmap_into, parse_module, parse_payload, parse_start_info,
};
use raven_elf::{Elf, Machine};
use raven_loader::layout::{HHDM_BASE, KERNEL_RANGE};
use raven_loader::paging::{NO_EXECUTE, PageTables, WRITABLE};
use raven_loader::paging_core::{PAGE_SIZE, PageError, PhysMem};

core::arch::global_asm!(include_str!("entry.S"), options(att_syntax));

/// Physical window the temporary identity map covers (first 4 GiB).
const IDENTITY_LIMIT: u64 = 0x1_0000_0000;
/// Largest PVH memory map QEMU's option ROM emits.
const MAX_E820: usize = 128;
/// Command-line buffer size (the kernel's own copy is 1 KiB).
const CMDLINE_MAX: usize = 4096;
/// Page-table frame pool. The kernel image uses 4 KiB leaves, so this is generous but small.
const POOL_SIZE: usize = 2 * 1024 * 1024;
/// Bump arena for `raven-loader`'s library code; the loader itself never allocates.
const ARENA_SIZE: usize = 256 * 1024;
/// ACPI tables the loader may synthesize (RSDP + XSDT + one MCFG).
const ACPI_BUFFER_SIZE: usize = 4096;

unsafe extern "C" {
    static __loader_start: u8;
    static __loader_data_start: u8;
    static __loader_end: u8;
}

#[repr(align(4096))]
struct PoolCell(UnsafeCell<[u8; POOL_SIZE]>);

// SAFETY: the pool is handed out once during single-CPU boot; the allocator zeroes every frame
// before it is published and no other thread exists.
unsafe impl Sync for PoolCell {}

static PAGE_POOL: PoolCell = PoolCell(UnsafeCell::new([0; POOL_SIZE]));

#[repr(align(16))]
struct ArenaCell(UnsafeCell<[u8; ARENA_SIZE]>);

// SAFETY: the bump pointer is atomic and the arena is only handed out monotonically.
unsafe impl Sync for ArenaCell {}

static ALLOC_ARENA: ArenaCell = ArenaCell(UnsafeCell::new([0; ARENA_SIZE]));
static ALLOC_NEXT: AtomicUsize = AtomicUsize::new(0);

#[repr(align(16))]
struct AcpiCell(UnsafeCell<[u8; ACPI_BUFFER_SIZE]>);

// SAFETY: the loader fills this buffer once before the handoff and the kernel only reads it;
// the page is marked AcpiReclaimable in the memory map.
unsafe impl Sync for AcpiCell {}

static ACPI_BUFFER: AcpiCell = AcpiCell(UnsafeCell::new([0; ACPI_BUFFER_SIZE]));

struct Bump;

// SAFETY: `alloc` hands out disjoint monotonic ranges inside `ALLOC_ARENA`; `dealloc` is a no-op
// because the loader lives for the whole boot. The arena is large enough for the library helpers.
unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(1);
        let mut current = ALLOC_NEXT.load(Ordering::Relaxed);
        loop {
            let Some(start) = current
                .checked_add(align - 1)
                .map(|value| value & !(align - 1))
            else {
                return ptr::null_mut();
            };
            let Some(end) = start.checked_add(layout.size()) else {
                return ptr::null_mut();
            };
            if end > ARENA_SIZE {
                return ptr::null_mut();
            }
            match ALLOC_NEXT.compare_exchange_weak(
                current,
                end,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // SAFETY: `start..end` is inside the static arena and was reserved by the
                    // compare-exchange above.
                    return unsafe { ALLOC_ARENA.0.get().cast::<u8>().add(start) };
                }
                Err(actual) => current = actual,
            }
        }
    }

    unsafe fn dealloc(&self, _pointer: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: Bump = Bump;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    fail("panic in the direct loader")
}

/// Rust entry called by `entry.S` after long mode is active.
///
/// QEMU passes the physical address of the PVH `hvm_start_info` in `%rdi`.
#[unsafe(no_mangle)]
pub extern "C" fn rust_entry(start_info_paddr: u64) -> ! {
    serial_init();
    serial_str("direct: loader start\n");

    let start_info = match read_start_info(start_info_paddr) {
        Some(info) => info,
        None => fail("cannot read hvm_start_info"),
    };
    if start_info.magic != HVM_START_MAGIC {
        fail("bad hvm_start_info magic");
    }
    if start_info.version != 1 {
        fail("unsupported PVH ABI version");
    }
    if start_info.nr_modules == 0 {
        fail("no initrd module (pass -initrd payload.bin)");
    }
    let module = match read_module(start_info.modlist_paddr) {
        Some(module) => module,
        None => fail("cannot read the PVH module list"),
    };
    if module.size == 0 {
        fail("empty initrd module");
    }

    let payload = match payload_slice(module) {
        Some(payload) => payload,
        None => fail("module is outside the identity window"),
    };
    let payload = match parse_payload(payload) {
        Ok(payload) => payload,
        Err(_) => fail("bad direct-boot payload header"),
    };
    if payload.initfs.is_empty() {
        fail("payload has no initfs; refusing to boot without services");
    }

    let elf = match Elf::parse(payload.kernel, Machine::X86_64, KERNEL_RANGE) {
        Ok(elf) => elf,
        Err(_) => fail("kernel ELF rejected by raven-elf"),
    };

    let mut kernel_virt_base = u64::MAX;
    let mut kernel_end = 0u64;
    for segment in elf.segments() {
        kernel_virt_base = kernel_virt_base.min(segment.vaddr);
        let segment_end = match segment.vaddr.checked_add(segment.memsz) {
            Some(end) => end,
            None => fail("kernel segment address overflow"),
        };
        kernel_end = kernel_end.max(segment_end);
    }
    if kernel_virt_base == u64::MAX || kernel_end <= kernel_virt_base {
        fail("kernel ELF has no loadable segments");
    }
    let kernel_span = match page_ceil(kernel_end - kernel_virt_base) {
        Some(size) if size != 0 => size,
        _ => fail("kernel image size overflow"),
    };

    let loader_start = (&raw const __loader_start) as u64;
    let loader_data_start = (&raw const __loader_data_start) as u64;
    let loader_end = (&raw const __loader_end) as u64;
    let avoid = [
        PhysRange {
            base: loader_start,
            len: loader_end.saturating_sub(loader_start),
        },
        PhysRange {
            base: module.paddr,
            len: module.size,
        },
    ];

    let mut e820 = [EMPTY_E820; MAX_E820];
    let e820_count = match read_e820(&start_info, &mut e820) {
        Some(count) => count,
        None => fail("cannot decode the PVH memory map"),
    };
    let entries = &e820[..e820_count];
    let kernel_phys = match find_free_range(entries, kernel_span, &avoid) {
        Some(base) => base,
        None => fail("no free physical range for the kernel image"),
    };

    if !copy_kernel(&elf, kernel_phys, kernel_virt_base, kernel_span) {
        fail("cannot place the kernel image");
    }

    let mut cmdline = [0u8; CMDLINE_MAX];
    let cmdline_len = read_cmdline(start_info.cmdline_paddr, &mut cmdline);
    if cmdline_len == cmdline.len() {
        fail("command line is not NUL terminated");
    }
    let acpi_rsdp = inject_mcfg(start_info.rsdp_paddr, &cmdline[..cmdline_len])
        .unwrap_or(start_info.rsdp_paddr);
    let acpi_buffer = (&raw const ACPI_BUFFER) as u64;

    let mut map = [EMPTY_REGION; MAX_MEMORY_MAP];
    let tracked = [
        TrackedRange {
            base: loader_start,
            len: loader_end.saturating_sub(loader_start),
            kind: MemoryKind::LoaderReclaimable,
        },
        TrackedRange {
            base: acpi_buffer,
            len: ACPI_BUFFER_SIZE as u64,
            kind: MemoryKind::AcpiReclaimable,
        },
        TrackedRange {
            base: kernel_phys,
            len: kernel_span,
            kind: MemoryKind::KernelAndModules,
        },
        TrackedRange {
            base: module.paddr,
            len: module.size,
            kind: MemoryKind::KernelAndModules,
        },
    ];
    let map_count = match build_memory_map(entries, &tracked, &mut map) {
        Ok(count) => count,
        Err(_) => fail("cannot build the kernel memory map"),
    };
    let map = &map[..map_count];

    if !module.paddr.is_multiple_of(PAGE_SIZE) {
        fail("the PVH module is not page aligned");
    }
    let initfs_base = module.paddr + PAYLOAD_HEADER_SIZE as u64 + payload.kernel.len() as u64;
    let initfs_len = payload.initfs.len() as u64;
    if !initfs_base.is_multiple_of(PAGE_SIZE) {
        fail("initfs is not page aligned; rebuild the payload with scripts/boot-direct.sh");
    }
    if initfs_base
        .checked_add(initfs_len)
        .is_none_or(|end| end > module.paddr + module.size)
    {
        fail("initfs range leaves the module");
    }

    serial_str("direct: bootinfo kernel_phys=");
    serial_hex(kernel_phys);
    serial_str(" size=");
    serial_hex(kernel_span);
    serial_str(" initfs=");
    serial_hex(initfs_base);
    serial_str(" len=");
    serial_hex(initfs_len);
    serial_str("\n");

    let mut boot_info = BootInfo::zeroed();
    boot_info.magic = BOOTINFO_MAGIC;
    boot_info.version = BOOTINFO_VERSION;
    boot_info.size = size_of::<BootInfo>() as u32;
    boot_info.arch = arch::X86_64;
    boot_info.hhdm_offset = HHDM_BASE;
    boot_info.kernel_phys_base = kernel_phys;
    boot_info.kernel_virt_base = kernel_virt_base;
    boot_info.kernel_size = kernel_span;
    boot_info.memory_map_base = map.as_ptr() as u64;
    boot_info.memory_map_len = map_count as u64;
    boot_info.memory_map_entry_size = size_of::<MemoryRegion>() as u64;
    boot_info.initfs = PhysRange {
        base: initfs_base,
        len: initfs_len,
    };
    boot_info.cmdline = PhysRange {
        base: cmdline.as_ptr() as u64,
        len: cmdline_len as u64,
    };
    boot_info.acpi_rsdp = acpi_rsdp;
    boot_info.boot_cpu_id = boot_cpu_id();
    // SAFETY: RDTSC is available on every x86-64 CPU and has no memory side effects.
    boot_info.entry_counter = unsafe { core::arch::x86_64::_rdtsc() };

    let pool_base = PAGE_POOL.0.get() as u64;
    let pool = PoolMem {
        base: pool_base,
        next: pool_base,
        end: pool_base.saturating_add(POOL_SIZE as u64),
    };
    let mut tables = match PageTables::new_with_physical_bits(pool, 40) {
        Ok(tables) => tables,
        Err(_) => fail("cannot create the page-table root"),
    };

    for region in map {
        if region.kind() == Some(MemoryKind::BadMemory) || region.kind() == Some(MemoryKind::Mmio) {
            continue;
        }
        if tables
            .map_hhdm_region(HHDM_BASE, region.base, region.len, WRITABLE | NO_EXECUTE)
            .is_err()
        {
            fail("cannot map the HHDM");
        }
    }

    for segment in elf.segments() {
        let vaddr = segment.vaddr & !(PAGE_SIZE - 1);
        let paddr = (kernel_phys + (segment.vaddr - kernel_virt_base)) & !(PAGE_SIZE - 1);
        let length = match page_ceil((segment.vaddr & (PAGE_SIZE - 1)) + segment.memsz) {
            Some(length) => length,
            None => fail("kernel segment size overflow"),
        };
        let flags = if segment.prot & 0b010 != 0 {
            WRITABLE | NO_EXECUTE
        } else if segment.prot & 0b100 == 0 {
            NO_EXECUTE
        } else {
            0
        };
        if tables.map_range_4k(vaddr, paddr, length, flags).is_err() {
            fail("cannot map a kernel segment");
        }
    }

    let text_len = loader_data_start.saturating_sub(loader_start);
    if tables
        .map_range_4k(loader_start, loader_start, text_len, 0)
        .is_err()
    {
        fail("cannot map the loader text");
    }
    let data_len = loader_end.saturating_sub(loader_data_start);
    if tables
        .map_range_4k(
            loader_data_start,
            loader_data_start,
            data_len,
            WRITABLE | NO_EXECUTE,
        )
        .is_err()
    {
        fail("cannot map the loader data");
    }

    let (_pool, root) = tables.into_parts();
    let bootinfo_virtual = HHDM_BASE + boot_info_ptr(&boot_info);
    serial_str("direct: entering kernel\n");
    // SAFETY: `root` maps the current code/stack identity, the kernel image at its link address
    // and the HHDM; `bootinfo_virtual` points at the `BootInfo` just filled; `entry` is the
    // validated kernel ELF entry.
    unsafe { enter_kernel(root, elf.entry(), bootinfo_virtual) }
}

/// Physical-memory access for the loader's temporary identity map.
struct PoolMem {
    base: u64,
    next: u64,
    end: u64,
}

impl PhysMem for PoolMem {
    fn allocate_frame(&mut self) -> Result<u64, PageError> {
        let frame = self.next;
        let next = frame.checked_add(PAGE_SIZE).ok_or(PageError::OutOfFrames)?;
        if next > self.end {
            return Err(PageError::OutOfFrames);
        }
        self.next = next;
        if !self.zero_frame(frame) {
            return Err(PageError::InvalidAddress);
        }
        Ok(frame)
    }

    fn read_u64(&self, physical: u64) -> Option<u64> {
        if !self.contains(physical, 8) || !physical.is_multiple_of(8) {
            return None;
        }
        // SAFETY: the range is inside the pool and the temporary identity map covers it.
        Some(unsafe { ptr::read_unaligned(physical as *const u64) })
    }

    fn write_u64(&mut self, physical: u64, value: u64) -> bool {
        if !self.contains(physical, 8) || !physical.is_multiple_of(8) {
            return false;
        }
        // SAFETY: the range is inside the pool and the temporary identity map covers it.
        unsafe { ptr::write_unaligned(physical as *mut u64, value) };
        true
    }

    fn zero_frame(&mut self, physical: u64) -> bool {
        if !self.contains(physical, PAGE_SIZE) {
            return false;
        }
        // SAFETY: the frame is inside the pool and the temporary identity map covers it.
        unsafe { ptr::write_bytes(physical as *mut u8, 0, PAGE_SIZE as usize) };
        true
    }
}

impl PoolMem {
    fn contains(&self, physical: u64, len: u64) -> bool {
        physical >= self.base && physical.checked_add(len).is_some_and(|end| end <= self.end)
    }
}

const EMPTY_REGION: MemoryRegion = MemoryRegion {
    base: 0,
    len: 0,
    kind: 0,
    reserved: 0,
};

const EMPTY_E820: MemmapEntry = MemmapEntry {
    addr: 0,
    size: 0,
    kind: MemoryKind::Reserved,
};

fn read_start_info(paddr: u64) -> Option<StartInfo> {
    if !identity_ok(paddr, 56) {
        return None;
    }
    let mut bytes = [0u8; 56];
    // SAFETY: `paddr..paddr+56` is inside the temporary identity map.
    unsafe { ptr::copy_nonoverlapping(paddr as *const u8, bytes.as_mut_ptr(), bytes.len()) };
    parse_start_info(&bytes)
}

fn read_module(paddr: u64) -> Option<Module> {
    if !identity_ok(paddr, 32) {
        return None;
    }
    let mut bytes = [0u8; 32];
    // SAFETY: `paddr..paddr+32` is inside the temporary identity map.
    unsafe { ptr::copy_nonoverlapping(paddr as *const u8, bytes.as_mut_ptr(), bytes.len()) };
    parse_module(&bytes)
}

fn read_e820(start_info: &StartInfo, out: &mut [MemmapEntry; MAX_E820]) -> Option<usize> {
    if start_info.memmap_paddr == 0 || start_info.memmap_entries == 0 {
        return None;
    }
    let count = usize::try_from(start_info.memmap_entries).ok()?;
    if count > out.len() {
        return None;
    }
    let needed = count.checked_mul(raven_direct::HVM_MEMMAP_ENTRY_SIZE)?;
    if !identity_ok(start_info.memmap_paddr, needed as u64) {
        return None;
    }
    // SAFETY: the range is inside the temporary identity map and has `needed` bytes.
    let bytes =
        unsafe { core::slice::from_raw_parts(start_info.memmap_paddr as *const u8, needed) };
    parse_memmap_into(bytes, start_info.memmap_entries, out).ok()
}

fn payload_slice(module: Module) -> Option<&'static [u8]> {
    let size = usize::try_from(module.size).ok()?;
    if !identity_ok(module.paddr, module.size) {
        return None;
    }
    // SAFETY: the module range is inside the temporary identity map and the loader owns it for
    // the rest of the boot.
    Some(unsafe { core::slice::from_raw_parts(module.paddr as *const u8, size) })
}

fn copy_kernel(elf: &Elf<'_>, kernel_phys: u64, virt_base: u64, size: u64) -> bool {
    if !identity_ok(kernel_phys, size) {
        return false;
    }
    // SAFETY: the destination was found inside the PVH usable map, is identity mapped and is
    // marked KernelAndModules; zeroing gives every BSS tail its required zero fill.
    unsafe { ptr::write_bytes(kernel_phys as *mut u8, 0, size as usize) };
    for segment in elf.segments() {
        let offset = match segment.vaddr.checked_sub(virt_base) {
            Some(offset) => offset,
            None => return false,
        };
        let source = elf.segment_data(&segment);
        if source.len() as u64 != segment.filesz {
            return false;
        }
        let destination = kernel_phys + offset;
        if !identity_ok(destination, source.len() as u64) {
            return false;
        }
        // SAFETY: both ranges are valid, the destination is inside the kernel buffer and the
        // source is the parsed ELF inside the module.
        unsafe {
            ptr::copy_nonoverlapping(source.as_ptr(), destination as *mut u8, source.len());
        }
    }
    true
}

fn read_cmdline(paddr: u64, out: &mut [u8]) -> usize {
    if paddr == 0 {
        return 0;
    }
    let mut length = 0usize;
    while length < out.len() && identity_ok(paddr + length as u64, 1) {
        // SAFETY: the byte is inside the temporary identity map.
        let byte = unsafe { ptr::read_volatile((paddr + length as u64) as *const u8) };
        if byte == 0 {
            return length;
        }
        out[length] = byte;
        length += 1;
    }
    length
}

fn inject_mcfg(rsdp_paddr: u64, cmdline: &[u8]) -> Option<u64> {
    let ecam_base = parse_mcfg_token(cmdline)?;
    if rsdp_paddr == 0 || !identity_ok(rsdp_paddr, ACPI_RSDP_SIZE as u64) {
        return None;
    }
    let mut rsdp = [0u8; ACPI_RSDP_SIZE];
    // SAFETY: the RSDP range was checked against the temporary identity map.
    unsafe {
        ptr::copy_nonoverlapping(rsdp_paddr as *const u8, rsdp.as_mut_ptr(), rsdp.len());
    }
    if rsdp.get(0..8) != Some(b"RSD PTR ") {
        return None;
    }
    let xsdt_paddr = u64::from_le_bytes(rsdp[24..32].try_into().ok()?);
    if xsdt_paddr == 0 || !identity_ok(xsdt_paddr, ACPI_HEADER_SIZE as u64) {
        return None;
    }
    let mut header = [0u8; ACPI_HEADER_SIZE];
    // SAFETY: the XSDT header is inside the temporary identity map.
    unsafe {
        ptr::copy_nonoverlapping(xsdt_paddr as *const u8, header.as_mut_ptr(), header.len());
    }
    if header.get(0..4) != Some(b"XSDT") {
        return None;
    }
    let length = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
    if length < ACPI_HEADER_SIZE || length > 4096 || !identity_ok(xsdt_paddr, length as u64) {
        return None;
    }
    let mut xsdt = [0u8; 4096];
    // SAFETY: the complete XSDT was checked against the temporary identity map.
    unsafe {
        ptr::copy_nonoverlapping(xsdt_paddr as *const u8, xsdt.as_mut_ptr(), length);
    }
    let entries = (length - ACPI_HEADER_SIZE) / 8;
    for index in 0..entries {
        let offset = ACPI_HEADER_SIZE + index * 8;
        let table = u64::from_le_bytes(xsdt[offset..offset + 8].try_into().ok()?);
        if table == 0 || !identity_ok(table, 4) {
            continue;
        }
        let mut signature = [0u8; 4];
        // SAFETY: the table header is inside the temporary identity map.
        unsafe {
            ptr::copy_nonoverlapping(table as *const u8, signature.as_mut_ptr(), 4);
        }
        if &signature == b"MCFG" {
            return None;
        }
    }

    let buffer = (&raw const ACPI_BUFFER) as u64;
    // SAFETY: the ACPI buffer is a loader-owned static and is written once during boot.
    let out = unsafe {
        core::slice::from_raw_parts_mut(ACPI_BUFFER.0.get().cast::<u8>(), ACPI_BUFFER_SIZE)
    };
    let offset = append_mcfg(&rsdp, &xsdt[..length], buffer, ecam_base, out).ok()?;
    Some(buffer + offset as u64)
}

fn parse_mcfg_token(cmdline: &[u8]) -> Option<u64> {
    for key in [b"raven_mcfg=".as_slice(), b"raven.mcfg=".as_slice()] {
        let Some(position) = find_subslice(cmdline, key) else {
            continue;
        };
        let mut value = 0u64;
        let mut digits = 0usize;
        let rest = &cmdline[position + key.len()..];
        let rest = if rest.starts_with(b"0x") || rest.starts_with(b"0X") {
            &rest[2..]
        } else {
            rest
        };
        for byte in rest {
            let digit = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => break,
            };
            if digits >= 16 {
                return None;
            }
            value = value.checked_mul(16)?.checked_add(u64::from(digit))?;
            digits += 1;
        }
        if digits != 0 {
            return Some(value);
        }
    }
    None
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn boot_info_ptr(boot_info: &BootInfo) -> u64 {
    boot_info as *const BootInfo as u64
}

fn boot_cpu_id() -> u64 {
    u64::from(core::arch::x86_64::__cpuid(1).ebx >> 24)
}

fn identity_ok(paddr: u64, len: u64) -> bool {
    paddr
        .checked_add(len)
        .is_some_and(|end| end <= IDENTITY_LIMIT)
}

fn page_ceil(value: u64) -> Option<u64> {
    value
        .checked_add(PAGE_SIZE - 1)
        .map(|value| value & !(PAGE_SIZE - 1))
}

unsafe fn enter_kernel(root: u64, entry: u64, bootinfo: u64) -> ! {
    // SAFETY: the caller guarantees the page tables, entry and BootInfo pointer. `cli` disables
    // interrupts for the handoff and the jump never returns.
    unsafe {
        core::arch::asm!(
            "cli",
            "mov cr3, {root}",
            "mov rdi, {bootinfo}",
            "jmp {entry}",
            root = in(reg) root,
            bootinfo = in(reg) bootinfo,
            entry = in(reg) entry,
            options(noreturn),
        )
    }
}

fn serial_init() {
    // SAFETY: COM1 is a legacy ISA port present on the QEMU machines this loader targets; the
    // sequence is the standard 115200 8N1 setup and touches only the UART registers.
    unsafe {
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x80);
        outb(COM1 + 0, 0x01);
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);
        outb(COM1 + 2, 0xC7);
        outb(COM1 + 4, 0x0B);
    }
}

const COM1: u16 = 0x3f8;

fn serial_str(text: &str) {
    for byte in text.as_bytes() {
        serial_byte(*byte);
    }
}

fn serial_byte(byte: u8) {
    // SAFETY: the UART is present and the loop only polls its line-status register.
    unsafe {
        while inb(COM1 + 5) & 0x20 == 0 {}
        outb(COM1, byte);
    }
}

fn serial_hex(value: u64) {
    serial_str("0x");
    let mut printed = false;
    for shift in (0..16).rev() {
        let nibble = ((value >> (shift * 4)) & 0xf) as u8;
        if nibble != 0 || printed || shift == 0 {
            printed = true;
            serial_byte(if nibble < 10 {
                b'0' + nibble
            } else {
                b'a' + (nibble - 10)
            });
        }
    }
}

unsafe fn outb(port: u16, value: u8) {
    // SAFETY: the caller names a valid x86 I/O port.
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: the caller names a valid x86 I/O port.
    unsafe {
        core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn fail(message: &str) -> ! {
    serial_str("direct: error: ");
    serial_str(message);
    serial_str("\n");
    // SAFETY: `-device isa-debug-exit` is part of the documented direct-boot command; on real
    // hardware the port write is harmless and `halt` follows.
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") 0xf4u16,
            in("eax") 0x11u32,
            options(nomem, nostack, preserves_flags),
        );
    }
    halt()
}

fn halt() -> ! {
    loop {
        // SAFETY: `cli; hlt` is a ring-0 idle sequence with no memory operands.
        unsafe {
            core::arch::asm!("cli", "hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
