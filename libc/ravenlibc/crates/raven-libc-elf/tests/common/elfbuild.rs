// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    dead_code,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::vec_init_then_push,
    clippy::assertions_on_constants
)]
// A test-only ELF writer. It is shared by the unit tests and the integration tests; not every
// helper is used by every consumer, and the arithmetic is over small layout constants.

//! A deterministic ET_DYN image writer for the tests.
//!
//! The image has an `RX` segment at vaddr 0 (header, program headers, code bytes) and an `RW`
//! segment at vaddr 0x1000 holding the dynamic array and every table. `p_offset == p_vaddr` for
//! both, so the in-memory image is the file bytes unchanged. The builder writes the SysV and GNU
//! hash tables itself (with the crate's hash functions) and sorts the dynamic symbols into GNU
//! bucket order, which is what a real linker produces.

use std::collections::BTreeMap;

pub const PAGE: u64 = 4096;
pub const DATA_BASE: u64 = 0x1000;
pub const RX_SIZE: usize = 0x1000;

const DT_NULL: i64 = 0;
const DT_NEEDED: i64 = 1;
const DT_PLTRELSZ: i64 = 2;
const DT_HASH: i64 = 4;
const DT_STRTAB: i64 = 5;
const DT_SYMTAB: i64 = 6;
const DT_RELA: i64 = 7;
const DT_RELASZ: i64 = 8;
const DT_RELAENT: i64 = 9;
const DT_STRSZ: i64 = 10;
const DT_SYMENT: i64 = 11;
const DT_INIT: i64 = 12;
const DT_FINI: i64 = 13;
const DT_SONAME: i64 = 14;
const DT_RPATH: i64 = 15;
const DT_SYMBOLIC: i64 = 16;
const DT_PLTREL: i64 = 20;
const DT_DEBUG: i64 = 21;
const DT_TEXTREL: i64 = 22;
const DT_JMPREL: i64 = 23;
const DT_BIND_NOW: i64 = 24;
const DT_INIT_ARRAY: i64 = 25;
const DT_FINI_ARRAY: i64 = 26;
const DT_INIT_ARRAYSZ: i64 = 27;
const DT_FINI_ARRAYSZ: i64 = 28;
const DT_RUNPATH: i64 = 29;
const DT_FLAGS: i64 = 30;
const DT_PREINIT_ARRAY: i64 = 32;
const DT_PREINIT_ARRAYSZ: i64 = 33;
const DT_RELRSZ: i64 = 35;
const DT_RELR: i64 = 36;
const DT_RELRENT: i64 = 37;
const DT_GNU_HASH: i64 = 0x6ffffef5;
const DT_VERSYM: i64 = 0x6ffffff0;
const DT_FLAGS_1: i64 = 0x6ffffffb;
const DT_VERDEF: i64 = 0x6ffffffc;
const DT_VERDEFNUM: i64 = 0x6ffffffd;
const DT_VERNEED: i64 = 0x6ffffffe;
const DT_VERNEEDNUM: i64 = 0x6fffffff;
const DT_RELA_TAG: i64 = 7;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_PHDR: u32 = 6;
const PT_TLS: u32 = 7;
const PT_GNU_EH_FRAME: u32 = 0x6474e550;
const PT_GNU_STACK: u32 = 0x6474e551;
const PT_GNU_RELRO: u32 = 0x6474e552;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;
const ET_DYN: u16 = 3;
const EM_X86_64: u16 = 62;

fn sysv(name: &[u8]) -> u32 {
    let mut hash: u64 = 0;
    for byte in name {
        hash = (hash << 4) + u64::from(*byte);
        hash ^= (hash >> 24) & 0xf0;
        hash &= 0xffff_ffff;
    }
    (hash & 0x0fff_ffff) as u32
}

fn gnu(name: &[u8]) -> u32 {
    let mut hash: u64 = 5381;
    for byte in name {
        hash = (hash * 33 + u64::from(*byte)) & 0xffff_ffff;
    }
    hash as u32
}

/// One dynamic symbol, before index assignment.
#[derive(Clone, Debug)]
pub struct SymbolSpec {
    pub name: Vec<u8>,
    pub info: u8,
    pub other: u8,
    pub shndx: u16,
    pub value: u64,
    pub size: u64,
    pub version: u16,
    pub hidden: bool,
}

