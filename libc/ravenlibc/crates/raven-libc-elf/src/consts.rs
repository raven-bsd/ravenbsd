// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Every ELF number this crate uses.
//!
//! Numbers are facts from the System V gABI, the psABI of the three targets and `glibc/elf/elf.h`
//! (read for the numbers only; the file is LGPL). `consts_ut.rs` re-reads the header when the
//! glibc checkout is present and compares every group, so a typo fails a test. No magic number
//! appears anywhere else in the crate.

// ELF header.
/// `e_ident[EI_MAG0..3]`: `0x7f 'E' 'L' 'F'`.
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
/// `e_ident[EI_CLASS]`: 64-bit objects.
pub const ELFCLASS64: u8 = 2;
/// `e_ident[EI_DATA]`: little endian.
pub const ELFDATA2LSB: u8 = 1;
/// `e_ident[EI_VERSION]`: the only version.
pub const EV_CURRENT: u8 = 1;
/// Size of `Elf64_Ehdr`.
pub const EHDR_SIZE: usize = 64;
/// Size of `Elf64_Phdr`.
pub const PHDR_SIZE: usize = 56;
/// Size of `Elf64_Dyn`.
pub const DYN_SIZE: usize = 16;
/// Size of `Elf64_Sym`.
pub const SYM_SIZE: usize = 24;
/// Size of `Elf64_Rela`.
pub const RELA_SIZE: usize = 24;
/// Size of `Elf64_Relr` (a pointer-sized word).
pub const RELR_ENTRY_SIZE: usize = 8;
/// Size of `Elf64_Verdef`.
pub const VERDEF_SIZE: usize = 20;
/// Size of `Elf64_Verdaux`.
pub const VERDAUX_SIZE: usize = 8;
/// Size of `Elf64_Verneed`.
pub const VERNEED_SIZE: usize = 16;
/// Size of `Elf64_Vernaux`.
pub const VERNAUX_SIZE: usize = 16;
/// Size of `Elf_Nhdr` (the note header).
pub const NOTE_HEADER_SIZE: usize = 12;

// e_type.
/// Relocatable object (not loadable).
pub const ET_REL: u16 = 1;
/// Executable (may be non-PIE).
pub const ET_EXEC: u16 = 2;
/// Shared object or PIE executable.
pub const ET_DYN: u16 = 3;

// e_machine.
/// AMD x86-64.
pub const EM_X86_64: u16 = 62;
/// ARM AArch64.
pub const EM_AARCH64: u16 = 183;
/// RISC-V 64-bit.
pub const EM_RISCV: u16 = 243;

// p_type.
/// Loadable segment.
pub const PT_LOAD: u32 = 1;
/// Dynamic linking information.
pub const PT_DYNAMIC: u32 = 2;
/// Interpreter path.
pub const PT_INTERP: u32 = 3;
/// Auxiliary information (notes).
pub const PT_NOTE: u32 = 4;
/// Program header table.
pub const PT_PHDR: u32 = 6;
/// Thread-local storage template.
pub const PT_TLS: u32 = 7;
/// `.eh_frame_hdr`.
pub const PT_GNU_EH_FRAME: u32 = 0x6474_e550;
/// Stack executability.
pub const PT_GNU_STACK: u32 = 0x6474_e551;
/// Read-only after relocation.
pub const PT_GNU_RELRO: u32 = 0x6474_e552;
/// GNU property notes.
pub const PT_GNU_PROPERTY: u32 = 0x6474_e553;

// p_flags.
/// Executable.
pub const PF_X: u32 = 1;
/// Writable.
pub const PF_W: u32 = 2;
/// Readable.
pub const PF_R: u32 = 4;

