// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The independent ABI model: argument assignment and frame construction per architecture.
//!
//! Written from the psABI texts (System V AMD64 3.2.3, AAPCS64 B.5, riscv64 psABI 2.2) and
//! `donor-map/04` 3.9.1-3.9.3, NOT from the readers: the model contains no reader code and
//! must fail when a reader changes. The test sweep feeds every class through it and compares
//! what the reader returns with what the model put there.
//!
//! [`assign`] places each argument into the register file the caller would use;
//! [`build_list`] copies that file into an [`Arena`] in exactly the memory image the stub of
//! C59 section 4.8 builds (save area, `va_list`, stack argument area) and returns the offset
//! of the list.

#![allow(dead_code)] // included by several test binaries, each using a subset

use crate::arena::{Arena, PADDING};

/// Padding byte of aggregates and `long double` slots.
const PAD: u8 = PADDING;

/// One variadic argument of the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    /// `int`, `unsigned`, `wint_t`: an 8-byte integer slot with undefined upper half.
    Int(u64),
    /// `long`, `long long`, `size_t`: a full 8-byte integer slot.
    Long(u64),
    /// A pointer value.
    Ptr(usize),
    /// A `double` as its 64-bit memory image.
    Double(u64),
    /// A `long double` as its 128-bit little-endian memory image.
    LongDouble(u128),
    /// An integer-class aggregate of `bytes` bytes and `align` bytes of alignment.
    Agg {
        /// The object image; the length is the size.
        bytes: Vec<u8>,
        /// Alignment in bytes (the documented subset uses 8).
        align: u8,
    },
}

/// The architecture and profile the model builds for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    /// x86_64 System V, hosted hard-float.
    X86_64,
    /// x86_64 System V, soft-float profile (`raven_va_softfloat`): doubles on the stack.
    X86_64Softfloat,
    /// aarch64 AAPCS64, hard-float.
    Aapcs64,
    /// aarch64 AAPCS64, soft-float (`target_abi = "softfloat"`): doubles on the stack.
    Aapcs64Softfloat,
    /// riscv64 psABI.
    Riscv64,
}

impl Arch {
    /// The number of integer argument registers.
    #[must_use]
    pub fn integer_registers(self) -> usize {
        match self {
            Self::X86_64 | Self::X86_64Softfloat => 6,
            Self::Aapcs64 | Self::Aapcs64Softfloat | Self::Riscv64 => 8,
        }
    }

    /// True when the profile passes every floating argument on the stack.
    #[must_use]
    pub fn softfloat(self) -> bool {
        matches!(self, Self::X86_64Softfloat | Self::Aapcs64Softfloat)
    }
}

/// A slot of the frame that holds a pointer to an aggregate passed by reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefSlot {
    /// The GP register of that index.
    Gp(usize),
    /// The byte offset of the slot inside the stack area.
    Stack(usize),
}

/// The caller's register file and stack argument area for one argument list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Integer registers (`rdi..r9` / `x0..x7` / `a0..a7`).
    pub gp: [u64; 8],
    /// Vector registers, 16 bytes each (`xmm0..7` / `q0..q7`).
    pub fp: [[u8; 16]; 8],
    /// The stack argument area, 8-byte slots, 16-byte aligned at its start.
    pub stack: Vec<u8>,
    /// Aggregates passed by reference: where the pointer lives and what it points at.
    pub refs: Vec<(RefSlot, Vec<u8>)>,
}

impl Frame {
    fn new() -> Self {
        Self {
            gp: [0; 8],
            fp: [[0; 16]; 8],
            stack: Vec::new(),
            refs: Vec::new(),
        }
    }

    fn stack_len(&self) -> usize {
        self.stack.len()
    }

    fn push_u64(&mut self, value: u64) {
        self.stack.extend_from_slice(&value.to_le_bytes());
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        self.stack.extend_from_slice(bytes);
    }

    /// Push an aggregate image padded to a whole number of 8-byte slots.
    fn push_slots(&mut self, bytes: &[u8]) {
        self.push_bytes(bytes);
        while !self.stack.len().is_multiple_of(8) {
            self.stack.push(PAD);
        }
    }

    fn align_stack(&mut self, align: usize) {
        while !self.stack.len().is_multiple_of(align) {
            self.stack.push(PAD);
        }
    }

