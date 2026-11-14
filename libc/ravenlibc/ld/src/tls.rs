// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! Static TLS offsets and the thread-block state (the C52/C57 arithmetic).
#![allow(clippy::needless_range_loop)] // the fixed module array is index-addressed

use raven_libc_elf::ObjId;
use raven_libc_tls::layout::{
    LayoutError, StaticLayout, TlsModuleIn, compute_static_layout, module_offsets,
};

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
