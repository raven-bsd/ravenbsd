// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-loader` UEFI application entry point.
//!
//! S03 reads the kernel, initfs and optional `boot.toml`, loads the kernel ELF, builds the page
//! tables, converts the final UEFI memory map, fills `BootInfo` and jumps to the kernel.

#![cfg_attr(target_os = "uefi", no_std)]
#![cfg_attr(target_os = "uefi", no_main)]

#[cfg(target_os = "uefi")]
extern crate alloc;

#[cfg(target_os = "uefi")]
use alloc::format;
#[cfg(target_os = "uefi")]
use alloc::string::String;
#[cfg(target_os = "uefi")]
use alloc::vec::Vec;

#[cfg(target_os = "uefi")]
use core::fmt;

#[cfg(target_os = "uefi")]
use raven_bootinfo::{BootInfo, Framebuffer, MemoryRegion, PhysRange};
#[cfg(target_os = "uefi")]
use raven_loader::layout::HHDM_BASE;
#[cfg(target_os = "uefi")]
use raven_loader::{bootinfo, config, fs, handoff, image, memmap, paging, serial};
#[cfg(target_os = "uefi")]
use uefi::boot::{self, AllocateType, MemoryType};
#[cfg(target_os = "uefi")]
use uefi::fs::FileSystem;
#[cfg(target_os = "uefi")]
use uefi::mem::memory_map::{MemoryMap, MemoryMapOwned};
#[cfg(target_os = "uefi")]
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
#[cfg(target_os = "uefi")]
use uefi::proto::loaded_image::LoadedImage;
#[cfg(target_os = "uefi")]
use uefi::table::cfg::ConfigTableEntry;

#[cfg(all(target_os = "uefi", target_arch = "x86_64"))]
const ARCH_NAME: &str = "x86_64";
#[cfg(all(target_os = "uefi", target_arch = "aarch64"))]
const ARCH_NAME: &str = "aarch64";

#[cfg(target_os = "uefi")]
const STACK_SIZE: u64 = 0x4000;
#[cfg(target_os = "uefi")]
const PAGE_TABLE_POOL_PAGES: usize = 256;
#[cfg(target_os = "uefi")]
const OLD_STACK_WINDOW: u64 = 0x1_0000;

#[cfg(target_os = "uefi")]
#[global_allocator]
static ALLOCATOR: uefi::allocator::Allocator = uefi::allocator::Allocator;

#[cfg(target_os = "uefi")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    serial::init_serial_only();
    serial::log(format_args!("error: panic: {info}"));
    exit_qemu(0x11)
}

#[cfg(target_os = "uefi")]
#[uefi::entry]
fn main() -> uefi::Status {
    serial::init();
    serial::log(format_args!("RavenBSD loader 0.1 ({ARCH_NAME})"));
    if !handoff::at_el1() {
        serial::log(format_args!(
            "error: the loader must run at EL1 (CurrentEL is EL2 or EL3)"
        ));
        exit_qemu(0x11);
    }

    match run() {
        Ok(()) => {
            serial::log(format_args!(
                "error: loader returned without entering the kernel"
            ));
            exit_qemu(0x11)
        }
        Err(error) => {
            serial::log(format_args!("error: {error}"));
            exit_qemu(0x11)
        }
    }
}

#[cfg(target_os = "uefi")]
struct Prepared {
    kernel: image::LoadedKernel,
    initfs: PhysRange,
    cmdline: PhysRange,
    bootinfo_phys: u64,
    bootinfo_ptr: *mut BootInfo,
    stack_base_phys: u64,
    stack_top_virt: u64,
    pool: paging::UefiPhysMem,
    image_base: u64,
    image_size: u64,
    old_stack_start: u64,
    old_stack_end: u64,
    acpi_rsdp: u64,
    dtb: PhysRange,
    framebuffer: Framebuffer,
    boot_cpu_id: u64,
    counter_freq: u64,
    uefi_system_table: u64,
    regions: Vec<MemoryRegion>,
    map_buffers: memmap::MemMapBuffers,
    descriptors: Vec<uefi::mem::memory_map::MemoryDescriptor>,
    verbose: bool,
}

