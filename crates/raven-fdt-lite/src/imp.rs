// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal flattened-device-tree reader.
//!
//! Supports the parts of the Devicetree Specification v0.4 the kernel needs: header validation,
//! node lookup by path, compatible-string search, property access and `reg`/`interrupts` cell
//! decoding. The structure and strings blocks are bounds-checked on every access; no allocation,
//! no panics. Memory-reservation entries are skipped, not interpreted.

use core::fmt;

/// Magic number of a flattened device tree (big endian in the blob).
pub const FDT_MAGIC: u32 = 0xd00d_feed;
/// Token opening a node; followed by a NUL-terminated name padded to 4 bytes.
pub const FDT_BEGIN_NODE: u32 = 1;
/// Token closing a node.
pub const FDT_END_NODE: u32 = 2;
/// Token introducing a property: `len`, `nameoff`, then `len` value bytes padded to 4.
pub const FDT_PROP: u32 = 3;
/// No-op token.
pub const FDT_NOP: u32 = 4;
/// Token ending the structure block.
pub const FDT_END: u32 = 9;

/// Bytes of the flattened header.
const HEADER_BYTES: usize = 40;
/// Highest structure version this reader accepts (`last_comp_version` may be 16).
const MAX_VERSION: u32 = 17;
/// Minimum structure version.
const MIN_VERSION: u32 = 16;

/// Why the blob could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FdtError {
    /// Shorter than the 40-byte header.
    TooShort,
    /// The magic number is not `0xd00d_feed`.
    BadMagic,
    /// `totalsize` is smaller than the header or larger than the slice.
    BadTotalsize,
    /// The version is not 16 or 17.
    BadVersion,
    /// A block offset/size leaves the blob.
    BadOffsets,
    /// A token or name/value range leaves the structure or strings block.
    Truncated,
    /// A token outside the specification.
    BadToken,
    /// A property name is not valid UTF-8 or has no terminator.
    BadString,
    /// A `reg` value is not a whole number of cells.
    BadCells,
}

impl fmt::Display for FdtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            FdtError::TooShort => "blob shorter than the header",
            FdtError::BadMagic => "bad device-tree magic",
            FdtError::BadTotalsize => "bad total size",
            FdtError::BadVersion => "unsupported device-tree version",
            FdtError::BadOffsets => "block offset out of bounds",
            FdtError::Truncated => "truncated device tree",
            FdtError::BadToken => "unknown structure token",
            FdtError::BadString => "bad property or node string",
            FdtError::BadCells => "property is not a whole number of cells",
        };
        f.write_str(message)
    }
}

/// A parsed device tree borrowing the blob.
#[derive(Clone, Copy, Debug)]
pub struct Fdt<'a> {
    data: &'a [u8],
    off_struct: usize,
    size_struct: usize,
    off_strings: usize,
    size_strings: usize,
}

