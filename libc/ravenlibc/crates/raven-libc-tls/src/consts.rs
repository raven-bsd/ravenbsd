// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:arch/x86_64/pthread_arch.h, arch/aarch64/pthread_arch.h, arch/riscv64/pthread_arch.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor files have no per-file header; covered by musl/COPYRIGHT
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! Per-architecture TLS ABI constants.
//!
//! `TLS_ABOVE_TP` selects variant I (the module blocks live above the thread pointer, the TCB
//! ends just below it); its absence selects variant II (the TCB is at `tp` and the blocks live
//! below it). The values are the musl `pthread_arch.h` values of the three targets.

/// Minimum alignment of the TLS block and of `Pthread` (RD-new 3: musl uses 8).
pub const MIN_TLS_ALIGN: usize = 16;

/// Largest `p_align` accepted from a `PT_TLS` header; larger values are capped.
pub const MAX_TLS_ALIGN: usize = 4096;

/// The maximum number of TLS modules this crate models (one static plus loaded objects).
pub const MAX_TLS_MODULES: usize = 16;

/// Two pointer-sized words of DTV headroom in `tls_size` (`2*sizeof(void*)` in the donor).
pub const DTV_HEADROOM: usize = 2 * core::mem::size_of::<usize>();

/// The TLS ABI variant selected by a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abi {
    /// Variant II: `tp` points at the TCB; module blocks lie below it; `dtv` is at the block base.
    VariantII {
        /// Offset added to every `dtv` entry (zero on x86-64).
        dtp_offset: usize,
    },
    /// Variant I: module blocks lie above the thread pointer; the TCB ends below it.
    VariantI {
        /// Gap between `tp` and the first TLS byte.
        gap_above_tp: usize,
        /// Offset added to every `dtv` entry (0x800 on RISC-V).
        dtp_offset: usize,
    },
}

impl Abi {
    /// Whether this variant keeps the TLS blocks above the thread pointer.
    #[must_use]
    pub const fn is_above_tp(self) -> bool {
        matches!(self, Self::VariantI { .. })
    }

    /// The `GAP_ABOVE_TP` value of this variant (0 for variant II).
    #[must_use]
    pub const fn gap_above_tp(self) -> usize {
        match self {
            Self::VariantII { .. } => 0,
            Self::VariantI { gap_above_tp, .. } => gap_above_tp,
        }
    }

    /// The `DTP_OFFSET` value of this variant.
    #[must_use]
    pub const fn dtp_offset(self) -> usize {
        match self {
            Self::VariantII { dtp_offset } | Self::VariantI { dtp_offset, .. } => dtp_offset,
        }
    }
}

/// x86-64 uses TLS variant II (`tp` = `Pthread` address, blocks below).
#[cfg(target_arch = "x86_64")]
pub const TARGET_ABI: Abi = Abi::VariantII { dtp_offset: 0 };

/// AArch64 uses TLS variant I with a 16-byte gap above `tp`.
#[cfg(target_arch = "aarch64")]
pub const TARGET_ABI: Abi = Abi::VariantI {
    gap_above_tp: 16,
    dtp_offset: 0,
};

/// RISC-V uses TLS variant I; `DTP_OFFSET` biases every `dtv` entry by 0x800.
#[cfg(target_arch = "riscv64")]
pub const TARGET_ABI: Abi = Abi::VariantI {
    gap_above_tp: 0,
    dtp_offset: 0x800,
};

/// `GAP_ABOVE_TP` of the target (0 on x86-64 and RISC-V, 16 on AArch64).
pub const GAP_ABOVE_TP: usize = TARGET_ABI.gap_above_tp();

/// `DTP_OFFSET` of the target (0 apart from the RISC-V 0x800).
pub const DTP_OFFSET: usize = TARGET_ABI.dtp_offset();

/// `true` on variant I targets (AArch64 and RISC-V).
pub const TLS_ABOVE_TP: bool = TARGET_ABI.is_above_tp();

/// `TP_OFFSET` (musl `TP_ADJ`); zero on all three Raven targets.
pub const TP_OFFSET: usize = 0;

/// Size of the static `builtin_tls` buffer: small executables need no mapping.
pub const BUILTIN_TLS_SIZE: usize = 4096;

#[cfg(test)]
#[path = "consts_ut.rs"]
mod tests;