    /// Store `chunk` (at most 8 bytes) into the low bytes of GP register `index`.
    fn set_gp_bytes(&mut self, index: usize, chunk: &[u8]) {
        let mut slot = self.gp[index].to_le_bytes();
        slot[..chunk.len()].copy_from_slice(chunk);
        self.gp[index] = u64::from_le_bytes(slot);
    }
}

/// The assignment rule of every architecture, from the ABI text.
#[must_use]
pub fn assign(arch: Arch, named: usize, args: &[Arg]) -> Frame {
    match arch {
        Arch::X86_64 | Arch::X86_64Softfloat => assign_sysv(arch, named, args),
        Arch::Aapcs64 | Arch::Aapcs64Softfloat => assign_aapcs64(arch, named, args),
        Arch::Riscv64 => assign_riscv64(named, args),
    }
}

fn assign_sysv(arch: Arch, named: usize, args: &[Arg]) -> Frame {
    let mut frame = Frame::new();
    let mut gp = named;
    let mut fp = 0usize;
    for arg in args {
        match arg {
            Arg::Int(v) | Arg::Long(v) => {
                if gp < 6 {
                    frame.gp[gp] = *v;
                    gp += 1;
                } else {
                    frame.push_u64(*v);
                }
            }
            Arg::Ptr(v) => {
                if gp < 6 {
                    frame.gp[gp] = *v as u64;
                    gp += 1;
                } else {
                    frame.push_u64(*v as u64);
                }
            }
            Arg::Double(bits) => {
                if !arch.softfloat() && fp < 8 {
                    frame.fp[fp][..8].copy_from_slice(&bits.to_le_bytes());
                    for byte in &mut frame.fp[fp][8..] {
                        *byte = PAD;
                    }
                    fp += 1;
                } else {
                    frame.push_u64(*bits);
                }
            }
            Arg::LongDouble(bits) => {
                // Memory class, always the stack, always 16-byte aligned (T4).
                frame.align_stack(16);
                frame.push_bytes(&bits.to_le_bytes());
            }
            Arg::Agg { bytes, align } => {
                let size = bytes.len();
                if size <= 16 && *align == 8 {
                    let slots = size.div_ceil(8);
                    if gp + slots <= 6 {
                        for (index, chunk) in bytes.chunks(8).enumerate() {
                            frame.set_gp_bytes(gp + index, chunk);
                        }
                        gp += slots;
                    } else {
                        frame.push_slots(bytes);
                    }
                } else {
                    // MEMORY class on x86_64: passed by value on the stack, never by
                    // reference (C59 section 4.5).
                    frame.align_stack(8);
                    frame.push_slots(bytes);
                }
            }
        }
    }
    frame
}

fn assign_aapcs64(arch: Arch, named: usize, args: &[Arg]) -> Frame {
    let mut frame = Frame::new();
    // Negative byte offsets from the top of each register area, as the ABI defines them.
    let mut gr = -(64i32 - 8 * named as i32);
    let mut vr = -128i32;
    for arg in args {
        match arg {
            Arg::Int(v) | Arg::Long(v) => {
                if gr < 0 && gr + 8 <= 0 {
                    let index = ((gr + 64) / 8) as usize;
                    frame.gp[index] = *v;
                    gr += 8;
                } else {
                    gr = 0;
                    frame.push_u64(*v);
                }
            }
            Arg::Ptr(v) => {
                if gr < 0 && gr + 8 <= 0 {
                    let index = ((gr + 64) / 8) as usize;
                    frame.gp[index] = *v as u64;
                    gr += 8;
                } else {
                    gr = 0;
                    frame.push_u64(*v as u64);
                }
            }
            Arg::Double(bits) => {
                if !arch.softfloat() && vr < 0 && vr + 16 <= 0 {
                    let index = ((vr + 128) / 16) as usize;
                    frame.fp[index][..8].copy_from_slice(&bits.to_le_bytes());
                    for byte in &mut frame.fp[index][8..] {
                        *byte = PAD;
                    }
                    vr += 16;
                } else {
                    frame.push_u64(*bits);
                }
            }
            Arg::LongDouble(bits) => {
                if !arch.softfloat() && vr < 0 && vr + 16 <= 0 {
                    let index = ((vr + 128) / 16) as usize;
                    frame.fp[index].copy_from_slice(&bits.to_le_bytes());
                    vr += 16;
                } else {
                    frame.align_stack(16);
                    frame.push_bytes(&bits.to_le_bytes());
                }
            }
            Arg::Agg { bytes, align } => {
                let size = bytes.len();
                if size <= 16 && *align == 8 {
                    let slots = size.div_ceil(8);
                    if gr < 0 && gr + (8 * slots as i32) <= 0 {
                        let index = ((gr + 64) / 8) as usize;
                        for (offset, chunk) in bytes.chunks(8).enumerate() {
                            frame.set_gp_bytes(index + offset, chunk);
                        }
                        gr += 8 * slots as i32;
                    } else {
                        gr = 0;
                        frame.push_slots(bytes);
                    }
                } else {
                    frame.align_stack(8);
                    let slot = if gr < 0 && gr + 8 <= 0 {
                        let index = ((gr + 64) / 8) as usize;
                        gr += 8;
                        RefSlot::Gp(index)
                    } else {
                        gr = 0;
                        let offset = frame.stack_len();
                        frame.push_u64(0);
                        RefSlot::Stack(offset)
                    };
                    frame.refs.push((slot, bytes.clone()));
                }
            }
        }
    }
    frame
}