impl<'a> Fdt<'a> {
    /// Validate the header and wrap `data`.
    pub fn parse(data: &'a [u8]) -> Result<Self, FdtError> {
        if data.len() < HEADER_BYTES {
            return Err(FdtError::TooShort);
        }
        let magic = read_be_u32(data, 0).ok_or(FdtError::TooShort)?;
        if magic != FDT_MAGIC {
            return Err(FdtError::BadMagic);
        }
        let totalsize = read_be_u32(data, 4).ok_or(FdtError::TooShort)? as usize;
        if totalsize < HEADER_BYTES || totalsize > data.len() {
            return Err(FdtError::BadTotalsize);
        }
        let version = read_be_u32(data, 20).ok_or(FdtError::TooShort)?;
        if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
            return Err(FdtError::BadVersion);
        }
        let off_struct = read_be_u32(data, 8).ok_or(FdtError::TooShort)? as usize;
        let off_strings = read_be_u32(data, 12).ok_or(FdtError::TooShort)? as usize;
        let size_strings = read_be_u32(data, 32).ok_or(FdtError::TooShort)? as usize;
        let size_struct = read_be_u32(data, 36).ok_or(FdtError::TooShort)? as usize;
        if off_struct
            .checked_add(size_struct)
            .is_none_or(|end| end > totalsize)
            || off_strings
                .checked_add(size_strings)
                .is_none_or(|end| end > totalsize)
        {
            return Err(FdtError::BadOffsets);
        }
        Ok(Self {
            data: data.get(..totalsize).ok_or(FdtError::BadTotalsize)?,
            off_struct,
            size_struct,
            off_strings,
            size_strings,
        })
    }

    /// The root node.
    pub fn root(&self) -> Option<Node<'a>> {
        let pos = 0usize;
        if self.struct_u32(pos)? != FDT_BEGIN_NODE {
            return None;
        }
        let name_start = pos.checked_add(4)?;
        let (name, _) = self.struct_name(name_start)?;
        if !name.is_empty() {
            return None;
        }
        Some(Node {
            fdt: *self,
            name_start,
        })
    }

    /// Look up a node by absolute path such as `/intc@8000000` or `/soc/serial`.
    ///
    /// A path component matches a node whose name is either exactly the component or starts with
    /// the component followed by `@` (the unit address may be omitted).
    pub fn find_node(&self, path: &str) -> Option<Node<'a>> {
        if !path.starts_with('/') {
            return None;
        }
        let mut node = self.root()?;
        for component in path.split('/').filter(|part| !part.is_empty()) {
            node = node.find_child(component)?;
        }
        Some(node)
    }

    /// Find the first node (in depth-first order) with a `compatible` entry equal to one of
    /// `names`.
    pub fn find_compatible(&self, names: &[&str]) -> Option<Node<'a>> {
        let mut pos = 0usize;
        loop {
            let token = self.struct_u32(pos)?;
            match token {
                FDT_BEGIN_NODE => {
                    let name_start = pos.checked_add(4)?;
                    let (_, after_name) = self.struct_name(name_start)?;
                    let node = Node {
                        fdt: *self,
                        name_start,
                    };
                    if names.iter().any(|name| node.compatible_with(name)) {
                        return Some(node);
                    }
                    // Continue with the children: `after_name` is the first token after the
                    // name, which is where the node's property/child tokens start.
                    pos = after_name;
                }
                FDT_PROP => pos = self.after_property(pos)?,
                FDT_NOP => pos = pos.checked_add(4)?,
                FDT_END_NODE => pos = pos.checked_add(4)?,
                FDT_END => return None,
                _ => return None,
            }
        }
    }

    fn struct_u32(&self, offset: usize) -> Option<u32> {
        let end = offset.checked_add(4)?;
        if end > self.size_struct {
            return None;
        }
        read_be_u32(self.data, self.off_struct.checked_add(offset)?)
    }

    fn struct_slice(&self, offset: usize, len: usize) -> Option<&'a [u8]> {
        let end = offset.checked_add(len)?;
        if end > self.size_struct {
            return None;
        }
        let start = self.off_struct.checked_add(offset)?;
        self.data.get(start..start.checked_add(len)?)
    }

    fn struct_name(&self, offset: usize) -> Option<(&'a str, usize)> {
        let remaining = self.size_struct.checked_sub(offset)?;
        let bytes = self.struct_slice(offset, remaining)?;
        let nul = bytes.iter().position(|byte| *byte == 0)?;
        let name = core::str::from_utf8(bytes.get(..nul)?).ok()?;
        let after = align4(offset.checked_add(nul)?.checked_add(1)?)?;
        Some((name, after))
    }

    fn string_at(&self, offset: usize) -> Option<&'a str> {
        let remaining = self.size_strings.checked_sub(offset)?;
        let start = self.off_strings.checked_add(offset)?;
        let bytes = self.data.get(start..start.checked_add(remaining)?)?;
        let nul = bytes.iter().position(|byte| *byte == 0)?;
        core::str::from_utf8(bytes.get(..nul)?).ok()
    }

    fn after_property(&self, offset: usize) -> Option<usize> {
        let len = self.struct_u32(offset.checked_add(4)?)? as usize;
        let value = offset.checked_add(12)?;
        align4(value.checked_add(len)?)
    }
}

