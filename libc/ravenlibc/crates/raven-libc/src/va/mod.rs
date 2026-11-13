// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Variadic trampolines and `va_list` readers for the three target architectures.
//!
//! Stable Rust cannot define a C-variadic function (`extern "C" fn f(x: T, ...)` needs the
//! nightly `c_variadic` gate), so every exported variadic symbol of ravenlibc is a
//! `global_asm!` stub that spills the argument registers, builds the platform's `va_list` in
//! its own frame and calls a normal Rust `v*` function with that list. [`trampoline`] holds
//! the one macro that emits the stubs, [`table`] the data table of every row, and the three
//! reader modules the exact `va_list` layout and reader rules of `donor-map/04` 3.9.
//!
//! Layouts (the contract, `donor-map/04` 3.9.1-3.9.3 and C59 section 4.4):
//!
//! | Arch | Save area | Frame | `va_list` | Reader |
//! |---|---|---|---|---|
//! | x86_64 SysV | [`sysv_x86_64::VaSaveSysV`] 176 B | 208 B | [`sysv_x86_64::VaListSysV`] at 176 | [`sysv_x86_64::SysvReader`] |
//! | aarch64 AAPCS64 | [`aapcs64::VaSaveAapcs64`] 208 B | 240 B | [`aapcs64::VaListAapcs64`] at 208 | [`aapcs64::Aapcs64Reader`] |
//! | riscv64 psABI | [`riscv64::VaSaveRiscv64`] 80 B | 80 B | [`riscv64::VaListRiscv64`] cursor | [`riscv64::RiscvReader`] |
//!
//! Only x86_64 is verified against the compiler's real calling convention (K03, 57 cases);
//! aarch64 and riscv64 are specifications whose non-`v` forms are link-time failing stubs
//! until a `qemu-user` lane passes (`donor-map/04` O3, RD-34, H-16).
//!
//! The readers implement [`raven_libc_fmt::VaReader`] and [`VaReaderExt`]; they copy the list
//! on construction and never write back into the caller's `va_list` (RD-new 4).
//!
//! All three reader modules compile on every target: the layouts are plain `#[repr(C)]`
//! structs and the readers are ordinary code, so the aarch64 and riscv64 readers are unit
//! tested on the x86_64 host. Only the assembly and the [`NativeReader`] alias are gated by
//! `target_arch` (RD-new 3).

pub mod aapcs64;
pub mod env;
pub mod riscv64;
pub mod sysv_x86_64;
pub mod table;
pub mod trampoline;

pub use env::CEnv;
pub use raven_libc_fmt::{FmtEnv, VaReader};

#[cfg(target_arch = "aarch64")]
pub use aapcs64::Aapcs64Reader as NativeReader;
#[cfg(target_arch = "riscv64")]
pub use riscv64::RiscvReader as NativeReader;
#[cfg(target_arch = "x86_64")]
pub use sysv_x86_64::SysvReader as NativeReader;

pub use aapcs64::{Aapcs64Reader, VaListAapcs64, VaSaveAapcs64};
pub use riscv64::{RiscvReader, VaListRiscv64, VaSaveRiscv64};
pub use sysv_x86_64::{SysvReader, VaListSysV, VaSaveSysV};

/// Maximum number of fixed leading (`named`) arguments of a stub on x86_64.
///
/// The `&va_list` takes the argument register after the named ones, and there are six:
/// `rdi rsi rdx rcx r8 r9`, so `N` is at most 5. Enforced by the macro at compile time.
pub const MAX_NAMED_X86_64: usize = 5;

/// Maximum number of fixed leading (`named`) arguments of a stub on aarch64 and riscv64.
///
/// Eight integer argument registers: `x0..x7` / `a0..a7`, so `N` is at most 7.
pub const MAX_NAMED_OTHER: usize = 7;

/// Size of the x86_64 stub frame in bytes (13 x 16, so the `call` is 16-byte aligned).
pub const FRAME_X86_64: usize = 208;

/// Offset of the `va_list` inside the x86_64 stub frame.
pub const LIST_OFFSET_X86_64: usize = 176;

/// Size of the aarch64 stub frame in bytes (15 x 16).
pub const FRAME_AAPCS64: usize = 240;

