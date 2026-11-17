// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded ELF32/ELF64 section and symbol inspection.
//!
//! This is the read-only consumer surface used by `nm` and `strip`. It accepts relocatable
//! (`ET_REL`) and executable/dynamic files, unlike the loader-facing [`crate::Elf`] parser, and
//! never trusts an offset or count: every section, string and symbol range is checked against
//! the input slice. The API borrows the original bytes and allocates nothing.

use super::ElfError;

/// Section type: inactive header.
pub const SHT_NULL: u32 = 0;
/// Section type: program data.
pub const SHT_PROGBITS: u32 = 1;
/// Section type: symbol table.
pub const SHT_SYMTAB: u32 = 2;
/// Section type: string table.
pub const SHT_STRTAB: u32 = 3;
/// Section type: uninitialized data.
pub const SHT_NOBITS: u32 = 8;
/// Section type: dynamic symbol table.
pub const SHT_DYNSYM: u32 = 11;
/// Section type: extended symbol-table section indices.
pub const SHT_SYMTAB_SHNDX: u32 = 18;

/// Symbol table index: undefined.
pub const SHN_UNDEF: u16 = 0;
/// Symbol table index: absolute.
pub const SHN_ABS: u16 = 0xfff1;
/// Symbol table index: common block.
pub const SHN_COMMON: u16 = 0xfff2;
/// Symbol table index: section index is in the extended table.
pub const SHN_XINDEX: u16 = 0xffff;

/// Symbol binding: local.
pub const STB_LOCAL: u8 = 0;
/// Symbol binding: global.
pub const STB_GLOBAL: u8 = 1;
/// Symbol binding: weak.
pub const STB_WEAK: u8 = 2;
/// Symbol binding: GNU unique.
pub const STB_GNU_UNIQUE: u8 = 10;

/// Symbol type: no type.
pub const STT_NOTYPE: u8 = 0;
/// Symbol type: data object.
pub const STT_OBJECT: u8 = 1;
/// Symbol type: function.
pub const STT_FUNC: u8 = 2;
/// Symbol type: section.
pub const STT_SECTION: u8 = 3;
/// Symbol type: file name.
pub const STT_FILE: u8 = 4;
/// Symbol type: common data.
pub const STT_COMMON: u8 = 5;
/// Symbol type: TLS object.
pub const STT_TLS: u8 = 6;
/// Symbol type: GNU indirect function.
pub const STT_GNU_IFUNC: u8 = 10;

const ET_REL: u16 = 1;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const ELFCLASS32: u8 = 1;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EHDR32_SIZE: usize = 52;
const EHDR64_SIZE: usize = 64;
const SHDR32_SIZE: usize = 40;
const SHDR64_SIZE: usize = 64;
const SYM32_SIZE: usize = 16;
const SYM64_SIZE: usize = 24;

/// The ELF class, which fixes every width in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// 32-bit ELF.
    Elf32,
    /// 64-bit ELF.
    Elf64,
}

/// A parsed ELF object that can expose sections and symbols.
#[derive(Clone, Copy, Debug)]
pub struct Object<'a> {
    data: &'a [u8],
    class: Class,
    e_type: u16,
    machine: u16,
    shoff: usize,
    shentsize: usize,
    shnum: usize,
    shstrndx: usize,
    shstr: &'a [u8],
}

/// One section header and its name table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section<'a> {
    data: &'a [u8],
    shstr: &'a [u8],
    index: usize,
    header_offset: usize,
    name_offset: u32,
    section_type: u32,
    flags: u64,
    address: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    align: u64,
    entsize: u64,
}

/// One symbol-table entry and its string table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Symbol<'a> {
    strtab: &'a [u8],
    name_offset: u32,
    value: u64,
    size: u64,
    info: u8,
    other: u8,
    shndx: u16,
}

