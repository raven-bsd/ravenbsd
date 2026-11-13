// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `dl_iterate_phdr` data model.
//!
//! Field order follows `struct dl_phdr_info` (13 3.17.2): `dlpi_addr`, `dlpi_name`, `dlpi_phdr`,
//! `dlpi_phnum`, then the generation counters and TLS fields. The C ABI struct itself lives in
//! the header crate; this is the Rust-side record the loader fills.

use alloc::vec::Vec;

use crate::scope::{ObjId, ObjTable};

/// One `dl_phdr_info` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhdrInfo {
    /// `dlpi_addr`: the load bias.
    pub addr: u64,
    /// `dlpi_name`: the object path (empty for the main program).
    pub name: Vec<u8>,
    /// `dlpi_phdr`: absolute address of the program header array and its count.
    pub phdr: (u64, u16),
    /// `dlpi_adds`: objects added so far.
    pub adds: u64,
    /// `dlpi_subs`: objects removed so far.
    pub subs: u64,
    /// `dlpi_tls_modid`.
    pub tls_modid: u64,
}

impl PhdrInfo {
    /// Build one record from an object view.
    pub fn new(
        bias: u64,
        name: Vec<u8>,
        phdr: (u64, u16),
        adds: u64,
        subs: u64,
        tls_modid: u64,
    ) -> Self {
        Self {
            addr: bias,
            name,
            phdr,
            adds,
            subs,
            tls_modid,
        }
    }

    /// The number of pointer-sized fields the C struct exposes in v1.
    pub const fn field_count() -> usize {
        8
    }
}

/// The iteration order: the main program first, then the global scope's DAG in order, each
/// object once.
pub fn phdr_info_list(
    table: &dyn ObjTable,
    scope: &crate::scope::Scope,
    adds: u64,
    subs: u64,
    tls_modid: &dyn Fn(ObjId) -> u64,
) -> Vec<PhdrInfo> {
    let mut order: Vec<ObjId> = Vec::new();
    order.push(scope.main);
    order.extend_from_slice(&crate::scope::dag_members(table, scope.main));
    for root in &scope.global {
        order.extend_from_slice(&crate::scope::dag_members(table, *root));
    }
    let mut records = Vec::new();
    for id in order {
        if records.iter().any(|record: &PhdrInfo| {
            // Deduplicate by the program-header address, which is unique per object.
            table
                .view(id)
                .map(|view| view.phdr.0 == record.phdr.0)
                .unwrap_or(false)
        }) {
            continue;
        }
        if let Some(view) = table.view(id) {
            records.push(PhdrInfo::new(
                view.bias,
                Vec::new(),
                view.phdr,
                adds,
                subs,
                tls_modid(id),
            ));
        }
    }
    records
}

#[cfg(test)]
#[path = "phdr_info_ut.rs"]
mod tests;