fn align4(value: usize) -> Option<usize> {
    value.checked_add(3).map(|rounded| rounded & !3)
}

fn read_be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes: [u8; 4] = data.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

fn read_be_u64(data: &[u8], offset: usize) -> Option<u64> {
    let bytes: [u8; 8] = data.get(offset..offset.checked_add(8)?)?.try_into().ok()?;
    Some(u64::from_be_bytes(bytes))
}

/// One node in the tree.
#[derive(Clone, Copy, Debug)]
pub struct Node<'a> {
    fdt: Fdt<'a>,
    /// Offset in the structure block of the node's NUL-terminated name.
    name_start: usize,
}

impl<'a> Node<'a> {
    /// The node name as written, including any unit address (`intc@8000000`).
    pub fn name(&self) -> &'a str {
        self.fdt
            .struct_name(self.name_start)
            .map(|(name, _)| name)
            .unwrap_or("")
    }

    /// The node name including any unit address, e.g. `intc@8000000`.
    pub fn full_name(&self) -> &'a str {
        self.name()
    }

    /// A property's raw value.
    pub fn property(&self, name: &str) -> Option<&'a [u8]> {
        let (_, mut pos) = self.fdt.struct_name(self.name_start)?;
        loop {
            let token = self.fdt.struct_u32(pos)?;
            match token {
                FDT_PROP => {
                    let len = self.fdt.struct_u32(pos.checked_add(4)?)? as usize;
                    let name_offset = self.fdt.struct_u32(pos.checked_add(8)?)? as usize;
                    let value_start = pos.checked_add(12)?;
                    if self.fdt.string_at(name_offset)? == name {
                        return self.fdt.struct_slice(value_start, len);
                    }
                    pos = align4(value_start.checked_add(len)?)?;
                }
                FDT_NOP => pos = pos.checked_add(4)?,
                FDT_BEGIN_NODE | FDT_END_NODE | FDT_END => return None,
                _ => return None,
            }
        }
    }

    /// The first string of a string-list property (for example `compatible`).
    pub fn property_str(&self, name: &str) -> Option<&'a str> {
        let value = self.property(name)?;
        let nul = value.iter().position(|byte| *byte == 0)?;
        core::str::from_utf8(value.get(..nul)?).ok()
    }

    /// True when the `compatible` string list contains `name`.
    pub fn compatible_with(&self, name: &str) -> bool {
        let Some(value) = self.property("compatible") else {
            return false;
        };
        value
            .split(|byte| *byte == 0)
            .any(|entry| entry == name.as_bytes())
    }

    /// A 32-bit big-endian scalar property (for example `#address-cells`).
    pub fn u32_property(&self, name: &str) -> Option<u32> {
        let value = self.property(name)?;
        if value.len() != 4 {
            return None;
        }
        read_be_u32(value, 0)
    }

    /// A 64-bit big-endian scalar property.
    pub fn u64_property(&self, name: &str) -> Option<u64> {
        let value = self.property(name)?;
        if value.len() != 8 {
            return None;
        }
        read_be_u64(value, 0)
    }

    /// Decode a property as big-endian cells of `cell_bytes` (4 or 8) bytes each.
    pub fn cells(&self, name: &str, cell_bytes: usize) -> Cells<'a> {
        Cells {
            value: self.property(name),
            cell_bytes,
            position: 0,
        }
    }

    fn find_child(&self, component: &str) -> Option<Node<'a>> {
        let (_, mut pos) = self.fdt.struct_name(self.name_start)?;
        loop {
            let token = self.fdt.struct_u32(pos)?;
            match token {
                FDT_BEGIN_NODE => {
                    let name_start = pos.checked_add(4)?;
                    let (name, after_name) = self.fdt.struct_name(name_start)?;
                    if matches_component(name, component) {
                        return Some(Node {
                            fdt: self.fdt,
                            name_start,
                        });
                    }
                    pos = self.skip_subtree(after_name)?;
                }
                FDT_PROP => pos = self.fdt.after_property(pos)?,
                FDT_NOP => pos = pos.checked_add(4)?,
                FDT_END_NODE | FDT_END => return None,
                _ => return None,
            }
        }
    }

    fn skip_subtree(&self, start: usize) -> Option<usize> {
        let mut pos = start;
        let mut depth = 1usize;
        loop {
            let token = self.fdt.struct_u32(pos)?;
            match token {
                FDT_BEGIN_NODE => {
                    let (_, after) = self.fdt.struct_name(pos.checked_add(4)?)?;
                    depth = depth.checked_add(1)?;
                    pos = after;
                }
                FDT_END_NODE => {
                    depth = depth.checked_sub(1)?;
                    pos = pos.checked_add(4)?;
                    if depth == 0 {
                        return Some(pos);
                    }
                }
                FDT_PROP => pos = self.fdt.after_property(pos)?,
                FDT_NOP => pos = pos.checked_add(4)?,
                FDT_END => return None,
                _ => return None,
            }
        }
    }

    /// `reg` entries decoded as `(address, size)` pairs using the given cell counts.
    ///
    /// Yields nothing when the property is absent; stops at the first incomplete entry. Callers
    /// that need to detect a malformed value use [`Node::reg_checked`].
    pub fn reg(&self, address_cells: u32, size_cells: u32) -> Reg<'a> {
        Reg {
            cells: self.cells("reg", 4),
            address_cells: address_cells as usize,
            size_cells: size_cells as usize,
        }
    }

    /// `reg` entries with malformed-cell detection.
    pub fn reg_checked(&self, address_cells: u32, size_cells: u32) -> Result<Reg<'a>, FdtError> {
        let value = self.property("reg").ok_or(FdtError::BadCells)?;
        let entry_cells = (address_cells as usize)
            .checked_add(size_cells as usize)
            .ok_or(FdtError::BadCells)?;
        if entry_cells == 0 || !value.len().is_multiple_of(entry_cells * 4) {
            return Err(FdtError::BadCells);
        }
        Ok(self.reg(address_cells, size_cells))
    }
}