#[derive(Clone, Copy)]
struct RawSection {
    header_offset: usize,
    name_offset: u32,
    section_type: u32,
    flags: u64,
    address: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    align: u64,
    entsize: u64,
}

/// Iterate section headers in index order.
#[derive(Clone, Debug)]
pub struct Sections<'a> {
    object: Object<'a>,
    index: usize,
}

/// Iterate `.symtab` entries in table order.
#[derive(Clone, Debug)]
pub struct Symbols<'a> {
    object: Object<'a>,
    table: usize,
    entry: usize,
    count: usize,
    entsize: usize,
    table_data: &'a [u8],
    strtab: &'a [u8],
}

impl<'a> Object<'a> {
    /// Parse an ELF32/ELF64 little-endian object's section table.
    ///
    /// Only the section table and its name string table are validated here; program headers are
    /// deliberately not required, because `ET_REL` objects have none. Symbols are parsed lazily
    /// by [`symbols`](Self::symbols).
    pub fn parse(data: &'a [u8]) -> Result<Self, ElfError> {
        if data.len() < EHDR32_SIZE {
            return Err(ElfError::TooShort);
        }
        if data.get(0..4) != Some(b"\x7fELF") {
            return Err(ElfError::BadMagic);
        }
        let class = match data.get(4).copied() {
            Some(ELFCLASS32) => Class::Elf32,
            Some(ELFCLASS64) => Class::Elf64,
            _ => return Err(ElfError::NotElf64),
        };
        if data.get(5) != Some(&ELFDATA2LSB) {
            return Err(ElfError::NotLittleEndian);
        }
        let header_size = match class {
            Class::Elf32 => EHDR32_SIZE,
            Class::Elf64 => EHDR64_SIZE,
        };
        if data.len() < header_size {
            return Err(ElfError::TooShort);
        }
        let e_type = read_u16_at(data, 16)?;
        if !matches!(e_type, ET_REL | ET_EXEC | ET_DYN) {
            return Err(ElfError::BadType);
        }
        let machine = read_u16_at(data, 18)?;
        let (shoff, shentsize, shnum, shstrndx) = match class {
            Class::Elf32 => (
                usize::try_from(read_u32_at(data, 0x20)?).map_err(|_| ElfError::BadSection)?,
                usize::from(read_u16_at(data, 0x2e)?),
                usize::from(read_u16_at(data, 0x30)?),
                usize::from(read_u16_at(data, 0x32)?),
            ),
            Class::Elf64 => (
                usize::try_from(read_u64_at(data, 0x28)?).map_err(|_| ElfError::BadSection)?,
                usize::from(read_u16_at(data, 0x3a)?),
                usize::from(read_u16_at(data, 0x3c)?),
                usize::from(read_u16_at(data, 0x3e)?),
            ),
        };
        let expected_size = section_header_size(class);
        if shnum == 0 && shoff != 0 {
            // Extended numbering: section 0's sh_size is the real count.
            let first = raw_section(data, class, shoff, 0, shentsize)?;
            let count = usize::try_from(first.size).map_err(|_| ElfError::BadSection)?;
            let mut extended = Self {
                data,
                class,
                e_type,
                machine,
                shoff,
                shentsize,
                shnum: count,
                shstrndx,
                shstr: &[],
            };
            if shstrndx == usize::from(SHN_XINDEX) {
                extended.shstrndx =
                    usize::try_from(first.link).map_err(|_| ElfError::BadSection)?;
            }
            extended.shstr = extended.name_table()?;
            return Ok(extended);
        }
        if shnum != 0 && shentsize != expected_size {
            return Err(ElfError::BadSection);
        }
        if shnum != 0 {
            let table_len = shnum.checked_mul(shentsize).ok_or(ElfError::BadSection)?;
            let table_end = shoff.checked_add(table_len).ok_or(ElfError::BadSection)?;
            if table_end > data.len() {
                return Err(ElfError::BadSection);
            }
        }
        let mut object = Self {
            data,
            class,
            e_type,
            machine,
            shoff,
            shentsize,
            shnum,
            shstrndx,
            shstr: &[],
        };
        object.shstr = object.name_table()?;
        Ok(object)
    }

