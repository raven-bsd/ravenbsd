// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! Static TLS offsets and the thread-block state (the C52/C57 arithmetic).
#![allow(clippy::needless_range_loop)] // the fixed module array is index-addressed

use raven_libc_elf::ObjId;
use raven_libc_elf::consts::{TLS_TCB_SIZE, TLS_TP_OFFSET_RISCV};
use raven_libc_tls::layout::{
    LayoutError, StaticLayout, TlsModuleIn, compute_static_layout, module_offsets,
};

/// The TP-relative offset stored in the argument word of a `TLSDESC` descriptor.
///
/// The resolver returns this value and the caller adds the thread pointer; the formula is the
/// same one [`raven_libc_elf::reloc`] uses for `R_*_TPOFF64`, kept here so the target applier
/// and the host tests share it.
#[must_use]
pub fn desc_argument(
    arch: raven_libc_elf::Arch,
    sym_value: u64,
    addend: i64,
    module_offset: i64,
) -> u64 {
    let value = sym_value.wrapping_add_signed(addend);
    match arch {
        raven_libc_elf::Arch::X86_64 => value.wrapping_sub(module_offset as u64),
        raven_libc_elf::Arch::Aarch64 => value.wrapping_add(module_offset as u64),
        raven_libc_elf::Arch::Riscv64 => value
            .wrapping_add(module_offset as u64)
            .wrapping_sub(TLS_TP_OFFSET_RISCV)
            .wrapping_sub(TLS_TCB_SIZE),
    }
}

/// The loader's TLS bookkeeping.
#[derive(Clone, Copy, Debug)]
pub struct TlsState {
    /// The current static layout.
    pub layout: StaticLayout,
    /// The module descriptions in assignment order.
    pub modules: [TlsModuleIn; raven_libc_tls::consts::MAX_TLS_MODULES],
    /// The module offsets.
    pub offsets: [usize; raven_libc_tls::consts::MAX_TLS_MODULES],
    /// Number of assigned modules.
    pub count: usize,
    /// The extra static TLS bytes for `dlopen`.
    pub static_tls_extra: usize,
}

impl TlsState {
    /// An empty TLS state.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            layout: StaticLayout {
                size: 0,
                offset: 0,
                align: 16,
                total: 0,
                tls_cnt: 0,
            },
            modules: [TlsModuleIn::new(0, 0, 0, 1); raven_libc_tls::consts::MAX_TLS_MODULES],
            offsets: [0; raven_libc_tls::consts::MAX_TLS_MODULES],
            count: 0,
            static_tls_extra: 1664,
        }
    }

    /// Assign one module offset after the already assigned modules.
    pub fn assign(&mut self, module: TlsModuleIn) -> Result<usize, LayoutError> {
        if self.count >= raven_libc_tls::consts::MAX_TLS_MODULES {
            return Err(LayoutError::TooLarge);
        }
        let mut modules = self.modules;
        modules[self.count] = module;
        let next = self.count.saturating_add(1);
        let layout = compute_static_layout(&modules[..next], 64)?;
        let offsets = module_offsets(&modules[..next])?;
        self.modules = modules;
        self.offsets = offsets.offsets;
        self.count = next;
        self.layout = layout;
        Ok(offsets
            .offsets
            .get(self.count.saturating_sub(1))
            .copied()
            .unwrap_or(0))
    }

    /// The offset of `id`.
    #[must_use]
    pub fn offset(&self, id: ObjId) -> Option<usize> {
        self.offsets.get(usize::try_from(id.0).ok()?).copied()
    }
}

impl Default for TlsState {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
#[path = "tls_ut.rs"]
mod tests;
