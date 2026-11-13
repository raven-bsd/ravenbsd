// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The ELF64 records the loader needs, parsed from byte slices.
//!
//! Every field is read with `u16/u32/u64::from_le_bytes` from a checked sub-slice; there is no
//! pointer cast and no `transmute`. A truncated record is [`ElfError::Truncated`], never a panic.
//! The layouts are the System V gABI ones; `elf_ut.rs` asserts the sizes and every field offset
//! with hand-built byte arrays.

use crate::consts::{
    EHDR_SIZE, ELF_MAGIC, ELFCLASS64, ELFDATA2LSB, EM_AARCH64, EM_RISCV, EM_X86_64, ET_DYN,
    ET_EXEC, EV_CURRENT, NOTE_HEADER_SIZE, PHDR_SIZE, RELA_SIZE, SYM_SIZE,
};
use crate::errors::ElfError;
use crate::limits::MAX_PHDRS;

/// The three architectures ravenlibc supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Arch {
    /// AMD x86-64 (`EM_X86_64`).
    X86_64,
    /// ARM AArch64 (`EM_AARCH64`).
    Aarch64,
    /// RISC-V 64-bit (`EM_RISCV`).
    Riscv64,
}

impl Arch {
    /// Map an `e_machine` value.
    pub fn from_machine(machine: u16) -> Option<Self> {
        match machine {
            EM_X86_64 => Some(Self::X86_64),
            EM_AARCH64 => Some(Self::Aarch64),
            EM_RISCV => Some(Self::Riscv64),
            _ => None,
        }
    }

    /// The `uname -m` style name, used only in error text.
    pub fn name(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Riscv64 => "riscv64",
        }
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, ElfError> {
    let end = offset.checked_add(2).ok_or(ElfError::Truncated)?;
    let slice = bytes.get(offset..end).ok_or(ElfError::Truncated)?;
    let array: [u8; 2] = slice.try_into().map_err(|_| ElfError::Truncated)?;
    Ok(u16::from_le_bytes(array))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, ElfError> {
    let end = offset.checked_add(4).ok_or(ElfError::Truncated)?;
    let slice = bytes.get(offset..end).ok_or(ElfError::Truncated)?;
    let array: [u8; 4] = slice.try_into().map_err(|_| ElfError::Truncated)?;
    Ok(u32::from_le_bytes(array))
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, ElfError> {
    let end = offset.checked_add(8).ok_or(ElfError::Truncated)?;
    let slice = bytes.get(offset..end).ok_or(ElfError::Truncated)?;
    let array: [u8; 8] = slice.try_into().map_err(|_| ElfError::Truncated)?;
    Ok(u64::from_le_bytes(array))
}

fn i64_at(bytes: &[u8], offset: usize) -> Result<i64, ElfError> {
    let end = offset.checked_add(8).ok_or(ElfError::Truncated)?;
    let slice = bytes.get(offset..end).ok_or(ElfError::Truncated)?;
    let array: [u8; 8] = slice.try_into().map_err(|_| ElfError::Truncated)?;
    Ok(i64::from_le_bytes(array))
}

/// The ELF64 file header (only the fields the loader uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ehdr {
    /// `e_type`: `ET_EXEC` or `ET_DYN`.
    pub e_type: u16,
    /// `e_machine`.
    pub e_machine: u16,
    /// `e_entry`.
    pub e_entry: u64,
    /// `e_phoff`: file offset of the program header table.
    pub e_phoff: u64,
    /// `e_phentsize`: always 56.
    pub e_phentsize: u16,
    /// `e_phnum`.
    pub e_phnum: u16,
}

impl Ehdr {
    /// Parse and validate the file header.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        let ident = bytes.get(..16).ok_or(ElfError::Truncated)?;
        if ident.get(..4) != Some(&ELF_MAGIC[..]) {
            return Err(ElfError::BadMagic);
        }
        if ident.get(4).copied() != Some(ELFCLASS64) {
            return Err(ElfError::BadClass);
        }
        if ident.get(5).copied() != Some(ELFDATA2LSB) {
            return Err(ElfError::BadEndian);
        }
        if ident.get(6).copied() != Some(EV_CURRENT) {
            return Err(ElfError::BadVersion);
        }
        if bytes.len() < EHDR_SIZE {
            return Err(ElfError::Truncated);
        }
        let e_type = u16_at(bytes, 16)?;
        if e_type != ET_EXEC && e_type != ET_DYN {
            return Err(ElfError::UnsupportedType);
        }
        let e_machine = u16_at(bytes, 18)?;
        let e_version = u32_at(bytes, 20)?;
        if u8::try_from(e_version).ok() != Some(EV_CURRENT) {
            return Err(ElfError::BadVersion);
        }
        let e_entry = u64_at(bytes, 24)?;
        let e_phoff = u64_at(bytes, 32)?;
        let e_phentsize = u16_at(bytes, 54)?;
        if usize::from(e_phentsize) != PHDR_SIZE {
            return Err(ElfError::BadPhdrSize);
        }
        let e_phnum = u16_at(bytes, 56)?;
        if usize::from(e_phnum) > MAX_PHDRS {
            return Err(ElfError::TooManyPhdrs);
        }
        Ok(Self {
            e_type,
            e_machine,
            e_entry,
            e_phoff,
            e_phentsize,
            e_phnum,
        })
    }
}

/// One program header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phdr {
    /// `p_type`.
    pub p_type: u32,
    /// `p_flags`.
    pub p_flags: u32,
    /// `p_offset`.
    pub p_offset: u64,
    /// `p_vaddr`.
    pub p_vaddr: u64,
    /// `p_paddr`.
    pub p_paddr: u64,
    /// `p_filesz`.
    pub p_filesz: u64,
    /// `p_memsz`.
    pub p_memsz: u64,
    /// `p_align`.
    pub p_align: u64,
}