    fn name_table(&self) -> Result<&'a [u8], ElfError> {
        if self.shstrndx >= self.shnum {
            return Ok(&[]);
        }
        let section = self.section(self.shstrndx).ok_or(ElfError::BadSection)?;
        Ok(section.data())
    }

    /// The ELF class.
    #[must_use]
    pub fn class(&self) -> Class {
        self.class
    }

    /// The `e_type` value (`ET_REL`, `ET_EXEC` or `ET_DYN`).
    #[must_use]
    pub fn object_type(&self) -> u16 {
        self.e_type
    }

    /// The `e_machine` value.
    #[must_use]
    pub fn machine(&self) -> u16 {
        self.machine
    }

    /// Number of section headers.
    #[must_use]
    pub fn section_count(&self) -> usize {
        self.shnum
    }

    /// Number of bytes in one section header.
    #[must_use]
    pub fn section_header_size(&self) -> usize {
        section_header_size(self.class)
    }

    /// Parse one section header by index.
    #[must_use]
    pub fn section(&self, index: usize) -> Option<Section<'a>> {
        if index >= self.shnum {
            return None;
        }
        let raw = raw_section(self.data, self.class, self.shoff, index, self.shentsize).ok()?;
        Some(Section {
            data: self.data,
            shstr: self.shstr,
            index,
            header_offset: raw.header_offset,
            name_offset: raw.name_offset,
            section_type: raw.section_type,
            flags: raw.flags,
            address: raw.address,
            offset: raw.offset,
            size: raw.size,
            link: raw.link,
            info: raw.info,
            align: raw.align,
            entsize: raw.entsize,
        })
    }

    /// Iterate section headers.
    #[must_use]
    pub fn sections(&self) -> Sections<'a> {
        Sections {
            object: *self,
            index: 0,
        }
    }

    /// Look up a NUL-terminated UTF-8 string in `section` at `offset`.
    #[must_use]
    pub fn string(&self, section: usize, offset: u32) -> Option<&'a str> {
        let table = self.section(section)?.data();
        string_at(table, offset)
    }

    /// Iterate every symbol in the `.symtab` sections, in table order.
    #[must_use]
    pub fn symbols(&self) -> Symbols<'a> {
        let mut symbols = Symbols {
            object: *self,
            table: 0,
            entry: 0,
            count: 0,
            entsize: 0,
            table_data: &[],
            strtab: &[],
        };
        symbols.load_table();
        symbols
    }
}

impl<'a> Section<'a> {
    /// Section index in the header table.
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /// File offset of this section header (for strip rewrites).
    #[must_use]
    pub fn header_offset(&self) -> usize {
        self.header_offset
    }

    /// Section name, if the name table is valid.
    #[must_use]
    pub fn name(&self) -> Option<&'a str> {
        string_at(self.shstr, self.name_offset)
    }

    /// Raw `sh_type`.
    #[must_use]
    pub fn section_type(&self) -> u32 {
        self.section_type
    }

    /// Raw `sh_flags`.
    #[must_use]
    pub fn flags(&self) -> u64 {
        self.flags
    }

    /// `sh_addr`.
    #[must_use]
    pub fn address(&self) -> u64 {
        self.address
    }

    /// File offset of the section contents.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Size of the section contents.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// `sh_link`.
    #[must_use]
    pub fn link(&self) -> u32 {
        self.link
    }

    /// `sh_info`.
    #[must_use]
    pub fn info(&self) -> u32 {
        self.info
    }

    /// `sh_addralign`.
    #[must_use]
    pub fn align(&self) -> u64 {
        self.align
    }

    /// `sh_entsize`.
    #[must_use]
    pub fn entsize(&self) -> u64 {
        self.entsize
    }

    /// Section contents; an invalid range yields an empty slice.
    #[must_use]
    pub fn data(&self) -> &'a [u8] {
        let Ok(start) = usize::try_from(self.offset) else {
            return &[];
        };
        let Ok(len) = usize::try_from(self.size) else {
            return &[];
        };
        let Some(end) = start.checked_add(len) else {
            return &[];
        };
        self.data.get(start..end).unwrap_or(&[])
    }
}

