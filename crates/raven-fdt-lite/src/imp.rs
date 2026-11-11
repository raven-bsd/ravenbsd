// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal flattened-device-tree reader.
//!
//! Supports the parts of the Devicetree Specification v0.4 the kernel needs: header validation,
//! node lookup by path, compatible-string search, property access and `reg`/`interrupts` cell
//! decoding, address translation through `ranges`, and DMA windows from `dma-ranges`. The
//! structure and strings blocks are bounds-checked on every access; no allocation, no panics.
//! Memory-reservation entries are exposed as read-only `(address, size)` pairs.

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
/// Deepest node nesting the parent scan accepts; real trees are far shallower.
const MAX_NODE_DEPTH: usize = 32;

/// One DMA address window declared by a bus node's `dma-ranges`.
///
/// Devices behind the bus see `bus_base`; the CPU sees the same bytes at `cpu_base`. `size` is
/// the window length in bytes. A driver must place DMA buffers in the CPU range or bounce them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaWindow {
    /// Address as seen by the device on the bus.
    pub bus_base: u64,
    /// Address in the CPU's physical address space.
    pub cpu_base: u64,
    /// Window length in bytes.
    pub size: u64,
}

impl DmaWindow {
    /// A zero-sized window, used as an array initialiser.
    pub const EMPTY: Self = Self {
        bus_base: 0,
        cpu_base: 0,
        size: 0,
    };
}

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
    off_rsv: usize,
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
        let off_rsv = read_be_u32(data, 16).ok_or(FdtError::TooShort)? as usize;
        let size_strings = read_be_u32(data, 32).ok_or(FdtError::TooShort)? as usize;
        let size_struct = read_be_u32(data, 36).ok_or(FdtError::TooShort)? as usize;
        if off_struct
            .checked_add(size_struct)
            .is_none_or(|end| end > totalsize)
            || off_strings
                .checked_add(size_strings)
                .is_none_or(|end| end > totalsize)
            || off_rsv >= totalsize
        {
            return Err(FdtError::BadOffsets);
        }
        Ok(Self {
            data: data.get(..totalsize).ok_or(FdtError::BadTotalsize)?,
            off_struct,
            size_struct,
            off_strings,
            size_strings,
            off_rsv,
        })
    }

    /// Iterate over the memory reservation block.
    ///
    /// Each item is a `(physical address, length)` pair; the terminating all-zero pair is not
    /// yielded. The iterator stops at a truncated entry.
    pub fn reservations(&self) -> Reservations<'a> {
        Reservations {
            fdt: *self,
            position: self.off_rsv,
        }
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

    /// The parent of the node whose name starts at `node_name`, or `None` for the root.
    ///
    /// The structure block is scanned once with a fixed-depth stack; no allocation. A tree deeper
    /// than [`MAX_NODE_DEPTH`] is treated as having no parent.
    fn parent_of(&self, node_name: usize) -> Option<usize> {
        let mut stack = [0usize; MAX_NODE_DEPTH];
        let mut depth = 0usize;
        let mut position = 0usize;
        loop {
            let token = self.struct_u32(position)?;
            match token {
                FDT_BEGIN_NODE => {
                    let name_start = position.checked_add(4)?;
                    if name_start == node_name {
                        return if depth == 0 {
                            None
                        } else {
                            stack.get(depth.checked_sub(1)?).copied()
                        };
                    }
                    let (_, after) = self.struct_name(name_start)?;
                    let slot = stack.get_mut(depth)?;
                    *slot = name_start;
                    depth = depth.checked_add(1)?;
                    position = after;
                }
                FDT_END_NODE => {
                    depth = depth.checked_sub(1)?;
                    position = position.checked_add(4)?;
                    if depth == 0 {
                        return None;
                    }
                }
                FDT_PROP => position = self.after_property(position)?,
                FDT_NOP => position = position.checked_add(4)?,
                FDT_END => return None,
                _ => return None,
            }
        }
    }

    /// Fill `out` with the `dma-ranges` windows of the nearest ancestor that declares them.
    ///
    /// Returns `None` when no ancestor has the property, `Some(0)` when the nearest declaration is
    /// empty (identity), and otherwise the number of windows written. Zero-sized entries are
    /// skipped; a malformed property yields `Some(0)`.
    pub fn dma_windows_for(&self, path: &str, out: &mut [DmaWindow]) -> Option<usize> {
        let node = self.find_node(path)?;
        let mut bus = node.parent();
        while let Some(current) = bus {
            if let Some(count) = dma_windows_of(&current, out) {
                return Some(count);
            }
            bus = current.parent();
        }
        None
    }

    /// The first DMA window of the nearest ancestor that declares `dma-ranges`.
    ///
    /// `None` means either no ancestor declares the property or the nearest declaration is empty
    /// (an identity mapping).
    pub fn dma_window_for(&self, path: &str) -> Option<DmaWindow> {
        let mut windows = [DmaWindow::EMPTY; 8];
        let count = self.dma_windows_for(path, &mut windows)?;
        if count == 0 {
            return None;
        }
        windows.first().copied()
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

    fn skip_subtree(&self, start: usize) -> Option<usize> {
        let mut pos = start;
        let mut depth = 1usize;
        loop {
            let token = self.struct_u32(pos)?;
            match token {
                FDT_BEGIN_NODE => {
                    let (_, after) = self.struct_name(pos.checked_add(4)?)?;
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
                FDT_PROP => pos = self.after_property(pos)?,
                FDT_NOP => pos = pos.checked_add(4)?,
                FDT_END => return None,
                _ => return None,
            }
        }
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

    /// The parent node, or `None` for the root.
    pub fn parent(&self) -> Option<Node<'a>> {
        let name_start = self.fdt.parent_of(self.name_start)?;
        Some(Node {
            fdt: self.fdt,
            name_start,
        })
    }

    /// `#address-cells` of this node, defaulting to the specification's 2.
    pub fn address_cells(&self) -> u32 {
        self.u32_property("#address-cells").unwrap_or(2)
    }

    /// `#size-cells` of this node, defaulting to the specification's 1.
    pub fn size_cells(&self) -> u32 {
        self.u32_property("#size-cells").unwrap_or(1)
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

    /// Iterate over the direct child nodes in tree order.
    ///
    /// Properties and NOP tokens are skipped; grandchildren are not yielded. A malformed tree
    /// ends the iteration.
    pub fn children(&self) -> Children<'a> {
        let start = self
            .fdt
            .struct_name(self.name_start)
            .map(|(_, after)| after);
        Children {
            fdt: self.fdt,
            position: start,
            skip: None,
            done: false,
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

    /// `reg` entries translated to the CPU physical address space through every parent `ranges`.
    ///
    /// The cell counts come from the parent, as the specification defines them for a child's
    /// `reg`. A parent range that does not cover the address ends the iterator; an invalid cell
    /// count (more than two cells) yields nothing. No allocation and no panics.
    pub fn reg_translated(&self) -> TranslatedReg<'a> {
        let (address_cells, size_cells) = match self.parent() {
            Some(parent) => (parent.address_cells(), parent.size_cells()),
            None => (2, 1),
        };
        let valid = address_cells <= 2 && size_cells <= 2;
        TranslatedReg {
            node: *self,
            regs: self.reg(address_cells, size_cells),
            done: !valid,
        }
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

/// Iterator over `reg` entries already translated through the parent `ranges`.
#[derive(Clone, Copy, Debug)]
pub struct TranslatedReg<'a> {
    node: Node<'a>,
    regs: Reg<'a>,
    done: bool,
}

impl Iterator for TranslatedReg<'_> {
    type Item = (u64, u64);

    fn next(&mut self) -> Option<(u64, u64)> {
        if self.done {
            return None;
        }
        let (address, size) = self.regs.next()?;
        match translate_address(&self.node, address) {
            Some(cpu_address) => Some((cpu_address, size)),
            None => {
                self.done = true;
                None
            }
        }
    }
}

/// Translate one address through the `ranges` of `bus` into the parent's address space.
///
/// An absent or empty `ranges` property is an identity mapping. `None` means the address is
/// outside every declared window, the cell counts are unsupported, or the arithmetic overflowed.
pub fn translate_ranges(bus: &Node<'_>, address: u64) -> Option<u64> {
    let Some(value) = bus.property("ranges") else {
        return Some(address);
    };
    if value.is_empty() {
        return Some(address);
    }
    let child_cells = bus.address_cells();
    let parent_cells = bus.parent().map_or(2, |parent| parent.address_cells());
    let size_cells = bus.size_cells();
    if child_cells > 2 || parent_cells > 2 || size_cells > 2 {
        return None;
    }
    let mut cells = bus.cells("ranges", 4);
    loop {
        let child = read_cells(&mut cells, child_cells)?;
        let parent = read_cells(&mut cells, parent_cells)?;
        let size = read_cells(&mut cells, size_cells)?;
        if size == 0 {
            continue;
        }
        let end = child.checked_add(size)?;
        if address >= child && address < end {
            return parent.checked_add(address.checked_sub(child)?);
        }
    }
}

/// Translate a node's `address` from its parent's bus space to the CPU physical address space.
///
/// Walks the parent chain from the immediate parent upwards, applying each `ranges`. The root has
/// no parent and needs no translation.
pub fn translate_address(node: &Node<'_>, address: u64) -> Option<u64> {
    let mut bus = node.parent()?;
    let mut current = address;
    loop {
        current = translate_ranges(&bus, current)?;
        let Some(parent) = bus.parent() else {
            return Some(current);
        };
        bus = parent;
    }
}

/// Fill `out` with `bus`'s `dma-ranges` windows.
///
/// `None` when the property is absent; `Some(0)` when it is empty or malformed.
fn dma_windows_of(bus: &Node<'_>, out: &mut [DmaWindow]) -> Option<usize> {
    let value = bus.property("dma-ranges")?;
    if value.is_empty() {
        return Some(0);
    }
    let child_cells = bus.address_cells();
    let parent_cells = bus.parent().map_or(2, |parent| parent.address_cells());
    let size_cells = bus.size_cells();
    if child_cells > 2 || parent_cells > 2 || size_cells > 2 {
        return Some(0);
    }
    let mut cells = bus.cells("dma-ranges", 4);
    let mut count = 0usize;
    while let Some(bus_base) = read_cells(&mut cells, child_cells) {
        let Some(cpu_base) = read_cells(&mut cells, parent_cells) else {
            break;
        };
        let Some(size) = read_cells(&mut cells, size_cells) else {
            break;
        };
        if size == 0 {
            continue;
        }
        let Some(slot) = out.get_mut(count) else {
            break;
        };
        *slot = DmaWindow {
            bus_base,
            cpu_base,
            size,
        };
        count = count.saturating_add(1);
    }
    Some(count)
}

fn read_cells(cells: &mut Cells<'_>, count: u32) -> Option<u64> {
    let mut value = 0u64;
    let mut index = 0u32;
    while index < count {
        value = value.checked_shl(32)?.checked_add(cells.next()?)?;
        index = index.saturating_add(1);
    }
    Some(value)
}

/// Iterator over the direct children of a node.
#[derive(Clone, Copy, Debug)]
pub struct Children<'a> {
    fdt: Fdt<'a>,
    position: Option<usize>,
    skip: Option<usize>,
    done: bool,
}

impl<'a> Iterator for Children<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Node<'a>> {
        if self.done {
            return None;
        }
        let mut position = self.position?;
        if let Some(skip) = self.skip.take() {
            position = self.fdt.skip_subtree(skip)?;
        }
        loop {
            let token = self.fdt.struct_u32(position)?;
            match token {
                FDT_PROP => position = self.fdt.after_property(position)?,
                FDT_NOP => position = position.checked_add(4)?,
                FDT_BEGIN_NODE => {
                    let name_start = position.checked_add(4)?;
                    let (_, after_name) = self.fdt.struct_name(name_start)?;
                    self.position = Some(position);
                    self.skip = Some(after_name);
                    return Some(Node {
                        fdt: self.fdt,
                        name_start,
                    });
                }
                FDT_END_NODE | FDT_END => {
                    self.done = true;
                    return None;
                }
                _ => {
                    self.done = true;
                    return None;
                }
            }
        }
    }
}