fn assign_riscv64(named: usize, args: &[Arg]) -> Frame {
    let mut frame = Frame::new();
    // The cursor is a byte position in the contiguous [a0..a7][stack] area.
    let mut cursor = 8 * named;
    for arg in args {
        match arg {
            Arg::Int(v) | Arg::Long(v) => {
                riscv_put_u64(&mut frame, cursor, *v);
                cursor += 8;
            }
            Arg::Ptr(v) => {
                riscv_put_u64(&mut frame, cursor, *v as u64);
                cursor += 8;
            }
            Arg::Double(bits) => {
                // The integer convention: the same placement as an integer.
                riscv_put_u64(&mut frame, cursor, *bits);
                cursor += 8;
            }
            Arg::LongDouble(bits) => {
                cursor = align_up(cursor, 16);
                riscv_put_bytes(&mut frame, cursor, &bits.to_le_bytes());
                cursor += 16;
            }
            Arg::Agg { bytes, align } => {
                let size = bytes.len();
                if size <= 16 && *align == 8 {
                    riscv_put_bytes(&mut frame, cursor, bytes);
                    cursor += 8 * size.div_ceil(8);
                } else {
                    let slot = if cursor + 8 <= 64 {
                        RefSlot::Gp(cursor / 8)
                    } else {
                        RefSlot::Stack(cursor - 64)
                    };
                    riscv_put_u64(&mut frame, cursor, 0);
                    cursor += 8;
                    frame.refs.push((slot, bytes.clone()));
                }
            }
        }
    }
    frame
}

/// Write 8 bytes at the riscv64 cursor position.
fn riscv_put_u64(frame: &mut Frame, cursor: usize, value: u64) {
    riscv_put_bytes(frame, cursor, &value.to_le_bytes());
}

/// Write `data` at the riscv64 cursor position.
///
/// The register file and the stack area are one contiguous window, so an object may start in
/// `a7` and continue in the first stack slot; the write is split per 8-byte slot.
fn riscv_put_bytes(frame: &mut Frame, cursor: usize, data: &[u8]) {
    let mut offset = 0usize;
    while offset < data.len() {
        let position = cursor + offset;
        let slot_start = position - (position % 8);
        let in_slot = position - slot_start;
        let take = (8 - in_slot).min(data.len() - offset);
        if slot_start < 64 {
            let index = slot_start / 8;
            let mut slot = frame.gp[index].to_le_bytes();
            slot[in_slot..in_slot + take].copy_from_slice(&data[offset..offset + take]);
            frame.gp[index] = u64::from_le_bytes(slot);
        } else {
            let stack_position = slot_start - 64 + in_slot;
            let needed = stack_position + take;
            while frame.stack.len() < needed {
                frame.stack.push(PAD);
            }
            frame.stack[stack_position..needed].copy_from_slice(&data[offset..offset + take]);
        }
        offset += take;
    }
}

fn align_up(value: usize, align: usize) -> usize {
    value.wrapping_add(align - 1) & !(align - 1)
}

/// The memory image the stub of C59 section 4.8 builds, plus the offset of the `va_list`.
pub struct Built {
    /// The architecture and profile.
    pub arch: Arch,
    /// Number of fixed leading arguments.
    pub named: usize,
    /// The arena holding the save area, the list and the stack arguments.
    pub arena: Arena,
    /// Offset of the `va_list` (SysV/AAPCS64) or of the riscv64 cursor.
    pub list: usize,
    /// Offset of the first stack argument.
    pub stack: usize,
}