/// A version definition (`DT_VERDEF` entry).
#[derive(Clone, Debug)]
pub struct VersionDef {
    pub name: Vec<u8>,
    pub index: u16,
    pub flags: u16,
}

/// A version requirement (`DT_VERNEED` entry).
#[derive(Clone, Debug)]
pub struct VersionNeed {
    pub file: Vec<u8>,
    pub versions: Vec<(Vec<u8>, u16, u16)>, // name, index, flags
}

/// Where each table landed in the image (all link-time virtual addresses).
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub dynamic: u64,
    pub dynstr: u64,
    pub dynsym: u64,
    pub hash: u64,
    pub gnu_hash: u64,
    pub versym: u64,
    pub verdef: u64,
    pub verneed: u64,
    pub rela: u64,
    pub plt_rela: u64,
    pub relr: u64,
    pub init_array: u64,
    pub fini_array: u64,
    pub preinit_array: u64,
    pub tls: u64,
    pub relro: u64,
    pub eh_frame: u64,
}

/// A built image.
#[derive(Clone, Debug)]
pub struct Image {
    pub bytes: Vec<u8>,
    pub phdr_vaddr: u64,
    pub phnum: u16,
    pub layout: Layout,
    pub symbol_index: BTreeMap<Vec<u8>, u32>,
}