#[cfg(target_os = "uefi")]
fn run() -> Result<(), String> {
    let mut filesystem = FileSystem::new(
        boot::get_image_file_system(boot::image_handle())
            .map_err(|error| format!("cannot open the boot volume: {error}"))?,
    );

    let boot_toml = if fs::exists(&mut filesystem, fs::BOOT_TOML_PATH)? {
        fs::read_file(&mut filesystem, fs::BOOT_TOML_PATH)?
    } else {
        Vec::new()
    };
    let (config, warnings) = config::parse_with_warnings(&boot_toml)
        .map_err(|error| format!("bad boot.toml: {error}"))?;
    for warning in warnings {
        serial::log(format_args!("warning: {warning}"));
    }

    let kernel_bytes = fs::read_file(&mut filesystem, fs::KERNEL_PATH)?;
    let initfs_bytes = fs::read_file(&mut filesystem, fs::INITFS_PATH)?;
    let kernel =
        image::load(&kernel_bytes).map_err(|error| format!("cannot load kernel: {error}"))?;
    drop(kernel_bytes);

    let verbose = config.log_level == config::LogLevel::Debug;
    let prepared = prepare(kernel, &initfs_bytes, &config.cmdline, verbose)?;

    drop(boot_toml);
    drop(config);
    drop(initfs_bytes);
    drop(filesystem);

    serial::disable_conout();
    // SAFETY: all boot-services protocols and allocator users were dropped above; every buffer
    // the kernel needs was allocated before this call; after it returns the loader only uses
    // memory it already owns and the final memory map it returns.
    let memory_map = unsafe { boot::exit_boot_services(Some(MemoryType::LOADER_DATA)) };
    finish(prepared, &memory_map)
}

#[cfg(target_os = "uefi")]
fn prepare(
    kernel: image::LoadedKernel,
    initfs_bytes: &[u8],
    cmdline: &str,
    verbose: bool,
) -> Result<Prepared, String> {
    let current_map = boot::memory_map(MemoryType::LOADER_DATA)
        .map_err(|error| format!("cannot query the UEFI memory map: {error}"))?;
    let max_entries = current_map
        .len()
        .checked_mul(2)
        .and_then(|value| value.checked_add(16))
        .ok_or_else(|| String::from("UEFI memory map is too large"))?;
    drop(current_map);

    let descriptors = Vec::with_capacity(max_entries);
    let regions = Vec::with_capacity(max_entries.saturating_mul(4).saturating_add(16));
    let map_buffers = memmap::MemMapBuffers::new(max_entries);

    let initfs = copy_to_pages(initfs_bytes)?;
    let cmdline = copy_to_pages(cmdline.as_bytes())?;
    let (bootinfo_phys, bootinfo_ptr) = allocate_pages(4096)?;
    let bootinfo_ptr = bootinfo_ptr.cast::<BootInfo>();
    let (stack_base_phys, _stack_ptr) = allocate_pages(STACK_SIZE)?;
    let stack_top_virt = HHDM_BASE
        .checked_add(stack_base_phys)
        .and_then(|value| value.checked_add(STACK_SIZE))
        .ok_or_else(|| String::from("stack address overflow"))?;

    let pool = paging::UefiPhysMem::allocate(PAGE_TABLE_POOL_PAGES)
        .map_err(|error| format!("cannot allocate page-table frames: {error}"))?;

    let image_protocol = boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle())
        .map_err(|error| format!("cannot read the loader image information: {error}"))?;
    let (image_base, image_size) = image_protocol.info();
    let image_base = image_base as usize as u64;
    drop(image_protocol);
    if image_size == 0 {
        return Err(String::from("loader image has zero size"));
    }

    let framebuffer = framebuffer();
    let acpi_rsdp = acpi_rsdp();
    let dtb = dtb();
    let uefi_system_table = uefi::table::system_table_raw()
        .map(|table| table.as_ptr() as usize as u64)
        .unwrap_or(0);
    let boot_cpu_id = handoff::boot_cpu_id();
    let counter_freq = handoff::counter_freq();
    if verbose {
        serial::log(format_args!(
            "dtb {:#x}+{:#x} counter_freq={} boot_cpu_id={:#x}",
            dtb.base, dtb.len, counter_freq, boot_cpu_id
        ));
    }

    let stack_pointer = handoff::stack_pointer();
    let old_stack_start = stack_pointer.saturating_sub(OLD_STACK_WINDOW) & !(paging::PAGE_SIZE - 1);
    let old_stack_end = align_up(
        stack_pointer
            .checked_add(paging::PAGE_SIZE)
            .ok_or_else(|| String::from("stack address overflow"))?,
    )
    .ok_or_else(|| String::from("stack address overflow"))?;

    Ok(Prepared {
        kernel,
        initfs,
        cmdline,
        bootinfo_phys,
        bootinfo_ptr,
        stack_base_phys,
        stack_top_virt,
        pool,
        image_base,
        image_size,
        old_stack_start,
        old_stack_end,
        acpi_rsdp,
        dtb,
        framebuffer,
        boot_cpu_id,
        counter_freq,
        uefi_system_table,
        regions,
        map_buffers,
        descriptors,
        verbose,
    })
}