impl<'a> Iterator for Sections<'a> {
    type Item = Section<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let section = self.object.section(self.index)?;
        self.index += 1;
        Some(section)
    }
}

impl<'a> Symbol<'a> {
    /// Symbol name from its string table, if valid.
    #[must_use]
    pub fn name(&self) -> Option<&'a str> {
        string_at(self.strtab, self.name_offset)
    }

    /// `st_value`.
    #[must_use]
    pub fn value(&self) -> u64 {
        self.value
    }

    /// `st_size`.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Symbol type (`STT_*`).
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.info & 0x0f
    }

    /// Symbol binding (`STB_*`).
    #[must_use]
    pub fn binding(&self) -> u8 {
        self.info >> 4
    }

    /// Symbol visibility (`st_other & 3`).
    #[must_use]
    pub fn visibility(&self) -> u8 {
        self.other & 0x03
    }

    /// Section index (`SHN_*` or a section index).
    #[must_use]
    pub fn section_index(&self) -> u16 {
        self.shndx
    }
}

impl<'a> Symbols<'a> {
    fn load_table(&mut self) {
        while self.table < self.object.section_count() {
            let index = self.table;
            self.table += 1;
            let Some(section) = self.object.section(index) else {
                self.count = 0;
                continue;
            };
            if section.section_type() != SHT_SYMTAB {
                continue;
            }
            let default = match self.object.class {
                Class::Elf32 => SYM32_SIZE,
                Class::Elf64 => SYM64_SIZE,
            };
            let entsize = usize::try_from(section.entsize()).unwrap_or(0);
            let entsize = if entsize < default { default } else { entsize };
            let data = section.data();
            self.count = data.len() / entsize;
            self.entsize = entsize;
            self.table_data = data;
            self.strtab = self
                .object
                .section(section.link() as usize)
                .map_or(&[], |table| table.data());
            self.entry = 0;
            if self.count != 0 {
                return;
            }
        }
        self.count = 0;
        self.table_data = &[];
        self.strtab = &[];
    }

    fn read_symbol(&self, entry: usize) -> Option<Symbol<'a>> {
        let base = entry.checked_mul(self.entsize)?;
        match self.object.class {
            Class::Elf32 => {
                let name_offset = read_u32_at(self.table_data, base).ok()?;
                let value = u64::from(read_u32_at(self.table_data, base + 4).ok()?);
                let size = u64::from(read_u32_at(self.table_data, base + 8).ok()?);
                let info = *self.table_data.get(base + 12)?;
                let other = *self.table_data.get(base + 13)?;
                let shndx = read_u16_at(self.table_data, base + 14).ok()?;
                Some(Symbol {
                    strtab: self.strtab,
                    name_offset,
                    value,
                    size,
                    info,
                    other,
                    shndx,
                })
            }
            Class::Elf64 => {
                let name_offset = read_u32_at(self.table_data, base).ok()?;
                let info = *self.table_data.get(base + 4)?;
                let other = *self.table_data.get(base + 5)?;
                let shndx = read_u16_at(self.table_data, base + 6).ok()?;
                let value = read_u64_at(self.table_data, base + 8).ok()?;
                let size = read_u64_at(self.table_data, base + 16).ok()?;
                Some(Symbol {
                    strtab: self.strtab,
                    name_offset,
                    value,
                    size,
                    info,
                    other,
                    shndx,
                })
            }
        }
    }
}

