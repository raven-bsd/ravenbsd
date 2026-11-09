// SPDX-License-Identifier: MIT
// Copyright (c) 2017 Jeremy Soller
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Hand-written page-flag bit set.
//!
//! The bit positions are the architectural x86-64 PTE bits. The crate does not depend on the
//! `bitflags` crate so the ABI stays explicit and dependency-free.

use core::fmt;

/// Page is present.
pub const PRESENT: u64 = 1 << 0;
/// Page is writable.
pub const WRITABLE: u64 = 1 << 1;
/// Page is accessible from ring 3.
pub const USER: u64 = 1 << 2;
/// Write-through cache policy.
pub const WRITE_THROUGH: u64 = 1 << 3;
/// Cache disabled.
pub const CACHE_DISABLE: u64 = 1 << 4;
/// Set by the CPU on access.
pub const ACCESSED: u64 = 1 << 5;
/// Set by the CPU on write.
pub const DIRTY: u64 = 1 << 6;
/// Huge page (PS) at PDP/PD level.
pub const HUGE: u64 = 1 << 7;
/// Global translation.
pub const GLOBAL: u64 = 1 << 8;
/// No execute (requires EFER.NXE).
pub const NO_EXECUTE: u64 = 1 << 63;

/// A set of x86-64 page flags.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct PageFlags(u64);

impl PageFlags {
    /// No flags.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Wrap raw bits.
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// The raw bits.
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// True when every bit in `other` is set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Union of two flag sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Set the bits in `other`.
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Clear the bits in `other`.
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// True when the present bit is set.
    pub const fn is_present(self) -> bool {
        self.0 & PRESENT != 0
    }

    /// True when the writable bit is set.
    pub const fn is_writable(self) -> bool {
        self.0 & WRITABLE != 0
    }

    /// True when the page is executable (NX clear).
    pub const fn is_executable(self) -> bool {
        self.0 & NO_EXECUTE == 0
    }

    /// True when the user bit is set.
    pub const fn is_user(self) -> bool {
        self.0 & USER != 0
    }

    /// True when the global bit is set.
    pub const fn is_global(self) -> bool {
        self.0 & GLOBAL != 0
    }

    /// True when the huge bit is set.
    pub const fn is_huge(self) -> bool {
        self.0 & HUGE != 0
    }

    /// True for a writable, executable page (forbidden by the W^X policy).
    pub const fn is_write_execute(self) -> bool {
        self.is_writable() && self.is_executable()
    }

    /// Kernel read/execute flags.
    pub const fn kernel_rx() -> Self {
        Self(PRESENT | GLOBAL)
    }

    /// Kernel read-only, no-execute flags.
    pub const fn kernel_ro() -> Self {
        Self(PRESENT | GLOBAL | NO_EXECUTE)
    }

    /// Kernel read/write, no-execute flags.
    pub const fn kernel_rw() -> Self {
        Self(PRESENT | WRITABLE | GLOBAL | NO_EXECUTE)
    }

    /// User read/execute flags.
    pub const fn user_rx() -> Self {
        Self(PRESENT | USER)
    }

    /// User read-only, no-execute flags.
    pub const fn user_ro() -> Self {
        Self(PRESENT | USER | NO_EXECUTE)
    }

    /// User read/write, no-execute flags.
    pub const fn user_rw() -> Self {
        Self(PRESENT | USER | WRITABLE | NO_EXECUTE)
    }

    /// Uncached device-memory flags (no execute).
    pub const fn mmio() -> Self {
        Self(PRESENT | WRITABLE | GLOBAL | NO_EXECUTE | CACHE_DISABLE | WRITE_THROUGH)
    }
}

impl Default for PageFlags {
    fn default() -> Self {
        Self::empty()
    }
}

impl fmt::Debug for PageFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageFlags")
            .field("present", &self.is_present())
            .field("writable", &self.is_writable())
            .field("user", &self.is_user())
            .field("executable", &self.is_executable())
            .field("global", &self.is_global())
            .field("huge", &self.is_huge())
            .field("bits", &format_args!("{:#x}", self.0))
            .finish()
    }
}

/// Size of a mapped page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSize {
    /// 4 KiB.
    Size4K,
    /// 2 MiB.
    Size2M,
    /// 1 GiB (translate only).
    Size1G,
}

impl PageSize {
    /// Size in bytes.
    pub const fn bytes(self) -> u64 {
        match self {
            PageSize::Size4K => 1 << 12,
            PageSize::Size2M => 1 << 21,
            PageSize::Size1G => 1 << 30,
        }
    }

    /// Page-table level: PT=0, PD=1, PDP=2.
    pub const fn level(self) -> u8 {
        match self {
            PageSize::Size4K => 0,
            PageSize::Size2M => 1,
            PageSize::Size1G => 2,
        }
    }
}