#[cfg(target_os = "uefi")]
fn finish(prepared: Prepared, memory_map: &MemoryMapOwned) -> ! {
    let Prepared {
        kernel,
        initfs,
        cmdline,
        bootinfo_phys,
        bootinfo_ptr,
        stack_base_phys,
        stack_top_virt,
        pool,
        image_base,
        image_size,
        old_stack_start,
        old_stack_end,
        acpi_rsdp,
        dtb,
        framebuffer,
        boot_cpu_id,
        counter_freq,
        uefi_system_table,
        mut regions,
        mut map_buffers,
        mut descriptors,
        verbose,
    } = prepared;

    descriptors.clear();
    for descriptor in memory_map.entries() {
        if descriptors.len() == descriptors.capacity() {
            fail(format_args!(
                "UEFI memory map grew past the reserved capacity"
            ));
        }
        descriptors.push(*descriptor);
    }

    let tracked = [
        PhysRange {
            base: kernel.phys_base,
            len: kernel.size,
        },
        initfs,
    ];
    if let Err(error) = memmap::convert_into(&descriptors, &tracked, &mut regions, &mut map_buffers)
    {
        fail(format_args!("cannot convert the UEFI memory map: {error}"));
    }
    if let Err(error) = raven_bootinfo::validate_memory_map(&regions) {
        fail(format_args!("converted memory map is invalid: {error:?}"));
    }
    if !kernel.virt_base.is_multiple_of(paging::PAGE_SIZE)
        || !kernel.phys_base.is_multiple_of(paging::PAGE_SIZE)
    {
        fail(format_args!(
            "kernel image base is not page aligned (virt={:#x} phys={:#x})",
            kernel.virt_base, kernel.phys_base
        ));
    }
    serial::log(format_args!("memory map: {} regions", regions.len()));
    if verbose {
        serial::log(format_args!(
            "loader image {:#x}+{:#x} stack {:#x}..{:#x} pool {:#x} kernel {:#x}+{:#x}",
            image_base,
            image_size,
            old_stack_start,
            old_stack_end,
            stack_base_phys,
            kernel.phys_base,
            kernel.size
        ));
    }

    for segment in &kernel.segments {
        if segment.prot & 0b010 != 0 && segment.prot & 0b100 != 0 {
            fail(format_args!(
                "kernel segment {:#x} is writable and executable",
                segment.vaddr
            ));
        }
    }

    if verbose {
        serial::log(format_args!("page tables: building"));
    }
    let roots = match paging::build_loader_tables(paging::LoaderMap {
        pool,
        regions: &regions,
        kernel: &kernel,
        image_base,
        image_size,
        old_stack_start,
        old_stack_end,
        stack_base_phys,
        stack_size: STACK_SIZE,
    }) {
        Ok(roots) => roots,
        Err(error) => fail(format_args!("cannot build the page tables: {error}")),
    };
    if verbose {
        serial::log(format_args!("page tables: built"));
    }

    let info = bootinfo::assemble(bootinfo::BootInfoParts {
        kernel_phys_base: kernel.phys_base,
        kernel_virt_base: kernel.virt_base,
        kernel_size: kernel.size,
        memory_map_base: regions.as_ptr() as usize as u64,
        memory_map_len: regions.len() as u64,
        initfs,
        cmdline,
        dtb,
        acpi_rsdp,
        framebuffer,
        boot_cpu_id,
        entry_counter: 0,
        counter_freq,
        uefi_system_table,
        verbose,
    });
    // SAFETY: the BootInfo page was allocated before ExitBootServices, is zeroed, identity-mapped
    // now and HHDM-mapped in the new tables, and `BootInfo` is a 200-byte repr(C) value.
    unsafe {
        core::ptr::write(bootinfo_ptr, info);
    }
    let bootinfo_virtual = match HHDM_BASE.checked_add(bootinfo_phys) {
        Some(address) => address,
        None => fail(format_args!("BootInfo address overflow")),
    };

    let entry_counter = handoff::read_counter();
    // SAFETY: the pointer is valid; this is the last write before the jump.
    unsafe {
        (*bootinfo_ptr).entry_counter = entry_counter;
    }

    let entry = kernel.entry;
    let kernel_phys = kernel.phys_base;
    let kernel_size = kernel.size;
    core::mem::forget(regions);
    core::mem::forget(descriptors);
    core::mem::forget(map_buffers);
    core::mem::forget(kernel);

    if verbose {
        serial::log(format_args!("jumping to kernel entry {entry:#x}"));
    }
    // SAFETY: the roots map the kernel at `entry`, the identity-mapped loader code and stack, and
    // the HHDM containing `bootinfo_virtual`; interrupts are disabled inside `jump`.
    unsafe {
        handoff::jump(
            entry,
            roots,
            stack_top_virt,
            bootinfo_virtual,
            kernel_phys,
            kernel_size,
        );
    }
}