impl<'a> Iterator for Symbols<'a> {
    type Item = Symbol<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.entry < self.count {
                let symbol = self.read_symbol(self.entry)?;
                self.entry += 1;
                return Some(symbol);
            }
            if self.table >= self.object.section_count() {
                return None;
            }
            self.load_table();
        }
    }
}

fn section_header_size(class: Class) -> usize {
    match class {
        Class::Elf32 => SHDR32_SIZE,
        Class::Elf64 => SHDR64_SIZE,
    }
}

fn raw_section(
    data: &[u8],
    class: Class,
    shoff: usize,
    index: usize,
    shentsize: usize,
) -> Result<RawSection, ElfError> {
    let expected = section_header_size(class);
    if shentsize < expected {
        return Err(ElfError::BadSection);
    }
    let header_offset = index
        .checked_mul(shentsize)
        .and_then(|delta| shoff.checked_add(delta))
        .ok_or(ElfError::BadSection)?;
    match class {
        Class::Elf32 => {
            let read32 = |offset: usize| read_u32_at(data, header_offset + offset);
            let name_offset = read32(0)?;
            let section_type = read32(4)?;
            let flags = u64::from(read32(8)?);
            let address = u64::from(read32(12)?);
            let offset = u64::from(read32(16)?);
            let size = u64::from(read32(20)?);
            let link = read32(24)?;
            let info = read32(28)?;
            let align = u64::from(read32(32)?);
            let entsize = u64::from(read32(36)?);
            Ok(RawSection {
                header_offset,
                name_offset,
                section_type,
                flags,
                address,
                offset,
                size,
                link,
                info,
                align,
                entsize,
            })
        }
        Class::Elf64 => {
            let name_offset = read_u32_at(data, header_offset)?;
            let section_type = read_u32_at(data, header_offset + 4)?;
            let flags = read_u64_at(data, header_offset + 8)?;
            let address = read_u64_at(data, header_offset + 16)?;
            let offset = read_u64_at(data, header_offset + 24)?;
            let size = read_u64_at(data, header_offset + 32)?;
            let link = read_u32_at(data, header_offset + 40)?;
            let info = read_u32_at(data, header_offset + 44)?;
            let align = read_u64_at(data, header_offset + 48)?;
            let entsize = read_u64_at(data, header_offset + 56)?;
            Ok(RawSection {
                header_offset,
                name_offset,
                section_type,
                flags,
                address,
                offset,
                size,
                link,
                info,
                align,
                entsize,
            })
        }
    }
}

fn string_at(table: &[u8], offset: u32) -> Option<&str> {
    let start = usize::try_from(offset).ok()?;
    let rest = table.get(start..)?;
    let end = rest.iter().position(|byte| *byte == 0)?;
    core::str::from_utf8(rest.get(..end)?).ok()
}

fn read_u16_at(data: &[u8], offset: usize) -> Result<u16, ElfError> {
    let end = offset.checked_add(2).ok_or(ElfError::BadSection)?;
    let bytes: [u8; 2] = data
        .get(offset..end)
        .ok_or(ElfError::BadSection)?
        .try_into()
        .map_err(|_| ElfError::BadSection)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32_at(data: &[u8], offset: usize) -> Result<u32, ElfError> {
    let end = offset.checked_add(4).ok_or(ElfError::BadSection)?;
    let bytes: [u8; 4] = data
        .get(offset..end)
        .ok_or(ElfError::BadSection)?
        .try_into()
        .map_err(|_| ElfError::BadSection)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64_at(data: &[u8], offset: usize) -> Result<u64, ElfError> {
    let end = offset.checked_add(8).ok_or(ElfError::BadSection)?;
    let bytes: [u8; 8] = data
        .get(offset..end)
        .ok_or(ElfError::BadSection)?
        .try_into()
        .map_err(|_| ElfError::BadSection)?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
#[path = "inspect_ut.rs"]
mod tests;
