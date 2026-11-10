// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Early boot: validate the loader handoff, bring up the frame allocator and print the banner.

use core::fmt;
use core::mem::{align_of, size_of};

use raven_bootinfo::{
    BootInfo, BootInfoError, MemoryKind, MemoryMapError, MemoryRegion, validate_memory_map,
};

use crate::arch::{ARCH_NAME, HHDM_BASE};
use crate::cmdline::CmdLine;
use crate::{kprintln, memory::frame};

/// Architectural upper bound of the HHDM window starting at [`HHDM_BASE`].
///
/// `HHDM_BASE + 1 << 52` overflows the canonical 64-bit address space; the top 2 GiB layout leaves
/// 2^47 bytes of direct-map window before `u64::MAX`.
const HHDM_WINDOW_BYTES: u64 = 1 << 47;

/// Validated boot data borrowed from HHDM-mapped loader memory.
pub struct BootData {
    /// The copied BootInfo value.
    pub info: BootInfo,
    /// Validated physical memory map.
    pub memory_map: &'static [MemoryRegion],
    /// Validated UTF-8 command line (not NUL terminated).
    pub cmdline: &'static str,
}

/// Why the boot handoff was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootError {
    /// The loader passed a null pointer.
    NullBootInfo,
    /// The BootInfo pointer is not 8-byte aligned.
    MisalignedBootInfo,
    /// The BootInfo pointer is outside the HHDM window.
    BootInfoOutsideHhdm,
    /// The BootInfo size field overflows or is smaller than the structure.
    BadBootInfoSize,
    /// `BootInfo::validate` rejected the fixed fields.
    InvalidBootInfo(BootInfoError),
    /// The HHDM base does not match the architecture contract.
    HhdmMismatch,
    /// The memory-map pointer is invalid.
    BadMemoryMapPointer,
    /// The loader used an entry size other than `MemoryRegion`.
    BadMemoryMapEntrySize,
    /// The memory-map array overflows or leaves the HHDM.
    BadMemoryMapSize,
    /// The memory map failed validation.
    InvalidMemoryMap(MemoryMapError),
    /// The command-line pointer is invalid.
    BadCmdlinePointer,
    /// The command line is not UTF-8.
    BadCmdline,
}

impl fmt::Display for BootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootError::NullBootInfo => f.write_str("null BootInfo pointer"),
            BootError::MisalignedBootInfo => f.write_str("misaligned BootInfo pointer"),
            BootError::BootInfoOutsideHhdm => f.write_str("BootInfo pointer outside the HHDM"),
            BootError::BadBootInfoSize => f.write_str("invalid BootInfo size"),
            BootError::InvalidBootInfo(error) => write!(f, "invalid BootInfo: {error:?}"),
            BootError::HhdmMismatch => f.write_str("BootInfo HHDM base does not match the layout"),
            BootError::BadMemoryMapPointer => f.write_str("invalid memory-map pointer"),
            BootError::BadMemoryMapEntrySize => f.write_str("unsupported memory-map entry size"),
            BootError::BadMemoryMapSize => f.write_str("memory map leaves the HHDM window"),
            BootError::InvalidMemoryMap(error) => write!(f, "invalid memory map: {error:?}"),
            BootError::BadCmdlinePointer => f.write_str("invalid command-line pointer"),
            BootError::BadCmdline => f.write_str("command line is not UTF-8"),
        }
    }
}

/// Validate and copy the loader handoff.
pub fn validate(boot: *const BootInfo) -> Result<BootData, BootError> {
    if boot.is_null() {
        return Err(BootError::NullBootInfo);
    }
    let address = boot as usize as u64;
    if !address.is_multiple_of(align_of::<BootInfo>() as u64) {
        return Err(BootError::MisalignedBootInfo);
    }
    if !in_hhdm(address, 0) {
        return Err(BootError::BootInfoOutsideHhdm);
    }

    // `size` lives at offset 8. Check that the header is mapped before reading it, then verify
    // that the complete structure fits before the volatile read.
    if !in_hhdm(address, 12) {
        return Err(BootError::BadBootInfoSize);
    }
    // SAFETY: the address is inside the HHDM, aligned, and at least 12 bytes are mapped.
    let size = u64::from(unsafe { core::ptr::addr_of!((*boot).size).read_volatile() });
    if size < size_of::<BootInfo>() as u64 || !in_hhdm(address, size) {
        return Err(BootError::BadBootInfoSize);
    }

    // SAFETY: the complete structure was checked against the HHDM window. Copy it out so no
    // reference into loader memory is kept.
    let info = unsafe { boot.read_volatile() };
    info.validate().map_err(BootError::InvalidBootInfo)?;
    if info.hhdm_offset != HHDM_BASE {
        return Err(BootError::HhdmMismatch);
    }

    let map_virtual = HHDM_BASE
        .checked_add(info.memory_map_base)
        .ok_or(BootError::BadMemoryMapPointer)?;
    if !in_hhdm(map_virtual, 0) || !map_virtual.is_multiple_of(align_of::<MemoryRegion>() as u64) {
        return Err(BootError::BadMemoryMapPointer);
    }
    if info.memory_map_entry_size != size_of::<MemoryRegion>() as u64 {
        return Err(BootError::BadMemoryMapEntrySize);
    }
    let map_len = usize::try_from(info.memory_map_len).map_err(|_| BootError::BadMemoryMapSize)?;
    let map_bytes = map_len
        .checked_mul(size_of::<MemoryRegion>())
        .ok_or(BootError::BadMemoryMapSize)?;
    if !in_hhdm(map_virtual, map_bytes as u64) {
        return Err(BootError::BadMemoryMapSize);
    }
    // SAFETY: the array is inside the HHDM, correctly aligned for MemoryRegion, and the byte
    // length was checked. Loader memory remains mapped for the kernel's lifetime.
    let memory_map: &'static [MemoryRegion] =
        unsafe { core::slice::from_raw_parts(map_virtual as *const MemoryRegion, map_len) };
    validate_memory_map(memory_map).map_err(BootError::InvalidMemoryMap)?;

    let cmdline = if info.cmdline.len == 0 {
        ""
    } else {
        let virtual_address = HHDM_BASE
            .checked_add(info.cmdline.base)
            .ok_or(BootError::BadCmdlinePointer)?;
        let length = usize::try_from(info.cmdline.len).map_err(|_| BootError::BadCmdlinePointer)?;
        if !in_hhdm(virtual_address, info.cmdline.len) {
            return Err(BootError::BadCmdlinePointer);
        }
        // SAFETY: the range is inside the HHDM and has the exact recorded length.
        let bytes = unsafe { core::slice::from_raw_parts(virtual_address as *const u8, length) };
        core::str::from_utf8(bytes).map_err(|_| BootError::BadCmdline)?
    };

    Ok(BootData {
        info,
        memory_map,
        cmdline,
    })
}