/// The builder.
#[derive(Clone, Debug)]
pub struct Builder {
    symbols: Vec<SymbolSpec>,
    needed: Vec<Vec<u8>>,
    soname: Option<Vec<u8>>,
    rpath: Option<Vec<u8>>,
    runpath: Option<Vec<u8>>,
    flags: Option<u32>,
    flags1: Option<u32>,
    bind_now: bool,
    symbolic: bool,
    debug_slot: bool,
    version_defs: Vec<VersionDef>,
    version_needs: Vec<VersionNeed>,
    relas: Vec<(u64, u32, u32, i64)>,
    plt_relas: Vec<(u64, u32, u32, i64)>,
    relr: Vec<u64>,
    init_array: Vec<u64>,
    fini_array: Vec<u64>,
    preinit_array: Vec<u64>,
    init: Option<u64>,
    fini: Option<u64>,
    sysv_hash: bool,
    gnu_hash: bool,
    tls: Option<(Vec<u8>, u64, u64)>,
    relro: Option<(u64, u64)>,
    eh_frame: Option<(u64, u64)>,
    stack_exec: bool,
    extra_dyn: Vec<(i64, u64)>,
    data_bss: u64,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            needed: Vec::new(),
            soname: None,
            rpath: None,
            runpath: None,
            flags: None,
            flags1: None,
            bind_now: false,
            symbolic: false,
            debug_slot: false,
            version_defs: Vec::new(),
            version_needs: Vec::new(),
            relas: Vec::new(),
            plt_relas: Vec::new(),
            relr: Vec::new(),
            init_array: Vec::new(),
            fini_array: Vec::new(),
            preinit_array: Vec::new(),
            init: None,
            fini: None,
            sysv_hash: true,
            gnu_hash: false,
            tls: None,
            relro: None,
            eh_frame: None,
            stack_exec: false,
            extra_dyn: Vec::new(),
            data_bss: 0,
        }
    }

    pub fn symbol(mut self, symbol: SymbolSpec) -> Self {
        self.symbols.push(symbol);
        self
    }

    pub fn needed(mut self, name: &[u8]) -> Self {
        self.needed.push(name.to_vec());
        self
    }

    pub fn soname(mut self, name: &[u8]) -> Self {
        self.soname = Some(name.to_vec());
        self
    }

    pub fn rpath(mut self, path: &[u8]) -> Self {
        self.rpath = Some(path.to_vec());
        self
    }

    pub fn runpath(mut self, path: &[u8]) -> Self {
        self.runpath = Some(path.to_vec());
        self
    }

    pub fn flags(mut self, flags: u32) -> Self {
        self.flags = Some(flags);
        self
    }

    pub fn flags1(mut self, flags: u32) -> Self {
        self.flags1 = Some(flags);
        self
    }

    pub fn bind_now(mut self) -> Self {
        self.bind_now = true;
        self
    }

    pub fn symbolic(mut self) -> Self {
        self.symbolic = true;
        self
    }

    pub fn with_debug_slot(mut self) -> Self {
        self.debug_slot = true;
        self
    }

    pub fn version_def(mut self, definition: VersionDef) -> Self {
        self.version_defs.push(definition);
        self
    }

    pub fn version_need(mut self, need: VersionNeed) -> Self {
        self.version_needs.push(need);
        self
    }

    pub fn rela(mut self, offset: u64, sym: u32, r_type: u32, addend: i64) -> Self {
        self.relas.push((offset, sym, r_type, addend));
        self
    }

    pub fn plt_rela(mut self, offset: u64, sym: u32, r_type: u32, addend: i64) -> Self {
        self.plt_relas.push((offset, sym, r_type, addend));
        self
    }

    pub fn relr(mut self, entry: u64) -> Self {
        self.relr.push(entry);
        self
    }

    pub fn init_array(mut self, entries: &[u64]) -> Self {
        self.init_array = entries.to_vec();
        self
    }

    pub fn fini_array(mut self, entries: &[u64]) -> Self {
        self.fini_array = entries.to_vec();
        self
    }

    pub fn preinit_array(mut self, entries: &[u64]) -> Self {
        self.preinit_array = entries.to_vec();
        self
    }

    pub fn init(mut self, address: u64) -> Self {
        self.init = Some(address);
        self
    }

    pub fn fini(mut self, address: u64) -> Self {
        self.fini = Some(address);
        self
    }

    pub fn with_sysv_hash(mut self, on: bool) -> Self {
        self.sysv_hash = on;
        self
    }

    pub fn with_gnu_hash(mut self, on: bool) -> Self {
        self.gnu_hash = on;
        self
    }

    pub fn tls(mut self, bytes: &[u8], memsz: u64, align: u64) -> Self {
        self.tls = Some((bytes.to_vec(), memsz, align));
        self
    }

    pub fn relro(mut self, offset: u64, len: u64) -> Self {
        self.relro = Some((offset, len));
        self
    }

    pub fn eh_frame(mut self, offset: u64, len: u64) -> Self {
        self.eh_frame = Some((offset, len));
        self
    }

    pub fn stack_exec(mut self) -> Self {
        self.stack_exec = true;
        self
    }

    pub fn extra_dyn(mut self, tag: i64, value: u64) -> Self {
        self.extra_dyn.push((tag, value));
        self
    }

    pub fn data_bss(mut self, size: u64) -> Self {
        self.data_bss = size;
        self
    }

    /// Build the image.
    pub fn build(self) -> Image {
        let mut strings = StrTab::new();
        let mut symbol_index = BTreeMap::new();

        // Symbols: index 0 is the null symbol. With a GNU hash, the hashed symbols are sorted by
        // bucket so that every bucket's chain is contiguous.
        let mut order: Vec<usize> = (0..self.symbols.len()).collect();
        if self.gnu_hash {
            order.sort_by_key(|index| {
                let symbol = &self.symbols[*index];
                (gnu(&symbol.name) % 32, *index)
            });
        }
        let mut symbols = vec![SymbolSpec {
            name: Vec::new(),
            info: 0,
            other: 0,
            shndx: 0,
            value: 0,
            size: 0,
            version: 0,
            hidden: false,
        }];
        for index in &order {
            symbols.push(self.symbols[*index].clone());
        }
        for (index, symbol) in symbols.iter().enumerate() {
            if !symbol.name.is_empty() {
                symbol_index.insert(symbol.name.clone(), index as u32);
            }
        }

        // Dynamic string table.
        let name_offsets: Vec<u32> = symbols
            .iter()
            .map(|symbol| {
                if symbol.name.is_empty() {
                    0
                } else {
                    strings.add(&symbol.name)
                }
            })
            .collect();
        let needed_offsets: Vec<u32> = self.needed.iter().map(|name| strings.add(name)).collect();
        let soname_offset = self.soname.as_ref().map(|name| strings.add(name));
        let rpath_offset = self.rpath.as_ref().map(|path| strings.add(path));
        let runpath_offset = self.runpath.as_ref().map(|path| strings.add(path));
        let verdef_names: Vec<u32> = self
            .version_defs
            .iter()
            .map(|definition| strings.add(&definition.name))
            .collect();
        let verneed_files: Vec<u32> = self
            .version_needs
            .iter()
            .map(|need| strings.add(&need.file))
            .collect();
        let verneed_names: Vec<Vec<u32>> = self
            .version_needs
            .iter()
            .map(|need| {
                need.versions
                    .iter()
                    .map(|(name, _, _)| strings.add(name))
                    .collect()
            })
            .collect();

        let dynstr = strings.bytes;
        let dynstr_size = dynstr.len() as u64;

        // Dynamic symbols.
        let mut dynsym = Vec::with_capacity(symbols.len() * 24);
        for (index, symbol) in symbols.iter().enumerate() {
            dynsym.extend_from_slice(&name_offsets[index].to_le_bytes());
            dynsym.push(symbol.info);
            dynsym.push(symbol.other);
            dynsym.extend_from_slice(&symbol.shndx.to_le_bytes());
            dynsym.extend_from_slice(&symbol.value.to_le_bytes());
            dynsym.extend_from_slice(&symbol.size.to_le_bytes());
        }

        // Version symbol table.
        let mut versym = Vec::new();
        if !self.version_defs.is_empty() || !self.version_needs.is_empty() {
            for symbol in &symbols {
                let raw = symbol.version | if symbol.hidden { 0x8000 } else { 0 };
                versym.extend_from_slice(&raw.to_le_bytes());
            }
        }

        // SysV hash: nbucket words, nchain words, buckets, chains.
        let mut hash = Vec::new();
        if self.sysv_hash {
            let nchain = symbols.len() as u32;
            let nbucket = nchain.max(1);
            let mut buckets = vec![0u32; nbucket as usize];
            let mut chains = vec![0u32; nchain as usize];
            let mut insert = |index: u32, name: &[u8]| {
                let bucket = (sysv(name) % nbucket) as usize;
                let mut current = buckets[bucket];
                if current == 0 {
                    buckets[bucket] = index;
                    return;
                }
                loop {
                    let next = chains[current as usize];
                    if next == 0 {
                        chains[current as usize] = index;
                        return;
                    }
                    current = next;
                }
            };
            for (index, symbol) in symbols.iter().enumerate().skip(1) {
                insert(index as u32, &symbol.name);
            }
            hash.extend_from_slice(&nbucket.to_le_bytes());
            hash.extend_from_slice(&nchain.to_le_bytes());
            for bucket in buckets {
                hash.extend_from_slice(&bucket.to_le_bytes());
            }
            for chain in chains {
                hash.extend_from_slice(&chain.to_le_bytes());
            }
        }

        // GNU hash.
        let mut gnu_hash = Vec::new();
        if self.gnu_hash {
            let nbuckets = 32u32;
            let symoffset = 1u32;
            let bloom_size = 1u32;
            let bloom_shift = 6u32;
            let mut bloom = 0u64;
            let mut buckets = vec![0u32; nbuckets as usize];
            let count = symbols.len() as u32;
            let mut chains = vec![0u32; count.saturating_sub(symoffset) as usize];
            for index in symoffset..count {
                let hash = gnu(&symbols[index as usize].name);
                let bucket = hash % nbuckets;
                if buckets[bucket as usize] == 0 {
                    buckets[bucket as usize] = index;
                }
                bloom |= 1u64 << (hash % 64);
                bloom |= 1u64 << ((hash >> bloom_shift) % 64);
                let next = index + 1;
                let last = next >= count || gnu(&symbols[next as usize].name) % nbuckets != bucket;
                chains[(index - symoffset) as usize] = (hash & !1) | u32::from(last);
            }
            gnu_hash.extend_from_slice(&nbuckets.to_le_bytes());
            gnu_hash.extend_from_slice(&symoffset.to_le_bytes());
            gnu_hash.extend_from_slice(&bloom_size.to_le_bytes());
            gnu_hash.extend_from_slice(&bloom_shift.to_le_bytes());
            gnu_hash.extend_from_slice(&bloom.to_le_bytes());
            for bucket in buckets {
                gnu_hash.extend_from_slice(&bucket.to_le_bytes());
            }
            for chain in chains {
                gnu_hash.extend_from_slice(&chain.to_le_bytes());
            }
        }

        // Version definitions: 28-byte header + 8-byte Verdaux, chained.
        let mut verdef = Vec::new();
        for (index, definition) in self.version_defs.iter().enumerate() {
            let start = verdef.len();
            verdef.extend_from_slice(&1u16.to_le_bytes());
            verdef.extend_from_slice(&definition.flags.to_le_bytes());
            verdef.extend_from_slice(&definition.index.to_le_bytes());
            verdef.extend_from_slice(&2u16.to_le_bytes());
            verdef.extend_from_slice(&gnu(&definition.name).to_le_bytes());
            verdef.extend_from_slice(&20u32.to_le_bytes());
            let last = index + 1 == self.version_defs.len();
            verdef.extend_from_slice(&(if last { 0u32 } else { 28 }).to_le_bytes());
            verdef.extend_from_slice(&verdef_names[index].to_le_bytes());
            verdef.extend_from_slice(&0u32.to_le_bytes());
            assert_eq!(verdef.len() - start, 28);
        }

        // Version requirements: 16-byte header + Vernaux records.
        let mut verneed = Vec::new();
        for (index, need) in self.version_needs.iter().enumerate() {
            let start = verneed.len();
            verneed.extend_from_slice(&1u16.to_le_bytes());
            verneed.extend_from_slice(&(need.versions.len() as u16).to_le_bytes());
            verneed.extend_from_slice(&verneed_files[index].to_le_bytes());
            verneed.extend_from_slice(&16u32.to_le_bytes());
            let last = index + 1 == self.version_needs.len();
            verneed.extend_from_slice(
                &(if last {
                    0u32
                } else {
                    32 + 16 * need.versions.len() as u32
                })
                .to_le_bytes(),
            );
            for (position, (name, other, flags)) in need.versions.iter().enumerate() {
                verneed.extend_from_slice(&gnu(name).to_le_bytes());
                verneed.extend_from_slice(&flags.to_le_bytes());
                verneed.extend_from_slice(&other.to_le_bytes());
                verneed.extend_from_slice(&verneed_names[index][position].to_le_bytes());
                let last_aux = position + 1 == need.versions.len();
                verneed.extend_from_slice(&(if last_aux { 0u32 } else { 16 }).to_le_bytes());
            }
            assert!(verneed.len() > start);
        }

        // Relocations.
        let encode_rela = |entries: &[(u64, u32, u32, i64)]| {
            let mut bytes = Vec::with_capacity(entries.len() * 24);
            for (offset, sym, r_type, addend) in entries {
                bytes.extend_from_slice(&offset.to_le_bytes());
                bytes.extend_from_slice(
                    &((u64::from(*sym) << 32) | u64::from(*r_type)).to_le_bytes(),
                );
                bytes.extend_from_slice(&addend.to_le_bytes());
            }
            bytes
        };
        let rela = encode_rela(&self.relas);
        let plt_rela = encode_rela(&self.plt_relas);

        let mut relr = Vec::new();
        for entry in &self.relr {
            relr.extend_from_slice(&entry.to_le_bytes());
        }

        let encode_array = |entries: &[u64]| {
            let mut bytes = Vec::with_capacity(entries.len() * 8);
            for entry in entries {
                bytes.extend_from_slice(&entry.to_le_bytes());
            }
            bytes
        };
        let init_array = encode_array(&self.init_array);
        let fini_array = encode_array(&self.fini_array);
        let preinit_array = encode_array(&self.preinit_array);

        // Assign data offsets.
        let mut layout = Layout::default();
        let mut blobs: Vec<(&str, Vec<u8>)> = Vec::new();
        let mut offsets: BTreeMap<&str, u64> = BTreeMap::new();
        let mut cursor = 0u64;
        let mut place = |name: &'static str,
                         bytes: Vec<u8>,
                         offsets: &mut BTreeMap<&'static str, u64>,
                         cursor: &mut u64| {
            if bytes.is_empty() {
                return;
            }
            *cursor = (*cursor + 7) & !7;
            offsets.insert(name, *cursor);
            *cursor += bytes.len() as u64;
            blobs.push((name, bytes));
        };
        // Reserve the dynamic array last but place it first in the segment.
        let dyn_placeholder = vec![0u8; dyn_placeholder_len(self.extra_dyn.len())];
        place("dynamic", dyn_placeholder, &mut offsets, &mut cursor);
        place("dynstr", dynstr, &mut offsets, &mut cursor);
        place("dynsym", dynsym, &mut offsets, &mut cursor);
        place("hash", hash, &mut offsets, &mut cursor);
        place("gnu_hash", gnu_hash, &mut offsets, &mut cursor);
        place("versym", versym, &mut offsets, &mut cursor);
        place("verdef", verdef, &mut offsets, &mut cursor);
        place("verneed", verneed, &mut offsets, &mut cursor);
        place("rela", rela, &mut offsets, &mut cursor);
        place("plt_rela", plt_rela, &mut offsets, &mut cursor);
        place("relr", relr, &mut offsets, &mut cursor);
        place("init_array", init_array, &mut offsets, &mut cursor);
        place("fini_array", fini_array, &mut offsets, &mut cursor);
        place("preinit_array", preinit_array, &mut offsets, &mut cursor);
        let tls_bytes = self
            .tls
            .as_ref()
            .map(|(bytes, _, _)| bytes.clone())
            .unwrap_or_default();
        place("tls", tls_bytes, &mut offsets, &mut cursor);
        cursor = (cursor + 7) & !7;
        let data_len = cursor;

        let address = |name: &str| offset_of(&offsets, name);
        layout.dynstr = address("dynstr");
        layout.dynsym = address("dynsym");
        layout.hash = address("hash");
        layout.gnu_hash = address("gnu_hash");
        layout.versym = address("versym");
        layout.verdef = address("verdef");
        layout.verneed = address("verneed");
        layout.rela = address("rela");
        layout.plt_rela = address("plt_rela");
        layout.relr = address("relr");
        layout.init_array = address("init_array");
        layout.fini_array = address("fini_array");
        layout.preinit_array = address("preinit_array");
        layout.tls = address("tls");
        layout.eh_frame = self.eh_frame.map(|(offset, _)| offset).unwrap_or(0);
        layout.dynamic = address("dynamic");
        layout.relro = self.relro.map(|(offset, _)| offset).unwrap_or(0);

        // Program headers.
        let mut headers: Vec<[u64; 8]> = Vec::new();
        let mut types: Vec<(u32, u32)> = Vec::new();
        // PT_PHDR
        types.push((PT_PHDR, PF_R));
        // PT_LOAD RX
        types.push((PT_LOAD, PF_R | PF_X));
        // PT_LOAD RW
        types.push((PT_LOAD, PF_R | PF_W));
        types.push((PT_DYNAMIC, PF_R | PF_W));
        if self.tls.is_some() {
            types.push((PT_TLS, PF_R));
        }
        if self.eh_frame.is_some() {
            types.push((PT_GNU_EH_FRAME, PF_R));
        }
        if self.relro.is_some() {
            types.push((PT_GNU_RELRO, PF_R));
        }
        types.push((
            PT_GNU_STACK,
            if self.stack_exec {
                PF_R | PF_W | PF_X
            } else {
                PF_R | PF_W
            },
        ));
        let phnum = types.len() as u64;

        // Dynamic entries.
        let dyn_strtab = DATA_BASE + layout.dynstr;
        let dyn_symtab = DATA_BASE + layout.dynsym;
        let mut entries: Vec<(i64, u64)> = Vec::new();
        for offset in &needed_offsets {
            entries.push((DT_NEEDED, u64::from(*offset)));
        }
        entries.push((DT_STRTAB, dyn_strtab));
        entries.push((DT_STRSZ, dynstr_size));
        entries.push((DT_SYMTAB, dyn_symtab));
        entries.push((DT_SYMENT, 24));
        if self.sysv_hash {
            entries.push((DT_HASH, DATA_BASE + layout.hash));
        }
        if self.gnu_hash {
            entries.push((DT_GNU_HASH, DATA_BASE + layout.gnu_hash));
        }
        if layout.versym != 0 {
            entries.push((DT_VERSYM, DATA_BASE + layout.versym));
        }
        if layout.verdef != 0 {
            entries.push((DT_VERDEF, DATA_BASE + layout.verdef));
            entries.push((DT_VERDEFNUM, self.version_defs.len() as u64));
        }
        if layout.verneed != 0 {
            entries.push((DT_VERNEED, DATA_BASE + layout.verneed));
            entries.push((DT_VERNEEDNUM, self.version_needs.len() as u64));
        }
        if layout.rela != 0 {
            entries.push((DT_RELA_TAG, DATA_BASE + layout.rela));
            entries.push((DT_RELASZ, self.relas.len() as u64 * 24));
            entries.push((DT_RELAENT, 24));
        }
        if layout.plt_rela != 0 {
            entries.push((DT_JMPREL, DATA_BASE + layout.plt_rela));
            entries.push((DT_PLTRELSZ, self.plt_relas.len() as u64 * 24));
            entries.push((DT_PLTREL, DT_RELA as u64));
        }
        if layout.relr != 0 {
            entries.push((DT_RELR, DATA_BASE + layout.relr));
            entries.push((DT_RELRSZ, self.relr.len() as u64 * 8));
            entries.push((DT_RELRENT, 8));
        }
        if let Some(init) = self.init {
            entries.push((DT_INIT, init));
        }
        if let Some(fini) = self.fini {
            entries.push((DT_FINI, fini));
        }
        if layout.init_array != 0 {
            entries.push((DT_INIT_ARRAY, DATA_BASE + layout.init_array));
            entries.push((DT_INIT_ARRAYSZ, self.init_array.len() as u64 * 8));
        }
        if layout.fini_array != 0 {
            entries.push((DT_FINI_ARRAY, DATA_BASE + layout.fini_array));
            entries.push((DT_FINI_ARRAYSZ, self.fini_array.len() as u64 * 8));
        }
        if layout.preinit_array != 0 {
            entries.push((DT_PREINIT_ARRAY, DATA_BASE + layout.preinit_array));
            entries.push((DT_PREINIT_ARRAYSZ, self.preinit_array.len() as u64 * 8));
        }
        if let Some(flags) = self.flags {
            entries.push((DT_FLAGS, u64::from(flags)));
        }
        if let Some(flags1) = self.flags1 {
            entries.push((DT_FLAGS_1, u64::from(flags1)));
        }
        if self.symbolic {
            entries.push((DT_SYMBOLIC, 0));
        }
        if self.bind_now {
            entries.push((DT_BIND_NOW, 0));
        }
        if self.debug_slot {
            entries.push((DT_DEBUG, 0));
        }
        if let Some(offset) = soname_offset {
            entries.push((DT_SONAME, u64::from(offset)));
        }
        if let Some(offset) = rpath_offset {
            entries.push((DT_RPATH, u64::from(offset)));
        }
        if let Some(offset) = runpath_offset {
            entries.push((DT_RUNPATH, u64::from(offset)));
        }
        entries.extend(self.extra_dyn.iter().copied());
        entries.push((DT_NULL, 0));
        assert!(entries.len() * 16 <= dyn_placeholder_len(self.extra_dyn.len()));

        // Encode the dynamic array.
        let mut dynamic = Vec::with_capacity(entries.len() * 16);
        for (tag, value) in &entries {
            dynamic.extend_from_slice(&tag.to_le_bytes());
            dynamic.extend_from_slice(&value.to_le_bytes());
        }
        // Pad the dynamic blob to its reserved size so later offsets do not move.
        dynamic.resize(dyn_placeholder_len(self.extra_dyn.len()), 0);

        // Program header values in the same order as `types`.
        let data_memsz = data_len + self.data_bss;
        let mut phdr_index = 0usize;
        for (p_type, p_flags) in &types {
            let record = match *p_type {
                PT_PHDR => [
                    6,
                    u64::from(*p_flags),
                    64,
                    64,
                    64,
                    phnum * 56,
                    phnum * 56,
                    8,
                ],
                PT_LOAD if phdr_index == 0 => {
                    phdr_index += 1;
                    [
                        1,
                        u64::from(*p_flags),
                        0,
                        0,
                        0,
                        RX_SIZE as u64,
                        RX_SIZE as u64,
                        PAGE,
                    ]
                }
                PT_LOAD => [
                    1,
                    u64::from(*p_flags),
                    DATA_BASE,
                    DATA_BASE,
                    DATA_BASE,
                    data_len,
                    data_memsz,
                    PAGE,
                ],
                PT_DYNAMIC => [
                    2,
                    u64::from(*p_flags),
                    DATA_BASE + layout.dynamic,
                    DATA_BASE + layout.dynamic,
                    DATA_BASE + layout.dynamic,
                    dynamic.len() as u64,
                    dynamic.len() as u64,
                    8,
                ],
                PT_TLS => {
                    let (bytes, memsz, align) = self.tls.as_ref().unwrap();
                    [
                        7,
                        u64::from(*p_flags),
                        DATA_BASE + layout.tls,
                        DATA_BASE + layout.tls,
                        DATA_BASE + layout.tls,
                        bytes.len() as u64,
                        *memsz,
                        *align,
                    ]
                }
                PT_GNU_EH_FRAME => {
                    let (offset, len) = self.eh_frame.unwrap();
                    [
                        0x6474e550,
                        u64::from(*p_flags),
                        DATA_BASE + offset,
                        DATA_BASE + offset,
                        DATA_BASE + offset,
                        len,
                        len,
                        8,
                    ]
                }
                PT_GNU_RELRO => {
                    let (offset, len) = self.relro.unwrap();
                    [
                        0x6474e552,
                        u64::from(*p_flags),
                        DATA_BASE + offset,
                        DATA_BASE + offset,
                        DATA_BASE + offset,
                        len,
                        len,
                        8,
                    ]
                }
                _ => [0x6474e551, u64::from(*p_flags), 0, 0, 0, 0, 0, 16],
            };
            headers.push(record);
        }

        // Assemble the file.
        let mut bytes = vec![0u8; RX_SIZE + data_len as usize];
        bytes[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[6] = 1;
        bytes[16..18].copy_from_slice(&ET_DYN.to_le_bytes());
        bytes[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        bytes[24..32].copy_from_slice(&0u64.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[52..54].copy_from_slice(&64u16.to_le_bytes());
        bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
        bytes[56..58].copy_from_slice(&(headers.len() as u16).to_le_bytes());
        // A few code bytes so the RX segment is not all zero.
        bytes[0x200] = 0xc3;

        let mut phdr_offset = 64usize;
        for record in &headers {
            for (field, value) in record.iter().enumerate() {
                if field == 0 || field == 1 {
                    bytes[phdr_offset + field * 4..phdr_offset + field * 4 + 4]
                        .copy_from_slice(&(*value as u32).to_le_bytes());
                } else {
                    let at = phdr_offset + 8 + (field - 2) * 8;
                    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
                }
            }
            phdr_offset += 56;
        }
        for (name, blob) in &blobs {
            if *name == "dynamic" {
                continue;
            }
            let offset = DATA_BASE as usize + offset_of(&offsets, name) as usize;
            bytes[offset..offset + blob.len()].copy_from_slice(blob);
        }
        let dyn_offset = DATA_BASE as usize + layout.dynamic as usize;
        bytes[dyn_offset..dyn_offset + dynamic.len()].copy_from_slice(&dynamic);

        Image {
            bytes,
            phdr_vaddr: 64,
            phnum: headers.len() as u16,
            layout,
            symbol_index,
        }
    }
}

fn dyn_placeholder_len(extra: usize) -> usize {
    16 * (extra + 512)
}

fn offset_of(offsets: &BTreeMap<&str, u64>, name: &str) -> u64 {
    offsets.get(name).copied().unwrap_or(0)
}

struct StrTab {
    bytes: Vec<u8>,
    len: usize,
}

impl StrTab {
    fn new() -> Self {
        Self {
            bytes: vec![0],
            len: 0,
        }
    }

    fn add(&mut self, text: &[u8]) -> u32 {
        let offset = self.bytes.len() as u32;
        self.bytes.extend_from_slice(text);
        self.bytes.push(0);
        self.len += text.len() + 1;
        offset
    }
}

impl Image {
    /// The image as it would be mapped at `bias`.
    pub fn memory(&self, bias: u64) -> raven_libc_elf::mem::SliceMem<'_> {
        raven_libc_elf::mem::SliceMem::new(bias, &self.bytes)
    }

    /// The link-time address of a layout field.
    pub fn address(&self, field: u64) -> u64 {
        DATA_BASE + field
    }
}

/// A convenience symbol: a global function.
pub fn global_func(name: &[u8], value: u64) -> SymbolSpec {
    SymbolSpec {
        name: name.to_vec(),
        info: (1 << 4) | 2,
        other: 0,
        shndx: 1,
        value,
        size: 16,
        version: 1,
        hidden: false,
    }
}

/// A convenience symbol: a global data object.
pub fn global_object(name: &[u8], value: u64) -> SymbolSpec {
    SymbolSpec {
        name: name.to_vec(),
        info: (1 << 4) | 1,
        other: 0,
        shndx: 1,
        value,
        size: 8,
        version: 1,
        hidden: false,
    }
}

/// A convenience symbol: a weak, undefined function.
pub fn weak_undefined(name: &[u8]) -> SymbolSpec {
    SymbolSpec {
        name: name.to_vec(),
        info: (2 << 4) | 2,
        other: 0,
        shndx: 0,
        value: 0,
        size: 0,
        version: 0,
        hidden: false,
    }
}

/// A convenience symbol: a TLS object.
pub fn tls_object(name: &[u8], value: u64) -> SymbolSpec {
    SymbolSpec {
        name: name.to_vec(),
        info: (1 << 4) | 6,
        other: 0,
        shndx: 1,
        value,
        size: 8,
        version: 0,
        hidden: false,
    }
}