/// Build the frame image for `arch` and `named` fixed arguments.
#[must_use]
pub fn build_list(arch: Arch, frame: &Frame, named: usize) -> Built {
    match arch {
        Arch::X86_64 | Arch::X86_64Softfloat => build_sysv(arch, frame, named),
        Arch::Aapcs64 | Arch::Aapcs64Softfloat => build_aapcs64(arch, frame, named),
        Arch::Riscv64 => build_riscv64(frame, named),
    }
}

/// The 16-byte canary the arena reserves before the frame and the data area.
const PREFIX: usize = 16;

/// Bytes of by-reference aggregate payloads the arena must reserve.
fn payload_bytes(frame: &Frame) -> usize {
    let mut total = 0usize;
    for (_, bytes) in &frame.refs {
        total += align_up(bytes.len(), 16).max(16);
    }
    total
}

fn build_sysv(arch: Arch, frame: &Frame, named: usize) -> Built {
    let save = PREFIX;
    let list = save + 176;
    let stack = align_up(list + 24, 16);
    let data = stack + align_up(frame.stack.len(), 16);
    let mut arena = Arena::new(data + payload_bytes(frame) + 16);
    for (index, slot) in frame.gp.iter().enumerate() {
        arena.write_u64(save + 8 * index, *slot);
    }
    for (index, slot) in frame.fp.iter().enumerate() {
        arena.write(save + 48 + 16 * index, slot);
    }
    arena.write(stack, &frame.stack);
    arena.write_u32(list, (8 * named) as u32);
    arena.write_u32(list + 4, if arch.softfloat() { 176 } else { 48 });
    arena.write_ptr(list + 8, arena.ptr(stack));
    arena.write_ptr(list + 16, arena.ptr(save));
    patch_refs(&mut arena, frame, save, 0, stack);
    Built {
        arch,
        named,
        arena,
        list,
        stack,
    }
}

fn build_aapcs64(arch: Arch, frame: &Frame, named: usize) -> Built {
    let save = PREFIX;
    let list = save + 208;
    let stack = align_up(list + 32, 16);
    let data = stack + align_up(frame.stack.len(), 16);
    let mut arena = Arena::new(data + payload_bytes(frame) + 16);
    for (index, slot) in frame.gp.iter().enumerate() {
        arena.write_u64(save + 16 + 8 * index, *slot);
    }
    for (index, slot) in frame.fp.iter().enumerate() {
        arena.write(save + 80 + 16 * index, slot);
    }
    arena.write(stack, &frame.stack);
    arena.write_ptr(list, arena.ptr(stack));
    arena.write_ptr(list + 8, arena.ptr(save + 80));
    arena.write_ptr(list + 16, arena.ptr(save + 208));
    arena.write_i32(list + 24, -(64 - 8 * named as i32));
    arena.write_i32(list + 28, if arch.softfloat() { 0 } else { -128 });
    patch_refs(&mut arena, frame, save, 16, stack);
    Built {
        arch,
        named,
        arena,
        list,
        stack,
    }
}

fn build_riscv64(frame: &Frame, named: usize) -> Built {
    let region = PREFIX;
    let stack = region + 64;
    let data = stack + align_up(frame.stack.len(), 16);
    let mut arena = Arena::new(data + payload_bytes(frame) + 16);
    for (index, slot) in frame.gp.iter().enumerate() {
        arena.write_u64(region + 8 * index, *slot);
    }
    arena.write(stack, &frame.stack);
    patch_refs(&mut arena, frame, region, 0, stack);
    Built {
        arch: Arch::Riscv64,
        named,
        arena,
        list: region + 8 * named,
        stack,
    }
}

/// Write every by-reference payload and patch the pointer that carries it.
fn patch_refs(arena: &mut Arena, frame: &Frame, base: usize, gp_offset: usize, stack: usize) {
    let mut data = stack + align_up(frame.stack.len(), 16);
    for (slot, bytes) in &frame.refs {
        let target = arena.ptr(data);
        arena.write(data, bytes);
        match slot {
            RefSlot::Gp(index) => arena.write_ptr(base + gp_offset + 8 * index, target),
            RefSlot::Stack(offset) => arena.write_ptr(stack + offset, target),
        }
        data += align_up(bytes.len(), 16).max(16);
    }
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;