#[cfg(all(target_os = "uefi", target_arch = "x86_64"))]
fn dtb() -> PhysRange {
    PhysRange { base: 0, len: 0 }
}

#[cfg(all(target_os = "uefi", target_arch = "aarch64"))]
fn dtb() -> PhysRange {
    // EFI_DTB_TABLE_GUID b1b621d5-f19c-41a5-830b-d9152c69aae0.
    const DTB_GUID: uefi::Guid = uefi::Guid::new(
        [0xd5, 0x21, 0xb6, 0xb1],
        [0x9c, 0xf1],
        [0xa5, 0x41],
        0x83,
        0x0b,
        [0xd9, 0x15, 0x2c, 0x69, 0xaa, 0xe0],
    );
    uefi::system::with_config_table(|entries| {
        for entry in entries {
            if entry.guid != DTB_GUID {
                continue;
            }
            let base = entry.address as usize as u64;
            // SAFETY: the configuration table points at an FDT; the first eight bytes are the
            // big-endian magic and totalsize, and UEFI keeps the table mapped until boot exit.
            let (magic, size) = unsafe {
                let pointer = entry.address as *const u8;
                (read_be32(pointer), read_be32(pointer.add(4)))
            };
            if magic == 0xd00d_feed && size >= 0x40 {
                return PhysRange {
                    base,
                    len: u64::from(size),
                };
            }
        }
        PhysRange { base: 0, len: 0 }
    })
}

#[cfg(all(target_os = "uefi", target_arch = "aarch64"))]
unsafe fn read_be32(pointer: *const u8) -> u32 {
    // SAFETY: the caller guarantees four readable bytes.
    unsafe {
        u32::from_be_bytes([
            pointer.read(),
            pointer.add(1).read(),
            pointer.add(2).read(),
            pointer.add(3).read(),
        ])
    }
}

