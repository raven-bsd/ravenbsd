// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Runtime view of the `#[repr(C)]` layout table generated for the current target.

/// One field of a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldLayout {
    /// Field name.
    pub name: &'static str,
    /// Byte offset.
    pub offset: usize,
    /// Size in bytes.
    pub size: usize,
}

/// One record of the external ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructLayout {
    /// Table name.
    pub name: &'static str,
    /// C spelling.
    pub c_name: &'static str,
    /// Size in bytes.
    pub size: usize,
    /// Alignment in bytes.
    pub align: usize,
    /// Fields in declaration order.
    pub fields: &'static [FieldLayout],
}

pub use crate::generated::layout_table::STRUCTS;

/// Find a record by its table name.
#[must_use]
pub fn find(name: &str) -> Option<&'static StructLayout> {
    STRUCTS.iter().find(|layout| layout.name == name)
}

#[cfg(test)]
#[path = "layout_ut.rs"]
mod tests;