/// Offset of the `va_list` inside the aarch64 stub frame.
pub const LIST_OFFSET_AAPCS64: usize = 208;

/// Size of the riscv64 stub frame in bytes.
pub const FRAME_RISCV64: usize = 80;

/// Offset of the first saved integer register in the riscv64 frame.
pub const SAVE_OFFSET_RISCV64: usize = 16;

/// The class of one variadic argument, as the C caller passed it.
///
/// `Int`, `Long` and `Ptr` all read one 8-byte integer slot: the reader returns the whole
/// slot and the engine truncates by the length modifier (A-stdio-3). `Agg` is the documented
/// integer-class struct-by-value subset of C59 section 4.5.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VaClass {
    /// `int`, `unsigned int`, `char`, `short`, `wint_t`: one 8-byte slot.
    Int,
    /// `long`, `unsigned long`, `long long`, `intmax_t`, `size_t`: one 8-byte slot.
    Long,
    /// A pointer value: one 8-byte slot.
    Ptr,
    /// A `double`, fetched as its 64-bit memory image.
    Double,
    /// A `long double`: x87 80-bit on x86_64, IEEE binary128 elsewhere.
    LongDouble,
    /// An integer-class aggregate of `size` bytes with `align` bytes of alignment.
    Agg {
        /// Size in bytes.
        size: u8,
        /// Alignment in bytes.
        align: u8,
    },
}

/// The reader operation a [`VaClass`] selects.
///
/// Recorder targets and the ABI model dispatch on this instead of on the class enum so that
/// an aggregate read carries its size.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VaKind {
    /// One integer slot read as a sign-extended `i32` (the upper half is dropped by the caller).
    Int,
    /// One integer slot read as an `i64`.
    Long,
    /// One integer slot read as a pointer value.
    Ptr,
    /// One `double`, as bits.
    Double,
    /// One `long double`, as bits.
    LongDouble,
    /// One integer-class aggregate of the given size in bytes.
    Agg(u8),
}

impl VaClass {
    /// The reader operation that fetches an argument of this class.
    #[must_use]
    pub const fn kind(self) -> VaKind {
        match self {
            Self::Int => VaKind::Int,
            Self::Long => VaKind::Long,
            Self::Ptr => VaKind::Ptr,
            Self::Double => VaKind::Double,
            Self::LongDouble => VaKind::LongDouble,
            Self::Agg { size, .. } => VaKind::Agg(size),
        }
    }
}

/// The reader operations a `v*` target and the tests use on top of [`VaReader`].
///
/// The three scalar methods have default bodies over [`VaReader::next_u64`]; the aggregate
/// method is per architecture because the register/stack rules differ (C59 section 4.5).
pub trait VaReaderExt: VaReader {
    /// The next `int` argument, truncated from its 8-byte slot.
    fn next_i32(&mut self) -> i32 {
        self.next_u64() as i32
    }

    /// The next `long` argument.
    fn next_i64(&mut self) -> i64 {
        self.next_u64() as i64
    }

    /// The next pointer-valued argument.
    fn next_ptr(&mut self) -> usize {
        self.next_u64() as usize
    }

    /// Read one integer-class aggregate of `size` bytes and `align` bytes of alignment.
    ///
    /// The supported subset is sizes 8, 12, 16, 24 and 32 with `align == 8`; anything else
    /// returns `false` and consumes nothing. On success the first `size` bytes of `out` hold
    /// the aggregate and the rest of `out` is untouched.
    fn next_aggregate_int(&mut self, size: usize, align: usize, out: &mut [u8; 32]) -> bool;
}

/// Round `value` up to the next multiple of 16.
#[inline]
pub(crate) fn align_up_16(value: usize) -> usize {
    value.wrapping_add(15) & !15usize
}

/// Round `value` up to the next multiple of 8.
#[inline]
pub(crate) fn align_up_8(value: usize) -> usize {
    value.wrapping_add(7) & !7usize
}

/// The documented integer-class aggregate subset: sizes 8, 12, 16, 24, 32 and alignment 8.
#[inline]
pub(crate) fn aggregate_in_subset(size: usize, align: usize) -> bool {
    align == 8 && matches!(size, 8 | 12 | 16 | 24 | 32)
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "mod_ut.rs"]
mod tests;