// d_tag.
/// End of the dynamic array.
pub const DT_NULL: i64 = 0;
/// A dependency name (string-table offset).
pub const DT_NEEDED: i64 = 1;
/// PLT relocation size in bytes.
pub const DT_PLTRELSZ: i64 = 2;
/// PLT GOT address.
pub const DT_PLTGOT: i64 = 3;
/// SysV hash table address.
pub const DT_HASH: i64 = 4;
/// Dynamic string table address.
pub const DT_STRTAB: i64 = 5;
/// Dynamic symbol table address.
pub const DT_SYMTAB: i64 = 6;
/// RELA table address.
pub const DT_RELA: i64 = 7;
/// RELA table size in bytes.
pub const DT_RELASZ: i64 = 8;
/// RELA entry size.
pub const DT_RELAENT: i64 = 9;
/// String table size in bytes.
pub const DT_STRSZ: i64 = 10;
/// Symbol table entry size.
pub const DT_SYMENT: i64 = 11;
/// Initialization function.
pub const DT_INIT: i64 = 12;
/// Finalization function.
pub const DT_FINI: i64 = 13;
/// Shared object name (string-table offset).
pub const DT_SONAME: i64 = 14;
/// Run-time search path.
pub const DT_RPATH: i64 = 15;
/// Symbolic binding.
pub const DT_SYMBOLIC: i64 = 16;
/// REL table address (rejected).
pub const DT_REL: i64 = 17;
/// REL table size (rejected).
pub const DT_RELSZ: i64 = 18;
/// REL entry size (rejected).
pub const DT_RELENT: i64 = 19;
/// Type of the PLT relocations (`DT_REL` or `DT_RELA`).
pub const DT_PLTREL: i64 = 20;
/// Debugger interface slot.
pub const DT_DEBUG: i64 = 21;
/// Text relocations present.
pub const DT_TEXTREL: i64 = 22;
/// PLT relocation address.
pub const DT_JMPREL: i64 = 23;
/// Bind now.
pub const DT_BIND_NOW: i64 = 24;
/// Init array address.
pub const DT_INIT_ARRAY: i64 = 25;
/// Fini array address.
pub const DT_FINI_ARRAY: i64 = 26;
/// Init array size in bytes.
pub const DT_INIT_ARRAYSZ: i64 = 27;
/// Fini array size in bytes.
pub const DT_FINI_ARRAYSZ: i64 = 28;
/// New-dtags search path.
pub const DT_RUNPATH: i64 = 29;
/// `DF_*` flags.
pub const DT_FLAGS: i64 = 30;
/// Pre-init array address.
pub const DT_PREINIT_ARRAY: i64 = 32;
/// Pre-init array size in bytes.
pub const DT_PREINIT_ARRAYSZ: i64 = 33;
/// RELR table size in bytes.
pub const DT_RELRSZ: i64 = 35;
/// RELR table address.
pub const DT_RELR: i64 = 36;
/// RELR entry size.
pub const DT_RELRENT: i64 = 37;
/// GNU hash table address.
pub const DT_GNU_HASH: i64 = 0x6fff_fef5;
/// Symbol version index table.
pub const DT_VERSYM: i64 = 0x6fff_fff0;
/// Number of RELA relocations.
pub const DT_RELACOUNT: i64 = 0x6fff_fff9;
/// `DF_1_*` flags.
pub const DT_FLAGS_1: i64 = 0x6fff_fffb;
/// Version definition table.
pub const DT_VERDEF: i64 = 0x6fff_fffc;
/// Number of version definitions.
pub const DT_VERDEFNUM: i64 = 0x6fff_fffd;
/// Version requirement table.
pub const DT_VERNEED: i64 = 0x6fff_fffe;
/// Number of version requirements.
pub const DT_VERNEEDNUM: i64 = 0x6fff_ffff;

// d_un.d_val flags.
/// `DF_ORIGIN`: an `$ORIGIN` path is used.
pub const DF_ORIGIN: u64 = 0x1;
/// `DF_SYMBOLIC`: symbolic binding.
pub const DF_SYMBOLIC: u64 = 0x2;
/// `DF_TEXTREL`: text relocations.
pub const DF_TEXTREL: u64 = 0x4;
/// `DF_BIND_NOW`: bind all symbols at load.
pub const DF_BIND_NOW: u64 = 0x8;
/// `DF_STATIC_TLS`: uses static TLS.
pub const DF_STATIC_TLS: u64 = 0x10;

// DT_FLAGS_1 bits.
/// `DF_1_NOW`.
pub const DF_1_NOW: u64 = 0x1;
/// `DF_1_GLOBAL`.
pub const DF_1_GLOBAL: u64 = 0x2;
/// `DF_1_NODELETE`.
pub const DF_1_NODELETE: u64 = 0x8;
/// `DF_1_NOOPEN`.
pub const DF_1_NOOPEN: u64 = 0x40;
/// `DF_1_ORIGIN`.
pub const DF_1_ORIGIN: u64 = 0x80;
/// `DF_1_INTERPOSE`.
pub const DF_1_INTERPOSE: u64 = 0x400;
/// `DF_1_NODEFLIB`.
pub const DF_1_NODEFLIB: u64 = 0x800;
/// `DF_1_PIE`.
pub const DF_1_PIE: u64 = 0x0800_0000;