impl Phdr {
    /// Parse one 56-byte record.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        if bytes.len() < PHDR_SIZE {
            return Err(ElfError::Truncated);
        }
        Ok(Self {
            p_type: u32_at(bytes, 0)?,
            p_flags: u32_at(bytes, 4)?,
            p_offset: u64_at(bytes, 8)?,
            p_vaddr: u64_at(bytes, 16)?,
            p_paddr: u64_at(bytes, 24)?,
            p_filesz: u64_at(bytes, 32)?,
            p_memsz: u64_at(bytes, 40)?,
            p_align: u64_at(bytes, 48)?,
        })
    }
}

/// Read `phnum` program headers at `phdr_vaddr` through a view.
pub fn read_phdrs(
    mem: &dyn crate::mem::MemView,
    phdr_vaddr: u64,
    phnum: u16,
) -> Result<alloc::vec::Vec<Phdr>, ElfError> {
    if usize::from(phnum) > MAX_PHDRS {
        return Err(ElfError::TooManyPhdrs);
    }
    let mut headers = alloc::vec::Vec::new();
    for index in 0..usize::from(phnum) {
        let offset = index.checked_mul(PHDR_SIZE).ok_or(ElfError::BadEntry)?;
        let address = phdr_vaddr
            .checked_add(u64::try_from(offset).map_err(|_| ElfError::BadEntry)?)
            .ok_or(ElfError::BadEntry)?;
        let bytes = mem.read(address, PHDR_SIZE).ok_or(ElfError::BadEntry)?;
        headers.push(Phdr::parse(bytes)?);
    }
    Ok(headers)
}

/// One `Elf64_Dyn` entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dyn {
    /// `d_tag`.
    pub tag: i64,
    /// `d_un.d_val` / `d_un.d_ptr`.
    pub val: u64,
}

impl Dyn {
    /// Parse one 16-byte record.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        if bytes.len() < 16 {
            return Err(ElfError::Truncated);
        }
        Ok(Self {
            tag: i64_at(bytes, 0)?,
            val: u64_at(bytes, 8)?,
        })
    }
}

/// One `Elf64_Sym`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sym {
    /// `st_name`: string-table offset.
    pub name: u32,
    /// `st_info`.
    pub info: u8,
    /// `st_other`.
    pub other: u8,
    /// `st_shndx`.
    pub shndx: u16,
    /// `st_value`.
    pub value: u64,
    /// `st_size`.
    pub size: u64,
}

impl Sym {
    /// Parse one 24-byte record.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        if bytes.len() < SYM_SIZE {
            return Err(ElfError::Truncated);
        }
        Ok(Self {
            name: u32_at(bytes, 0)?,
            info: *bytes.get(4).ok_or(ElfError::Truncated)?,
            other: *bytes.get(5).ok_or(ElfError::Truncated)?,
            shndx: u16_at(bytes, 6)?,
            value: u64_at(bytes, 8)?,
            size: u64_at(bytes, 16)?,
        })
    }

    /// `ELF_ST_BIND(st_info)`.
    pub fn bind(&self) -> u8 {
        self.info >> 4
    }

    /// `ELF_ST_TYPE(st_info)`.
    pub fn typ(&self) -> u8 {
        self.info & 0x0f
    }

    /// `ELF_ST_VISIBILITY(st_other)`.
    pub fn visibility(&self) -> u8 {
        self.other & 0x03
    }
}

/// One `Elf64_Rela`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rela {
    /// `r_offset`.
    pub offset: u64,
    /// `r_info`.
    pub info: u64,
    /// `r_addend`.
    pub addend: i64,
}

impl Rela {
    /// Parse one 24-byte record.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        if bytes.len() < RELA_SIZE {
            return Err(ElfError::Truncated);
        }
        Ok(Self {
            offset: u64_at(bytes, 0)?,
            info: u64_at(bytes, 8)?,
            addend: i64_at(bytes, 16)?,
        })
    }

    /// `ELF64_R_SYM(r_info)`.
    pub fn sym(&self) -> u32 {
        u32::try_from(self.info >> 32).unwrap_or(u32::MAX)
    }

    /// `ELF64_R_TYPE(r_info)`.
    pub fn typ(&self) -> u32 {
        u32::try_from(self.info & 0xffff_ffff).unwrap_or(u32::MAX)
    }
}