fn matches_component(name: &str, component: &str) -> bool {
    name == component
        || name
            .strip_prefix(component)
            .is_some_and(|rest| rest.starts_with('@'))
}

/// Iterator over big-endian cells of a property.
#[derive(Clone, Copy, Debug)]
pub struct Cells<'a> {
    value: Option<&'a [u8]>,
    cell_bytes: usize,
    position: usize,
}

impl Iterator for Cells<'_> {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        let value = self.value?;
        if self.cell_bytes != 4 && self.cell_bytes != 8 {
            return None;
        }
        let end = self.position.checked_add(self.cell_bytes)?;
        let bytes = value.get(self.position..end)?;
        self.position = end;
        if self.cell_bytes == 4 {
            read_be_u32(bytes, 0).map(u64::from)
        } else {
            read_be_u64(bytes, 0)
        }
    }
}

/// Iterator over `reg` entries.
#[derive(Clone, Copy, Debug)]
pub struct Reg<'a> {
    cells: Cells<'a>,
    address_cells: usize,
    size_cells: usize,
}

impl Iterator for Reg<'_> {
    type Item = (u64, u64);

    fn next(&mut self) -> Option<(u64, u64)> {
        let mut address = 0u64;
        let mut index = 0usize;
        while index < self.address_cells {
            address = address.checked_shl(32)?.checked_add(self.cells.next()?)?;
            index = index.checked_add(1)?;
        }
        let mut size = 0u64;
        index = 0;
        while index < self.size_cells {
            size = size.checked_shl(32)?.checked_add(self.cells.next()?)?;
            index = index.checked_add(1)?;
        }
        Some((address, size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny FDT writer used only by the tests.
    struct Builder {
        structure: Vec<u8>,
        strings: Vec<u8>,
    }

    impl Builder {
        fn new() -> Self {
            Self {
                structure: Vec::new(),
                strings: Vec::new(),
            }
        }

        fn token(&mut self, token: u32) {
            self.structure.extend_from_slice(&token.to_be_bytes());
        }

        fn string_offset(&mut self, value: &str) -> u32 {
            let offset = self.strings.len() as u32;
            self.strings.extend_from_slice(value.as_bytes());
            self.strings.push(0);
            offset
        }

        fn begin(&mut self, name: &str) {
            self.token(FDT_BEGIN_NODE);
            self.structure.extend_from_slice(name.as_bytes());
            self.structure.push(0);
            while !self.structure.len().is_multiple_of(4) {
                self.structure.push(0);
            }
        }

        fn end(&mut self) {
            self.token(FDT_END_NODE);
        }

        fn prop(&mut self, name: &str, value: &[u8]) {
            let name_offset = self.string_offset(name);
            self.token(FDT_PROP);
            self.structure
                .extend_from_slice(&(value.len() as u32).to_be_bytes());
            self.structure.extend_from_slice(&name_offset.to_be_bytes());
            self.structure.extend_from_slice(value);
            while !self.structure.len().is_multiple_of(4) {
                self.structure.push(0);
            }
        }

        fn u32_prop(&mut self, name: &str, value: u32) {
            self.prop(name, &value.to_be_bytes());
        }

        fn nop(&mut self) {
            self.token(FDT_NOP);
        }

        fn finish(&mut self) -> Vec<u8> {
            self.token(FDT_END);
            let struct_offset = 48usize;
            let strings_offset = struct_offset + self.structure.len();
            let totalsize = strings_offset + self.strings.len();
            let mut blob = Vec::with_capacity(totalsize);
            blob.extend_from_slice(&FDT_MAGIC.to_be_bytes());
            blob.extend_from_slice(&(totalsize as u32).to_be_bytes());
            blob.extend_from_slice(&(struct_offset as u32).to_be_bytes());
            blob.extend_from_slice(&(strings_offset as u32).to_be_bytes());
            blob.extend_from_slice(&40u32.to_be_bytes()); // memory reservation block
            blob.extend_from_slice(&17u32.to_be_bytes()); // version
            blob.extend_from_slice(&16u32.to_be_bytes()); // last compatible version
            blob.extend_from_slice(&0u32.to_be_bytes()); // boot cpuid
            blob.extend_from_slice(&(self.strings.len() as u32).to_be_bytes());
            blob.extend_from_slice(&(self.structure.len() as u32).to_be_bytes());
            blob.extend_from_slice(&[0u8; 8]); // empty memory reservation entry
            blob.extend_from_slice(&self.structure);
            blob.extend_from_slice(&self.strings);
            blob
        }
    }

    fn sample() -> Vec<u8> {
        let mut builder = Builder::new();
        builder.begin("");
        builder.prop("compatible", b"test,root\0");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 2);
        builder.prop("test64", &[0, 0, 0, 1, 0, 0, 0, 2]);

        builder.begin("intc@8000000");
        builder.prop("compatible", b"arm,cortex-a15-gic\0arm,gic-400\0");
        builder.prop(
            "reg",
            &[
                0, 0, 0, 0, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, // 0x0800_0000 + 0x1_0000
                0, 0, 0, 0, 0x08, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, // 0x0801_0000 + 0x1_0000
            ],
        );
        builder.u32_prop("#interrupt-cells", 3);
        builder.begin("nested");
        builder.u32_prop("value", 7);
        builder.end();
        builder.end();

        builder.begin("timer");
        builder.prop("compatible", b"arm,armv8-timer\0");
        builder.prop(
            "interrupts",
            &[
                0, 0, 0, 1, 0, 0, 0, 13, 0, 0, 0x0f, 4, 0, 0, 0, 1, 0, 0, 0, 14, 0, 0, 0x0f, 4,
            ],
        );
        builder.nop();
        builder.end();

        builder.begin("serial@9000000");
        builder.prop("compatible", b"arm,pl011\0");
        builder.end();
        builder.end();
        builder.finish()
    }

    fn without_reg_entries() -> Vec<u8> {
        let mut builder = Builder::new();
        builder.begin("");
        builder.begin("bad");
        builder.prop("reg", &[0, 0, 0, 0, 0, 0]);
        builder.end();
        builder.end();
        builder.finish()
    }

    #[test]
    fn parses_a_valid_blob() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).expect("valid");
        assert_eq!(fdt.root().expect("root").name(), "");
    }

    #[test]
    fn rejects_a_short_blob() {
        assert_eq!(Fdt::parse(&[0u8; 39]).unwrap_err(), FdtError::TooShort);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut blob = sample();
        blob[0..4].copy_from_slice(&0xdead_beefu32.to_be_bytes());
        assert_eq!(Fdt::parse(&blob).unwrap_err(), FdtError::BadMagic);
    }

    #[test]
    fn rejects_small_totalsize() {
        let mut blob = sample();
        blob[4..8].copy_from_slice(&8u32.to_be_bytes());
        assert_eq!(Fdt::parse(&blob).unwrap_err(), FdtError::BadTotalsize);
    }

    #[test]
    fn rejects_large_totalsize() {
        let mut blob = sample();
        let huge = (blob.len() as u32).saturating_add(4096);
        blob[4..8].copy_from_slice(&huge.to_be_bytes());
        assert_eq!(Fdt::parse(&blob).unwrap_err(), FdtError::BadTotalsize);
    }

    #[test]
    fn rejects_unknown_version() {
        let mut blob = sample();
        blob[20..24].copy_from_slice(&99u32.to_be_bytes());
        assert_eq!(Fdt::parse(&blob).unwrap_err(), FdtError::BadVersion);
    }

    #[test]
    fn rejects_structure_offset_outside_the_blob() {
        let mut blob = sample();
        blob[8..12].copy_from_slice(&0xffff_0000u32.to_be_bytes());
        assert_eq!(Fdt::parse(&blob).unwrap_err(), FdtError::BadOffsets);
    }

    #[test]
    fn finds_the_root() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(fdt.find_node("/").unwrap().name(), "");
    }

    #[test]
    fn finds_a_node_by_full_path_with_unit_address() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(
            fdt.find_node("/intc@8000000").unwrap().name(),
            "intc@8000000"
        );
    }

    #[test]
    fn finds_a_node_by_name_without_the_unit_address() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(fdt.find_node("/intc").unwrap().name(), "intc@8000000");
    }

    #[test]
    fn finds_a_nested_node() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let nested = fdt.find_node("/intc/nested").unwrap();
        assert_eq!(nested.u32_property("value"), Some(7));
    }

    #[test]
    fn missing_path_is_none() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert!(fdt.find_node("/missing").is_none());
        assert!(fdt.find_node("relative").is_none());
    }

    #[test]
    fn reads_a_property_value() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let root = fdt.root().unwrap();
        assert_eq!(root.property("compatible"), Some(b"test,root\0".as_slice()));
    }

    #[test]
    fn missing_property_is_none() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert!(fdt.root().unwrap().property("nope").is_none());
    }

    #[test]
    fn reads_the_first_string_of_a_string_list() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let intc = fdt.find_node("/intc").unwrap();
        assert_eq!(intc.property_str("compatible"), Some("arm,cortex-a15-gic"));
    }

    #[test]
    fn compatible_matches_any_entry() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let intc = fdt.find_node("/intc").unwrap();
        assert!(intc.compatible_with("arm,cortex-a15-gic"));
        assert!(intc.compatible_with("arm,gic-400"));
        assert!(!intc.compatible_with("arm,gic-v3"));
    }

    #[test]
    fn find_compatible_locates_the_gic() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let gic = fdt
            .find_compatible(&["arm,cortex-a15-gic", "arm,gic-400"])
            .expect("gic");
        assert_eq!(gic.name(), "intc@8000000");
    }

    #[test]
    fn find_compatible_locates_the_timer() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let timer = fdt.find_compatible(&["arm,armv8-timer"]).expect("timer");
        assert_eq!(timer.name(), "timer");
    }

    #[test]
    fn find_compatible_missing_is_none() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert!(fdt.find_compatible(&["not,here"]).is_none());
    }

    #[test]
    fn reads_a_u32_property() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(fdt.root().unwrap().u32_property("#address-cells"), Some(2));
    }

    #[test]
    fn wrong_length_u32_property_is_none() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(fdt.root().unwrap().u32_property("compatible"), None);
    }

    #[test]
    fn reads_a_u64_property() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(
            fdt.root().unwrap().u64_property("test64"),
            Some(0x0000_0001_0000_0002)
        );
    }

    #[test]
    fn decodes_reg_pairs() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let intc = fdt.find_node("/intc").unwrap();
        let entries: Vec<(u64, u64)> = intc.reg(2, 2).collect();
        assert_eq!(
            entries,
            vec![(0x0800_0000, 0x1_0000), (0x0801_0000, 0x1_0000)]
        );
    }

    #[test]
    fn reg_checked_rejects_partial_entries() {
        let blob = without_reg_entries();
        let fdt = Fdt::parse(&blob).unwrap();
        let bad = fdt.find_node("/bad").unwrap();
        assert_eq!(bad.reg_checked(2, 2).unwrap_err(), FdtError::BadCells);
    }

    #[test]
    fn reg_checked_absent_is_an_error() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let timer = fdt.find_node("/timer").unwrap();
        assert_eq!(timer.reg_checked(2, 2).unwrap_err(), FdtError::BadCells);
    }

    #[test]
    fn decodes_interrupt_cells() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let timer = fdt.find_node("/timer").unwrap();
        let values: Vec<u64> = timer.cells("interrupts", 4).collect();
        assert_eq!(values, vec![1, 13, 0x0f04, 1, 14, 0x0f04]);
    }

    #[test]
    fn cells_reject_an_invalid_cell_size() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let timer = fdt.find_node("/timer").unwrap();
        assert_eq!(timer.cells("interrupts", 3).count(), 0);
    }

    #[test]
    fn nop_tokens_are_skipped() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        // The sample puts a NOP at the end of /timer; the node must still be readable.
        assert!(
            fdt.find_node("/timer")
                .unwrap()
                .compatible_with("arm,armv8-timer")
        );
    }

    #[test]
    fn truncated_structure_is_rejected_without_panicking() {
        let mut blob = sample();
        let len = blob.len();
        blob.truncate(len.saturating_sub(12));
        if let Ok(fdt) = Fdt::parse(&blob) {
            // The header says the structure is longer than the blob: parse must have rejected it.
            panic!("truncated blob parsed: {fdt:?}");
        }
    }

    #[test]
    fn unknown_token_stops_the_walk() {
        let mut blob = sample();
        // The first structure token is at offset 48; make it unknown.
        blob[48..52].copy_from_slice(&0xffff_ffffu32.to_be_bytes());
        let fdt = Fdt::parse(&blob).unwrap();
        assert!(fdt.root().is_none());
        assert!(fdt.find_node("/intc").is_none());
    }

    #[test]
    fn end_token_before_a_node_is_handled() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.end();
        builder.token(FDT_END);
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        assert!(fdt.find_node("/anything").is_none());
    }

    #[test]
    fn empty_root_name_must_be_empty() {
        let mut builder = Builder::new();
        builder.begin("not-the-root");
        builder.end();
        builder.token(FDT_END);
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        assert!(fdt.root().is_none());
    }

    #[test]
    fn property_name_offsets_are_bounds_checked() {
        let mut blob = sample();
        // Corrupt the first property's name offset in the root node.
        // Root BEGIN_NODE is at 48, name is one NUL padded to 4, so the PROP token is at 56.
        let name_offset_at = 64;
        blob[name_offset_at..name_offset_at + 4].copy_from_slice(&0x7fff_ffffu32.to_be_bytes());
        let fdt = Fdt::parse(&blob).unwrap();
        assert!(fdt.root().unwrap().property("compatible").is_none());
    }
}