// Symbol types (st_info & 0xf).
/// No type.
pub const STT_NOTYPE: u8 = 0;
/// Data object.
pub const STT_OBJECT: u8 = 1;
/// Function.
pub const STT_FUNC: u8 = 2;
/// Section symbol.
pub const STT_SECTION: u8 = 3;
/// File name.
pub const STT_FILE: u8 = 4;
/// Common block.
pub const STT_COMMON: u8 = 5;
/// Thread-local storage.
pub const STT_TLS: u8 = 6;
/// GNU indirect function.
pub const STT_GNU_IFUNC: u8 = 10;

// Symbol bindings (st_info >> 4).
/// Local.
pub const STB_LOCAL: u8 = 0;
/// Global.
pub const STB_GLOBAL: u8 = 1;
/// Weak.
pub const STB_WEAK: u8 = 2;

// Symbol visibility (st_other & 3).
/// Default visibility.
pub const STV_DEFAULT: u8 = 0;
/// Internal visibility.
pub const STV_INTERNAL: u8 = 1;
/// Hidden visibility.
pub const STV_HIDDEN: u8 = 2;
/// Protected visibility.
pub const STV_PROTECTED: u8 = 3;

// st_shndx special values.
/// Undefined symbol.
pub const SHN_UNDEF: u16 = 0;
/// Absolute symbol.
pub const SHN_ABS: u16 = 0xfff1;
/// Common symbol.
pub const SHN_COMMON: u16 = 0xfff2;

// Versioning.
/// `VER_FLG_BASE`: the file's own version node.
pub const VER_FLG_BASE: u16 = 1;
/// `VER_FLG_WEAK`: a missing definition is not an error.
pub const VER_FLG_WEAK: u16 = 2;
/// `VER_NDX_LOCAL`.
pub const VER_NDX_LOCAL: u16 = 0;
/// `VER_NDX_GLOBAL`.
pub const VER_NDX_GLOBAL: u16 = 1;
/// First index a definition or requirement may use.
pub const VER_NDX_LORESERVE: u16 = 0xff00;
/// The first index that carries a real version name (`VER_NDX_GIVEN`).
pub const VER_NDX_GIVEN: u16 = 2;
/// Hidden-version bit in `DT_VERSYM`.
pub const VER_NDX_HIDDEN: u16 = 0x8000;
/// Mask that strips the hidden bit.
pub const VER_NDX_MASK: u16 = 0x7fff;
/// `VER_DEF_CURRENT`.
pub const VER_DEF_CURRENT: u16 = 1;
/// `VER_NEED_CURRENT`.
pub const VER_NEED_CURRENT: u16 = 1;

// x86_64 relocations (R_X86_64_*).
/// `R_X86_64_NONE`.
pub const R_X86_64_NONE: u32 = 0;
/// `R_X86_64_64`.
pub const R_X86_64_64: u32 = 1;
/// `R_X86_64_PC32`.
pub const R_X86_64_PC32: u32 = 2;
/// `R_X86_64_COPY`.
pub const R_X86_64_COPY: u32 = 5;
/// `R_X86_64_GLOB_DAT`.
pub const R_X86_64_GLOB_DAT: u32 = 6;
/// `R_X86_64_JUMP_SLOT`.
pub const R_X86_64_JUMP_SLOT: u32 = 7;
/// `R_X86_64_RELATIVE`.
pub const R_X86_64_RELATIVE: u32 = 8;
/// `R_X86_64_DTPMOD64`.
pub const R_X86_64_DTPMOD64: u32 = 16;
/// `R_X86_64_DTPOFF64`.
pub const R_X86_64_DTPOFF64: u32 = 17;
/// `R_X86_64_TPOFF64`.
pub const R_X86_64_TPOFF64: u32 = 18;
/// `R_X86_64_DTPOFF32` (rejected).
pub const R_X86_64_DTPOFF32: u32 = 21;
/// `R_X86_64_TPOFF32` (rejected).
pub const R_X86_64_TPOFF32: u32 = 23;
/// `R_X86_64_TLSDESC`.
pub const R_X86_64_TLSDESC: u32 = 36;
/// `R_X86_64_IRELATIVE`.
pub const R_X86_64_IRELATIVE: u32 = 37;