/// One `Elf64_Verdef`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdef {
    /// `vd_version`.
    pub version: u16,
    /// `vd_flags`.
    pub flags: u16,
    /// `vd_ndx`.
    pub ndx: u16,
    /// `vd_cnt`.
    pub cnt: u16,
    /// `vd_hash`.
    pub hash: u32,
    /// `vd_aux`: byte offset of the first `Verdaux`.
    pub aux: u32,
    /// `vd_next`: byte offset of the next `Verdef`, 0 at the end.
    pub next: u32,
}

impl Verdef {
    /// Parse one 20-byte record.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        if bytes.len() < 20 {
            return Err(ElfError::Truncated);
        }
        Ok(Self {
            version: u16_at(bytes, 0)?,
            flags: u16_at(bytes, 2)?,
            ndx: u16_at(bytes, 4)?,
            cnt: u16_at(bytes, 6)?,
            hash: u32_at(bytes, 8)?,
            aux: u32_at(bytes, 12)?,
            next: u32_at(bytes, 16)?,
        })
    }
}

/// One `Elf64_Verdaux`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdaux {
    /// `vda_name`: string-table offset.
    pub name: u32,
    /// `vda_next`: byte offset of the next `Verdaux`, 0 at the end.
    pub next: u32,
}

impl Verdaux {
    /// Parse one 8-byte record.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        if bytes.len() < 8 {
            return Err(ElfError::Truncated);
        }
        Ok(Self {
            name: u32_at(bytes, 0)?,
            next: u32_at(bytes, 4)?,
        })
    }
}

/// One `Elf64_Verneed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verneed {
    /// `vn_version`.
    pub version: u16,
    /// `vn_cnt`.
    pub cnt: u16,
    /// `vn_file`: string-table offset of the dependency name.
    pub file: u32,
    /// `vn_aux`: byte offset of the first `Vernaux`.
    pub aux: u32,
    /// `vn_next`: byte offset of the next `Verneed`, 0 at the end.
    pub next: u32,
}

impl Verneed {
    /// Parse one 16-byte record.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        if bytes.len() < 16 {
            return Err(ElfError::Truncated);
        }
        Ok(Self {
            version: u16_at(bytes, 0)?,
            cnt: u16_at(bytes, 2)?,
            file: u32_at(bytes, 4)?,
            aux: u32_at(bytes, 8)?,
            next: u32_at(bytes, 12)?,
        })
    }
}

/// One `Elf64_Vernaux`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vernaux {
    /// `vna_hash`.
    pub hash: u32,
    /// `vna_flags`.
    pub flags: u16,
    /// `vna_other`: the version index.
    pub other: u16,
    /// `vna_name`: string-table offset.
    pub name: u32,
    /// `vna_next`: byte offset of the next `Vernaux`, 0 at the end.
    pub next: u32,
}

impl Vernaux {
    /// Parse one 16-byte record.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        if bytes.len() < 16 {
            return Err(ElfError::Truncated);
        }
        Ok(Self {
            hash: u32_at(bytes, 0)?,
            flags: u16_at(bytes, 4)?,
            other: u16_at(bytes, 6)?,
            name: u32_at(bytes, 8)?,
            next: u32_at(bytes, 12)?,
        })
    }
}

/// One ELF note (`Elf_Nhdr` plus the 4-aligned name and descriptor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note<'a> {
    /// `n_type`.
    pub n_type: u32,
    /// The name, without its padding.
    pub name: &'a [u8],
    /// The descriptor, without its padding.
    pub desc: &'a [u8],
    /// The number of bytes this note occupies, padding included.
    pub len: usize,
}

impl<'a> Note<'a> {
    /// Parse one note at the start of `bytes`.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ElfError> {
        if bytes.len() < NOTE_HEADER_SIZE {
            return Err(ElfError::Truncated);
        }
        let namesz = usize::try_from(u32_at(bytes, 0)?).map_err(|_| ElfError::Truncated)?;
        let descsz = usize::try_from(u32_at(bytes, 4)?).map_err(|_| ElfError::Truncated)?;
        let n_type = u32_at(bytes, 8)?;
        let name_start = NOTE_HEADER_SIZE;
        let name_end = name_start.checked_add(namesz).ok_or(ElfError::Truncated)?;
        let name = bytes.get(name_start..name_end).ok_or(ElfError::Truncated)?;
        let desc_start = align4(name_end).ok_or(ElfError::Truncated)?;
        let desc_end = desc_start.checked_add(descsz).ok_or(ElfError::Truncated)?;
        let desc = bytes.get(desc_start..desc_end).ok_or(ElfError::Truncated)?;
        let len = align4(desc_end).ok_or(ElfError::Truncated)?;
        Ok(Self {
            n_type,
            name,
            desc,
            len,
        })
    }
}

fn align4(value: usize) -> Option<usize> {
    let rounded = value.checked_add(3)?;
    Some(rounded & !3)
}

#[cfg(test)]
#[path = "elf_ut.rs"]
mod tests;