#[cfg(target_os = "uefi")]
fn framebuffer() -> Framebuffer {
    let handles = match boot::find_handles::<GraphicsOutput>() {
        Ok(handles) => handles,
        Err(_) => return Framebuffer::default(),
    };
    for handle in handles {
        let Ok(mut gop) = boot::open_protocol_exclusive::<GraphicsOutput>(handle) else {
            continue;
        };
        let mode = gop.current_mode_info();
        let pixel_format = match mode.pixel_format() {
            PixelFormat::Rgb => 0,
            PixelFormat::Bgr => 1,
            PixelFormat::Bitmask => 2,
            PixelFormat::BltOnly => continue,
        };
        let (width, height) = mode.resolution();
        let Ok(width) = u32::try_from(width) else {
            continue;
        };
        let Ok(height) = u32::try_from(height) else {
            continue;
        };
        if width == 0 || height == 0 {
            continue;
        }
        let stride_bytes = u32::try_from(mode.stride().saturating_mul(4)).unwrap_or(0);
        let base = gop.frame_buffer().as_mut_ptr() as usize as u64;
        return Framebuffer {
            base,
            width,
            height,
            stride_bytes,
            bits_per_pixel: 32,
            pixel_format,
            reserved: 0,
        };
    }
    Framebuffer::default()
}

#[cfg(target_os = "uefi")]
fn acpi_rsdp() -> u64 {
    uefi::system::with_config_table(|entries| {
        let mut acpi1 = 0u64;
        let mut acpi2 = 0u64;
        for entry in entries {
            if entry.guid == ConfigTableEntry::ACPI2_GUID {
                acpi2 = entry.address as usize as u64;
            } else if entry.guid == ConfigTableEntry::ACPI_GUID {
                acpi1 = entry.address as usize as u64;
            }
        }
        if acpi2 != 0 { acpi2 } else { acpi1 }
    })
}

#[cfg(target_os = "uefi")]
fn allocate_pages(bytes: u64) -> Result<(u64, *mut u8), String> {
    let pages = usize::try_from(bytes.div_ceil(paging::PAGE_SIZE))
        .map_err(|_| String::from("allocation size does not fit usize"))?;
    if pages == 0 {
        return Err(String::from("zero-sized allocation"));
    }
    let allocation = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        .map_err(|error| format!("cannot allocate {pages} page(s): {error}"))?;
    let pointer = allocation.as_ptr();
    let length = pages
        .checked_mul(paging::PAGE_SIZE as usize)
        .ok_or_else(|| String::from("allocation size overflow"))?;
    // SAFETY: `allocation` is exactly `length` bytes and owned by the loader.
    unsafe {
        core::ptr::write_bytes(pointer, 0, length);
    }
    Ok((pointer as usize as u64, pointer))
}

#[cfg(target_os = "uefi")]
fn copy_to_pages(data: &[u8]) -> Result<PhysRange, String> {
    if data.is_empty() {
        return Ok(PhysRange { base: 0, len: 0 });
    }
    let (base, pointer) = allocate_pages(data.len() as u64)?;
    // SAFETY: the destination is a fresh page allocation of at least `data.len()` bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), pointer, data.len());
    }
    Ok(PhysRange {
        base,
        len: data.len() as u64,
    })
}

#[cfg(target_os = "uefi")]
fn align_up(value: u64) -> Option<u64> {
    value
        .checked_add(paging::PAGE_SIZE - 1)
        .map(|rounded| rounded & !(paging::PAGE_SIZE - 1))
}

#[cfg(target_os = "uefi")]
fn fail(message: fmt::Arguments<'_>) -> ! {
    serial::log(format_args!("error: {message}"));
    exit_qemu(0x11)
}

#[cfg(target_os = "uefi")]
fn exit_qemu(code: u8) -> ! {
    handoff::exit_qemu(code)
}

#[cfg(not(target_os = "uefi"))]
fn main() {}