/// Iterator over the memory reservation block.
#[derive(Clone, Copy, Debug)]
pub struct Reservations<'a> {
    fdt: Fdt<'a>,
    position: usize,
}

impl Iterator for Reservations<'_> {
    type Item = (u64, u64);

    fn next(&mut self) -> Option<(u64, u64)> {
        let address = read_be_u64(self.fdt.data, self.position)?;
        let size = read_be_u64(self.fdt.data, self.position.checked_add(8)?)?;
        self.position = self.position.checked_add(16)?;
        if address == 0 && size == 0 {
            return None;
        }
        Some((address, size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Big-endian cells for the test builder.
    fn be32(values: &[u32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    }

    /// Tiny FDT writer used only by the tests.
    struct Builder {
        structure: Vec<u8>,
        strings: Vec<u8>,
        reservations: Vec<(u64, u64)>,
    }

    impl Builder {
        fn new() -> Self {
            Self {
                structure: Vec::new(),
                strings: Vec::new(),
                reservations: Vec::new(),
            }
        }

        fn reservation(&mut self, address: u64, size: u64) {
            self.reservations.push((address, size));
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
            let reservation_bytes = (self.reservations.len() + 1) * 16;
            let struct_offset = 40 + reservation_bytes;
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
            for (address, size) in &self.reservations {
                blob.extend_from_slice(&address.to_be_bytes());
                blob.extend_from_slice(&size.to_be_bytes());
            }
            blob.extend_from_slice(&[0u8; 16]); // terminator
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
        // The first structure token follows the reservation block: 40-byte header, one 16-byte
        // terminator, so the root BEGIN_NODE is at offset 56.
        blob[56..60].copy_from_slice(&0xffff_ffffu32.to_be_bytes());
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
        // Root BEGIN_NODE is at 56, its name is one NUL padded to 4, so the PROP token is at 64
        // and the name-offset field is at 72.
        let name_offset_at = 72;
        blob[name_offset_at..name_offset_at + 4].copy_from_slice(&0x7fff_ffffu32.to_be_bytes());
        let fdt = Fdt::parse(&blob).unwrap();
        assert!(fdt.root().unwrap().property("compatible").is_none());
    }

    #[test]
    fn children_are_direct_and_in_tree_order() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let names: Vec<&str> = fdt
            .root()
            .unwrap()
            .children()
            .map(|node| node.name())
            .collect();
        assert_eq!(names, vec!["intc@8000000", "timer", "serial@9000000"]);
        let nested: Vec<&str> = fdt
            .find_node("/intc")
            .unwrap()
            .children()
            .map(|node| node.name())
            .collect();
        assert_eq!(nested, vec!["nested"]);
    }

    #[test]
    fn children_of_a_leaf_are_empty() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(fdt.find_node("/timer").unwrap().children().count(), 0);
    }

    #[test]
    fn reservations_are_listed() {
        let mut builder = Builder::new();
        builder.reservation(0x1000, 0x2000);
        builder.reservation(0x4000_0000, 0x1000);
        builder.begin("");
        builder.end();
        builder.token(FDT_END);
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        let entries: Vec<(u64, u64)> = fdt.reservations().collect();
        assert_eq!(entries, vec![(0x1000, 0x2000), (0x4000_0000, 0x1000)]);
    }

    #[test]
    fn empty_reservation_block_yields_nothing() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(fdt.reservations().count(), 0);
    }

    #[test]
    fn truncated_reservation_block_stops() {
        let mut blob = sample();
        // Cut the blob inside the reservation terminator; parse truncates `totalsize` to the
        // original value, so the blob must still parse but the iterator must stop.
        let len = blob.len();
        blob.truncate(len.saturating_sub(8));
        assert!(Fdt::parse(&blob).is_err());
    }

    #[allow(clippy::too_many_arguments)] // Test fixture; the arguments mirror the tree shape.
    fn node_tree(
        root_address_cells: u32,
        root_size_cells: u32,
        bus: &str,
        bus_address_cells: u32,
        bus_size_cells: u32,
        ranges: &[u32],
        device: &str,
        reg: &[u32],
    ) -> Vec<u8> {
        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", root_address_cells);
        builder.u32_prop("#size-cells", root_size_cells);
        builder.begin(bus);
        builder.u32_prop("#address-cells", bus_address_cells);
        builder.u32_prop("#size-cells", bus_size_cells);
        if !ranges.is_empty() {
            builder.prop("ranges", &be32(ranges));
        }
        builder.begin(device);
        builder.prop("reg", &be32(reg));
        builder.end();
        builder.end();
        builder.end();
        builder.finish()
    }

    #[test]
    fn parent_chain_is_recovered() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.begin("a");
        builder.begin("b");
        builder.begin("c@1");
        builder.end();
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        let c = fdt.find_node("/a/b/c@1").unwrap();
        let b = c.parent().unwrap();
        let a = b.parent().unwrap();
        let root = a.parent().unwrap();
        assert_eq!(b.name(), "b");
        assert_eq!(a.name(), "a");
        assert_eq!(root.name(), "");
        assert!(root.parent().is_none());
    }

    #[test]
    fn cell_count_defaults_are_two_and_one() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        let timer = fdt.find_node("/timer").unwrap();
        assert_eq!(timer.address_cells(), 2);
        assert_eq!(timer.size_cells(), 1);
    }

    #[test]
    fn absent_ranges_are_identity() {
        let blob = node_tree(2, 1, "bus", 1, 1, &[], "dev@1000", &[0x1000, 0x100]);
        let fdt = Fdt::parse(&blob).unwrap();
        let device = fdt.find_node("/bus/dev@1000").unwrap();
        assert_eq!(
            device.reg_translated().collect::<Vec<_>>(),
            vec![(0x1000, 0x100)]
        );
    }

    #[test]
    fn empty_ranges_are_identity() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.begin("bus");
        builder.u32_prop("#address-cells", 1);
        builder.u32_prop("#size-cells", 1);
        builder.prop("ranges", &[]);
        builder.begin("dev@1000");
        builder.prop("reg", &be32(&[0x1000, 0x100]));
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        let device = fdt.find_node("/bus/dev@1000").unwrap();
        assert_eq!(
            device.reg_translated().collect::<Vec<_>>(),
            vec![(0x1000, 0x100)]
        );
    }

    #[test]
    fn one_cell_soc_range_translates_uart() {
        let blob = node_tree(
            2,
            1,
            "soc",
            1,
            1,
            &[0x7e00_0000, 0x0, 0xfe00_0000, 0x0180_0000],
            "serial@7e201000",
            &[0x7e20_1000, 0x200],
        );
        let fdt = Fdt::parse(&blob).unwrap();
        let device = fdt.find_node("/soc/serial@7e201000").unwrap();
        assert_eq!(
            device.reg_translated().collect::<Vec<_>>(),
            vec![(0xfe20_1000, 0x200)]
        );
    }

    #[test]
    fn bcm2711_soc_ranges_fixture() {
        // The three real /soc windows from bcm2711-rpi-4-b.dtb.
        let blob = node_tree(
            2,
            1,
            "soc",
            1,
            1,
            &[
                0x7e00_0000,
                0x0,
                0xfe00_0000,
                0x0180_0000,
                0x7c00_0000,
                0x0,
                0xfc00_0000,
                0x0200_0000,
                0x4000_0000,
                0x0,
                0xff80_0000,
                0x0080_0000,
            ],
            "serial@7e201000",
            &[0x7e20_1000, 0x200],
        );
        let fdt = Fdt::parse(&blob).unwrap();
        let device = fdt.find_node("/soc/serial@7e201000").unwrap();
        assert_eq!(
            device.reg_translated().collect::<Vec<_>>(),
            vec![(0xfe20_1000, 0x200)]
        );

        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.begin("soc");
        builder.u32_prop("#address-cells", 1);
        builder.u32_prop("#size-cells", 1);
        builder.prop(
            "ranges",
            &be32(&[
                0x7e00_0000,
                0x0,
                0xfe00_0000,
                0x0180_0000,
                0x7c00_0000,
                0x0,
                0xfc00_0000,
                0x0200_0000,
                0x4000_0000,
                0x0,
                0xff80_0000,
                0x0080_0000,
            ]),
        );
        builder.begin("rng@7e104000");
        builder.prop("reg", &be32(&[0x7e10_4000, 0x28]));
        builder.end();
        builder.begin("local@40041000");
        builder.prop("reg", &be32(&[0x4004_1000, 0x1000]));
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(
            fdt.find_node("/soc/rng@7e104000")
                .unwrap()
                .reg_translated()
                .collect::<Vec<_>>(),
            vec![(0xfe10_4000, 0x28)]
        );
        assert_eq!(
            fdt.find_node("/soc/local@40041000")
                .unwrap()
                .reg_translated()
                .collect::<Vec<_>>(),
            vec![(0xff84_1000, 0x1000)]
        );
    }

    #[test]
    fn two_cell_address_emmc2bus_fixture() {
        let blob = node_tree(
            2,
            1,
            "emmc2bus",
            2,
            1,
            &[0x0, 0x7e00_0000, 0x0, 0xfe00_0000, 0x0180_0000],
            "mmc@7e340000",
            &[0x0, 0x7e34_0000, 0x100],
        );
        let fdt = Fdt::parse(&blob).unwrap();
        let device = fdt.find_node("/emmc2bus/mmc@7e340000").unwrap();
        assert_eq!(
            device.reg_translated().collect::<Vec<_>>(),
            vec![(0xfe34_0000, 0x100)]
        );
    }

    #[test]
    fn bcm2711_scb_ranges_fixture() {
        let blob = node_tree(
            2,
            1,
            "scb",
            2,
            2,
            &[
                0x0,
                0x7c00_0000,
                0x0,
                0xfc00_0000,
                0x0,
                0x0380_0000,
                0x0,
                0x4000_0000,
                0x0,
                0xff80_0000,
                0x0,
                0x0080_0000,
                0x6,
                0x0,
                0x6,
                0x0,
                0x0,
                0x4000_0000,
            ],
            "ethernet@7d580000",
            &[0x0, 0x7d58_0000, 0x0, 0x0001_0000],
        );
        let fdt = Fdt::parse(&blob).unwrap();
        let device = fdt.find_node("/scb/ethernet@7d580000").unwrap();
        assert_eq!(
            device.reg_translated().collect::<Vec<_>>(),
            vec![(0xfd58_0000, 0x1_0000)]
        );
    }

    #[test]
    fn nested_ranges_compose() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.begin("bus1");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.prop(
            "ranges",
            &be32(&[0x0, 0x1000_0000, 0x0, 0x2000_0000, 0x0010_0000]),
        );
        builder.begin("bus2");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.prop("ranges", &be32(&[0x0, 0x0, 0x0, 0x1000_0000, 0x0000_1000]));
        builder.begin("dev@100");
        builder.prop("reg", &be32(&[0x0, 0x100, 0x10]));
        builder.end();
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        let device = fdt.find_node("/bus1/bus2/dev@100").unwrap();
        assert_eq!(
            device.reg_translated().collect::<Vec<_>>(),
            vec![(0x2000_0100, 0x10)]
        );
    }

    #[test]
    fn address_outside_every_window_stops() {
        let blob = node_tree(
            2,
            1,
            "soc",
            1,
            1,
            &[0x7e00_0000, 0x0, 0xfe00_0000, 0x0180_0000],
            "dev@dead0000",
            &[0xdead_0000, 0x100],
        );
        let fdt = Fdt::parse(&blob).unwrap();
        let device = fdt.find_node("/soc/dev@dead0000").unwrap();
        assert_eq!(device.reg_translated().next(), None);
    }

    #[test]
    fn zero_sized_range_is_skipped() {
        let blob = node_tree(
            2,
            1,
            "soc",
            1,
            1,
            &[
                0x7e00_0000,
                0x0,
                0xfe00_0000,
                0x0,
                0x4000_0000,
                0x0,
                0xff80_0000,
                0x0080_0000,
            ],
            "local@40041000",
            &[0x4004_1000, 0x1000],
        );
        let fdt = Fdt::parse(&blob).unwrap();
        let device = fdt.find_node("/soc/local@40041000").unwrap();
        assert_eq!(
            device.reg_translated().collect::<Vec<_>>(),
            vec![(0xff84_1000, 0x1000)]
        );
    }

    #[test]
    fn malformed_cell_counts_yield_nothing() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.begin("bus");
        builder.u32_prop("#address-cells", 3);
        builder.u32_prop("#size-cells", 1);
        builder.prop("ranges", &be32(&[0x0, 0x0, 0x1000, 0x0, 0x2000, 0x100]));
        builder.begin("dev@1000");
        builder.prop("reg", &be32(&[0x0, 0x0, 0x1000, 0x100]));
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        let bus = fdt.find_node("/bus").unwrap();
        assert_eq!(translate_ranges(&bus, 0x1000), None);
        assert_eq!(
            fdt.find_node("/bus/dev@1000")
                .unwrap()
                .reg_translated()
                .next(),
            None
        );
    }

    #[test]
    fn dma_window_soc_shape() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.begin("soc");
        builder.u32_prop("#address-cells", 1);
        builder.u32_prop("#size-cells", 1);
        builder.prop(
            "dma-ranges",
            &be32(&[0xc000_0000, 0x0, 0x0000_0000, 0x4000_0000]),
        );
        builder.begin("rng@7e104000");
        builder.prop("reg", &be32(&[0x7e10_4000, 0x28]));
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(
            fdt.dma_window_for("/soc/rng@7e104000"),
            Some(DmaWindow {
                bus_base: 0xc000_0000,
                cpu_base: 0,
                size: 0x4000_0000,
            })
        );
    }

    #[test]
    fn dma_window_emmc2bus_shape() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.begin("emmc2bus");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.prop(
            "dma-ranges",
            &be32(&[0x0, 0xc000_0000, 0x0, 0x0000_0000, 0x4000_0000]),
        );
        builder.begin("mmc@7e340000");
        builder.prop("reg", &be32(&[0x0, 0x7e34_0000, 0x100]));
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(
            fdt.dma_window_for("/emmc2bus/mmc@7e340000"),
            Some(DmaWindow {
                bus_base: 0xc000_0000,
                cpu_base: 0,
                size: 0x4000_0000,
            })
        );
    }

    #[test]
    fn dma_window_scb_shape_skips_zero_size() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.begin("scb");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 2);
        builder.prop(
            "dma-ranges",
            &be32(&[
                0x4,
                0x7c00_0000,
                0x0,
                0xfc00_0000,
                0x0,
                0x0380_0000,
                0x0,
                0x0,
                0x0,
                0x4,
                0x0,
                0x0,
            ]),
        );
        builder.begin("ethernet@7d580000");
        builder.prop("reg", &be32(&[0x0, 0x7d58_0000, 0x0, 0x0001_0000]));
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(
            fdt.dma_window_for("/scb/ethernet@7d580000"),
            Some(DmaWindow {
                bus_base: 0x4_7c00_0000,
                cpu_base: 0xfc00_0000,
                size: 0x0380_0000,
            })
        );
    }

    #[test]
    fn dma_windows_lists_multiple_entries() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.begin("soc");
        builder.u32_prop("#address-cells", 1);
        builder.u32_prop("#size-cells", 1);
        builder.prop(
            "dma-ranges",
            &be32(&[
                0xc000_0000,
                0x0,
                0x0000_0000,
                0x4000_0000,
                0x7c00_0000,
                0x0,
                0xfc00_0000,
                0x0380_0000,
            ]),
        );
        builder.begin("dev@7e000000");
        builder.prop("reg", &be32(&[0x7e00_0000, 0x100]));
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        let mut windows = [DmaWindow::EMPTY; 4];
        assert_eq!(
            fdt.dma_windows_for("/soc/dev@7e000000", &mut windows),
            Some(2)
        );
        assert_eq!(windows[0].bus_base, 0xc000_0000);
        assert_eq!(windows[1].bus_base, 0x7c00_0000);
        assert_eq!(
            fdt.dma_window_for("/soc/dev@7e000000").unwrap().bus_base,
            0xc000_0000
        );
    }

    #[test]
    fn dma_window_uses_nearest_ancestor() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.begin("soc");
        builder.u32_prop("#address-cells", 1);
        builder.u32_prop("#size-cells", 1);
        builder.prop(
            "dma-ranges",
            &be32(&[0xc000_0000, 0x0, 0x0000_0000, 0x4000_0000]),
        );
        builder.begin("bus");
        builder.u32_prop("#address-cells", 1);
        builder.u32_prop("#size-cells", 1);
        builder.prop("dma-ranges", &be32(&[0x8000_0000, 0x0, 0x8000_0000]));
        builder.begin("dev@1000");
        builder.prop("reg", &be32(&[0x1000, 0x100]));
        builder.end();
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(
            fdt.dma_window_for("/soc/bus/dev@1000"),
            Some(DmaWindow {
                bus_base: 0x8000_0000,
                cpu_base: 0,
                size: 0x8000_0000,
            })
        );
    }

    #[test]
    fn dma_window_absent_or_empty_is_none() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(fdt.dma_window_for("/intc@8000000"), None);

        let mut builder = Builder::new();
        builder.begin("");
        builder.begin("bus");
        builder.prop("dma-ranges", &[]);
        builder.begin("dev@1");
        builder.prop("reg", &be32(&[0x1, 0x10]));
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(fdt.dma_window_for("/bus/dev@1"), None);
    }

    #[test]
    fn reg_translated_yields_every_entry() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.u32_prop("#address-cells", 2);
        builder.u32_prop("#size-cells", 1);
        builder.begin("soc");
        builder.u32_prop("#address-cells", 1);
        builder.u32_prop("#size-cells", 1);
        builder.prop(
            "ranges",
            &be32(&[0x7e00_0000, 0x0, 0xfe00_0000, 0x0180_0000]),
        );
        builder.begin("dev@7e000000");
        builder.prop("reg", &be32(&[0x7e00_0000, 0x1000, 0x7e00_1000, 0x2000]));
        builder.end();
        builder.end();
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        let device = fdt.find_node("/soc/dev@7e000000").unwrap();
        assert_eq!(
            device.reg_translated().collect::<Vec<_>>(),
            vec![(0xfe00_0000, 0x1000), (0xfe00_1000, 0x2000)]
        );
    }

    #[test]
    fn dma_window_missing_path_is_none() {
        let blob = sample();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(fdt.dma_window_for("/not/present"), None);
    }
}