fn in_hhdm(address: u64, length: u64) -> bool {
    if address < HHDM_BASE || !is_canonical(address) {
        return false;
    }
    let Some(offset) = address.checked_sub(HHDM_BASE) else {
        return false;
    };
    if offset >= HHDM_WINDOW_BYTES {
        return false;
    }
    let Some(end_offset) = offset.checked_add(length) else {
        return false;
    };
    if end_offset > HHDM_WINDOW_BYTES {
        return false;
    }
    let Some(end) = address.checked_add(length) else {
        return false;
    };
    is_canonical(end)
}

fn is_canonical(address: u64) -> bool {
    let top = address >> 47;
    top == 0 || top == 0x1_ffff
}

fn usable_stats(map: &[MemoryRegion]) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut regions = 0u64;
    for region in map {
        if region.kind() == Some(MemoryKind::Usable) {
            bytes = bytes.saturating_add(region.len);
            regions = regions.saturating_add(1);
        }
    }
    (bytes, regions)
}

/// Print the C4 banner and the command line.
pub fn print_banner(data: &BootData) {
    let (usable_bytes, usable_regions) = usable_stats(data.memory_map);
    kprintln!(
        "RavenBSD kernel 0.1 ({}) {}",
        ARCH_NAME,
        env!("RAVEN_BUILD_ID")
    );
    kprintln!(
        "mem: {} MiB usable in {} regions",
        usable_bytes / (1024 * 1024),
        usable_regions
    );
    kprintln!("cmdline: {}", data.cmdline);
}

/// Validate the handoff, initialise the frame allocator and either exit the test or halt.
#[cfg_attr(target_arch = "aarch64", allow(dead_code))] // x86-64 `kmain` calls this; aarch64 has its own flow.
pub fn run(boot: *const BootInfo) -> ! {
    let data = match validate(boot) {
        Ok(data) => data,
        Err(error) => {
            kprintln!("kernel: error: {error}");
            exit_qemu(0x11);
        }
    };
    let cmdline = CmdLine::parse(data.cmdline);

    crate::log::init(cmdline.loglevel());
    crate::log::set_test_exit(cmdline.test_exit());

    if let Err(error) = frame::init(data.info.hhdm_offset, data.memory_map) {
        kprintln!("kernel: error: {error}");
        exit_qemu(0x11);
    }
    let stats = frame::stats();

    print_banner(&data);
    crate::kdebug!("frame allocator: {} free bytes", stats.free_bytes);
    if cmdline.test_exit() {
        exit_qemu(0x10);
    }
    kprintln!("boot: early init complete, halting");
    crate::arch::halt()
}

/// Signal a QEMU test result through the C5 exit protocol and halt.
pub fn exit_qemu(code: u8) -> ! {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: QEMU's isa-debug-exit device watches port 0xf4; on real hardware this is an
        // unused port and the CPU halts afterwards.
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") 0xf4u16,
                in("al") code,
                options(nomem, nostack, preserves_flags),
            );
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // Semihosting SYS_EXIT_EXTENDED (0x20): `x1` points at {ADP_Stopped_ApplicationExit,
        // status}. The status keeps the C5 numbers: 0x10 -> 33 success, 0x11 -> 35 failure.
        let status = [0x20026u64, (u64::from(code) << 1) | 1];
        // SAFETY: `halt #0xf000` is the AArch64 semihosting call; with
        // `-semihosting-config enable=on,target=native` QEMU reads the two-word block and exits.
        unsafe {
            core::arch::asm!(
                "hlt #0xf000",
                in("x0") 0x20u64,
                in("x1") status.as_ptr(),
                options(nostack),
            );
        }
    }
    crate::arch::halt()
}
