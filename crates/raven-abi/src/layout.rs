// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Virtual memory layout constants shared by loader, kernel and `raven-rt`.
//! x86-64 and aarch64 values are final for ABI v1 (aarch64: 4 KiB granule, 48-bit VA, TTBR1 kernel half,
//! TTBR0 user half, the same split as x86-64); riscv64 values are added by its ticket (S19) with lead approval.

/// x86-64 layout (`architecture/00-overview.md` section 5).
pub mod x86_64 {
    /// Lowest mappable user address (page 0 stays unmapped).
    pub const USER_MIN: u64 = 0x0000_0000_0000_1000;
    /// One past the highest user address (canonical lower half, minus one guard page).
    pub const USER_END: u64 = 0x0000_7FFF_FFFF_F000;
    /// Virtual address where physical address 0 is mapped (direct map of all RAM).
    pub const HHDM_BASE: u64 = 0xFFFF_8000_0000_0000;
    /// Start of the kernel heap / dynamic kernel mappings.
    pub const KERNEL_HEAP_BASE: u64 = 0xFFFF_C000_0000_0000;
    /// End of the kernel heap region.
    pub const KERNEL_HEAP_END: u64 = 0xFFFF_E000_0000_0000;
    /// Virtual base of the kernel image (top 2 GiB; code model `kernel`).
    pub const KERNEL_IMAGE_BASE: u64 = 0xFFFF_FFFF_8000_0000;
    /// Default user program load address used by `userland/linker/x86_64.ld`.
    pub const USER_LOAD_BASE: u64 = 0x0000_0000_0040_0000;
}

/// aarch64 layout (`architecture/00-overview.md` section 5: "aarch64 uses the equivalent split"). The values equal the
/// x86-64 ones on purpose: the loader (S12) already maps the HHDM and the kernel image at these addresses.
pub mod aarch64 {
    /// Lowest mappable user address (page 0 stays unmapped).
    pub const USER_MIN: u64 = 0x0000_0000_0000_1000;
    /// One past the highest user address (TTBR0 range, minus one guard page).
    pub const USER_END: u64 = 0x0000_7FFF_FFFF_F000;
    /// Virtual address where physical address 0 is mapped (direct map of all RAM and device windows).
    pub const HHDM_BASE: u64 = 0xFFFF_8000_0000_0000;
    /// Start of the kernel heap / dynamic kernel mappings.
    pub const KERNEL_HEAP_BASE: u64 = 0xFFFF_C000_0000_0000;
    /// End of the kernel heap region.
    pub const KERNEL_HEAP_END: u64 = 0xFFFF_E000_0000_0000;
    /// Virtual base of the kernel image (top 2 GiB).
    pub const KERNEL_IMAGE_BASE: u64 = 0xFFFF_FFFF_8000_0000;
    /// Default user program load address used by `userland/linker/aarch64.ld`.
    pub const USER_LOAD_BASE: u64 = 0x0000_0000_0040_0000;
}

const _: () = {
    use aarch64 as a;
    use x86_64 as x;
    assert!(a::USER_MIN == x::USER_MIN && a::USER_END == x::USER_END);
    assert!(a::HHDM_BASE == x::HHDM_BASE && a::KERNEL_IMAGE_BASE == x::KERNEL_IMAGE_BASE);
    assert!(a::KERNEL_HEAP_BASE == x::KERNEL_HEAP_BASE && a::KERNEL_HEAP_END == x::KERNEL_HEAP_END);
    assert!(a::USER_LOAD_BASE == x::USER_LOAD_BASE);
};

const _: () = {
    use x86_64::*;
    assert!(USER_MIN < USER_END);
    assert!(USER_END < 0x0000_8000_0000_0000);
    assert!(HHDM_BASE >= 0xFFFF_8000_0000_0000);
    assert!(HHDM_BASE < KERNEL_HEAP_BASE);
    assert!(KERNEL_HEAP_BASE < KERNEL_HEAP_END);
    assert!(KERNEL_HEAP_END < KERNEL_IMAGE_BASE);
    assert!(KERNEL_IMAGE_BASE.is_multiple_of(4096));
    assert!(USER_LOAD_BASE >= USER_MIN && USER_LOAD_BASE < USER_END);
};