// aarch64 relocations, by number (A-ld-21).
/// `R_AARCH64_NONE`.
pub const R_AARCH64_NONE: u32 = 0;
/// `R_AARCH64_ABS64`.
pub const R_AARCH64_ABS64: u32 = 257;
/// `R_AARCH64_COPY`.
pub const R_AARCH64_COPY: u32 = 1024;
/// `R_AARCH64_GLOB_DAT`.
pub const R_AARCH64_GLOB_DAT: u32 = 1025;
/// `R_AARCH64_JUMP_SLOT`.
pub const R_AARCH64_JUMP_SLOT: u32 = 1026;
/// `R_AARCH64_RELATIVE`.
pub const R_AARCH64_RELATIVE: u32 = 1027;
/// `R_AARCH64_TLS_DTPMOD64` (module id, by number).
pub const R_AARCH64_TLS_DTPMOD64: u32 = 1028;
/// `R_AARCH64_TLS_DTPREL64` (offset inside the module, by number).
pub const R_AARCH64_TLS_DTPREL64: u32 = 1029;
/// `R_AARCH64_TLS_TPREL64`.
pub const R_AARCH64_TLS_TPREL64: u32 = 1030;
/// `R_AARCH64_TLSDESC`.
pub const R_AARCH64_TLSDESC: u32 = 1031;
/// `R_AARCH64_IRELATIVE`.
pub const R_AARCH64_IRELATIVE: u32 = 1032;

// riscv64 relocations. There is no GLOB_DAT on riscv64 (T-8).
/// `R_RISCV_NONE`.
pub const R_RISCV_NONE: u32 = 0;
/// `R_RISCV_64`.
pub const R_RISCV_64: u32 = 2;
/// `R_RISCV_RELATIVE`.
pub const R_RISCV_RELATIVE: u32 = 3;
/// `R_RISCV_COPY`.
pub const R_RISCV_COPY: u32 = 4;
/// `R_RISCV_JUMP_SLOT`.
pub const R_RISCV_JUMP_SLOT: u32 = 5;
/// `R_RISCV_TLS_DTPMOD64`.
pub const R_RISCV_TLS_DTPMOD64: u32 = 7;
/// `R_RISCV_TLS_DTPREL64`.
pub const R_RISCV_TLS_DTPREL64: u32 = 9;
/// `R_RISCV_TLS_TPREL64`.
pub const R_RISCV_TLS_TPREL64: u32 = 11;
/// `R_RISCV_TLSDESC` (rejected, O-3).
pub const R_RISCV_TLSDESC: u32 = 12;
/// `R_RISCV_IRELATIVE`.
pub const R_RISCV_IRELATIVE: u32 = 58;

// TLS layout.
/// Variant II (x86_64): the thread pointer precedes the TCB; offsets are negative.
pub const TLS_VARIANT_II: u8 = 2;
/// Variant I (aarch64, riscv64): static TLS sits above the TCB.
pub const TLS_VARIANT_I: u8 = 1;
/// Size of the TCB in Variant I (`aarch64`, `riscv64`).
pub const TLS_TCB_SIZE: u64 = 16;
/// TCB alignment.
pub const TLS_TCB_ALIGN: u64 = 16;
/// riscv64 `TLS_DTV_OFFSET` (RISC-V psABI; mlibc `rtld-abi.hpp:22-23`).
pub const TLS_DTV_OFFSET_RISCV: u64 = 0x800;
/// riscv64 `TLS_TP_OFFSET` (RISC-V psABI).
pub const TLS_TP_OFFSET_RISCV: u64 = 0;

// `dlopen` modes and special handles (3.7).
/// `RTLD_LAZY`.
pub const RTLD_LAZY: u32 = 0x1;
/// `RTLD_NOW`.
pub const RTLD_NOW: u32 = 0x2;
/// `RTLD_NOLOAD`.
pub const RTLD_NOLOAD: u32 = 0x4;
/// `RTLD_DEEPBIND`.
pub const RTLD_DEEPBIND: u32 = 0x8;
/// `RTLD_GLOBAL`.
pub const RTLD_GLOBAL: u32 = 0x100;
/// `RTLD_NODELETE`.
pub const RTLD_NODELETE: u32 = 0x1000;
