// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C `dl_phdr_info` record and the host-testable iteration.

use raven_libc_elf::ObjId;

use crate::loadobj::Loader;
use crate::mapper::Platform;

/// The C ABI `struct dl_phdr_info` (Linux layout).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DlPhdrInfo {
    /// Load address.
    pub dlpi_addr: usize,
    /// NUL-terminated object name.
    pub dlpi_name: *const u8,
    /// Program headers.
    pub dlpi_phdr: *const u8,
    /// Number of program headers.
    pub dlpi_phnum: u16,
    /// Objects added.
    pub dlpi_adds: u64,
    /// Objects removed.
    pub dlpi_subs: u64,
    /// Static TLS module id, or zero.
    pub dlpi_tls_modid: usize,
    /// Static TLS block address, or null.
    pub dlpi_tls_data: *mut u8,
}

impl<P: Platform, M: raven_libc_elf::MemView, A: raven_libc_elf::RelocApplier> Loader<P, M, A> {
    /// The `dl_phdr_info` record for one object.
    #[must_use]
    pub fn dl_phdr_info(&self, id: ObjId) -> Option<DlPhdrInfo> {
        let object = self.object(id)?;
        let name = object
            .names
            .iter()
            .take(object.name_count)
            .next()
            .map_or(core::ptr::null(), |name| name.as_bytes().as_ptr());
        Some(DlPhdrInfo {
            dlpi_addr: usize::try_from(object.mapped.base).ok()?,
            dlpi_name: name,
            dlpi_phdr: usize::try_from(object.view.phdr.0)
                .ok()
                .map_or(core::ptr::null(), |address| address as *const u8),
            dlpi_phnum: object.view.phdr.1,
            dlpi_adds: u64::from(self.adds),
            dlpi_subs: u64::from(self.subs),
            dlpi_tls_modid: object.tls_offset,
            dlpi_tls_data: core::ptr::null_mut(),
        })
    }

    /// Iterate the main-first `dl_phdr_info` list; a non-zero callback result stops the walk.
    pub fn iterate_phdr_info<F: FnMut(&DlPhdrInfo) -> i32>(&self, mut callback: F) -> i32 {
        for id in self.iterate_ids().into_iter().flatten() {
            if let Some(info) = self.dl_phdr_info(id) {
                let result = callback(&info);
                if result != 0 {
                    return result;
                }
            }
        }
        0
    }
}
