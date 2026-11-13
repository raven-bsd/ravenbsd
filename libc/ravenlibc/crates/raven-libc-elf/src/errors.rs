// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Error values.
//!
//! Each error is a small `Copy` value with a fixed `Display` string. The strings are ours, not
//! the donor's `printf` text (13 3.6), and `errno()` maps them to the ravenlibc numbers. The
//! loader formats the text into its error buffer; this crate never prints.

use core::fmt;

use raven_libc_abi::errno;
use raven_libc_abi::errno::{E2BIG, EACCES, EFAULT, ENAMETOOLONG, ENOENT, ENOEXEC, ENOMEM, ESRCH};

fn errno_of(value: i32) -> errno::Errno {
    errno::Errno(u16::try_from(value).unwrap_or(0))
}

/// Errors from parsing an ELF header, a program header, a dynamic entry or a table record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElfError {
    /// The `e_ident` magic is not `0x7f 'E' 'L' 'F'`.
    BadMagic,
    /// Not `ELFCLASS64`.
    BadClass,
    /// Not little endian.
    BadEndian,
    /// `e_ident[EI_VERSION]` or `e_version` is not `EV_CURRENT`.
    BadVersion,
    /// A table was shorter than one record.
    Truncated,
    /// `e_phentsize` is not 56.
    BadPhdrSize,
    /// `e_phnum` exceeds [`MAX_PHDRS`](crate::limits::MAX_PHDRS).
    TooManyPhdrs,
    /// `e_type` is neither `ET_EXEC` nor `ET_DYN`.
    UnsupportedType,
    /// The entry offset points outside the program header table.
    BadEntry,
    /// There is no `PT_LOAD` segment.
    NoLoad,
    /// A load segment is malformed (size, alignment, overlap or `W|X`).
    BadSegment,
    /// More than [`MAX_LOAD_SEGMENTS`](crate::limits::MAX_LOAD_SEGMENTS) load segments.
    TooManySegments,
}

impl fmt::Display for ElfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::BadMagic => "not an ELF object",
            Self::BadClass => "not an ELF64 object",
            Self::BadEndian => "not a little-endian object",
            Self::BadVersion => "unsupported ELF version",
            Self::Truncated => "truncated ELF record",
            Self::BadPhdrSize => "bad program header size",
            Self::TooManyPhdrs => "too many program headers",
            Self::UnsupportedType => "unsupported ELF type",
            Self::BadEntry => "bad ELF entry",
            Self::NoLoad => "no PT_LOAD segment",
            Self::BadSegment => "bad load segment",
            Self::TooManySegments => "too many PT_LOAD segments",
        };
        f.write_str(text)
    }
}

impl ElfError {
    /// The C library error number for this failure.
    pub fn errno(self) -> errno::Errno {
        errno_of(ENOEXEC)
    }
}

/// Errors when digesting one object's dynamic section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestError {
    /// There is no `PT_DYNAMIC` segment (a static object).
    NotDynamic,
    /// The dynamic array is malformed (a loop, no `DT_NULL`, a bad `DT_PLTREL`).
    BadDynamic,
    /// A `DT_*` address range is not inside one `PT_LOAD`; the name is the tag.
    RangeOutsideLoad(&'static str),
    /// There is no `DT_SYMTAB`/`DT_STRTAB`.
    NoSymtab,
    /// Neither `DT_HASH` nor `DT_GNU_HASH` is usable.
    NoHash,
    /// `DT_TEXTREL` or `DF_TEXTREL` is set.
    TextRel,
    /// `DT_REL`/`DT_RELSZ`/`DT_RELENT` is present; all three targets use RELA.
    RelNotSupported,
    /// The version tables are malformed.
    BadVersionTable,
    /// More than [`MAX_NEEDED`](crate::limits::MAX_NEEDED) `DT_NEEDED` entries.
    TooManyNeeded,
    /// A string-table offset is not below `DT_STRSZ`.
    BadString,
    /// A relocation or array size is not a multiple of its entry size.
    BadSize,
}

impl fmt::Display for DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotDynamic => f.write_str("not a dynamic object"),
            Self::BadDynamic => f.write_str("bad dynamic section"),
            Self::RangeOutsideLoad(tag) => write!(f, "{tag} outside any PT_LOAD"),
            Self::NoSymtab => f.write_str("no symbol table"),
            Self::NoHash => f.write_str("no hash table"),
            Self::TextRel => f.write_str("text relocations not supported"),
            Self::RelNotSupported => f.write_str("REL relocations not supported"),
            Self::BadVersionTable => f.write_str("bad version table"),
            Self::TooManyNeeded => f.write_str("too many DT_NEEDED entries"),
            Self::BadString => f.write_str("string table offset out of range"),
            Self::BadSize => f.write_str("bad table size"),
        }
    }
}

impl DigestError {
    /// The C library error number for this failure.
    pub fn errno(self) -> errno::Errno {
        match self {
            Self::TooManyNeeded => errno_of(E2BIG),
            _ => errno_of(ENOEXEC),
        }
    }
}

/// Errors while looking a symbol up in one object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupError {
    /// The hash table is inconsistent (a bad bucket, a bloom size that is not a power of two).
    BadTable,
    /// A hash chain walk exceeded its step limit.
    ChainLoop,
    /// A symbol index is outside the dynamic symbol table.
    BadSymbol,
    /// A string-table offset is outside the table.
    BadString,
    /// The version tables are malformed.
    BadVersion,
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::BadTable => "bad hash table",
            Self::ChainLoop => "hash chain loop",
            Self::BadSymbol => "bad symbol index",
            Self::BadString => "bad string table offset",
            Self::BadVersion => "bad version table",
        };
        f.write_str(text)
    }
}

impl LookupError {
    /// The C library error number for this failure.
    pub fn errno(self) -> errno::Errno {
        errno_of(ENOEXEC)
    }
}

/// Errors while checking symbol versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionError {
    /// A required version is missing from the dependency.
    Unsatisfied {
        /// The version name, as a string-table offset in the referencing object.
        version: u32,
        /// The dependency, as a string-table offset in the referencing object.
        library: u32,
        /// The referencing object, as a string-table offset in its own table.
        required_by: u32,
    },
    /// A version table has a bad index, a version other than the current one, or a short record.
    BadTable,
    /// A dependency named by `DT_VERNEED` is not in the object table.
    MissingDependency,
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsatisfied { .. } => f.write_str("unsatisfied symbol version"),
            Self::BadTable => f.write_str("bad version table"),
            Self::MissingDependency => f.write_str("version dependency not loaded"),
        }
    }
}

impl VersionError {
    /// The C library error number for this failure.
    pub fn errno(self) -> errno::Errno {
        match self {
            Self::Unsatisfied { .. } | Self::MissingDependency => errno_of(ENOENT),
            Self::BadTable => errno_of(ENOEXEC),
        }
    }
}

/// Errors while deciding or applying relocations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelocError {
    /// A symbol has no definition.
    Undefined(u32),
    /// A relocation type is not part of the table.
    Unsupported {
        /// The object's architecture.
        arch: crate::elf::Arch,
        /// The refused relocation number.
        r_type: u32,
    },
    /// A `COPY` relocation appeared outside the main program.
    UnexpectedCopy,
    /// An IFUNC definition was referenced by a TLS relocation.
    IfuncForTls,
    /// A relocation offset is outside every `PT_LOAD`.
    BadOffset,
    /// The `DT_RELR` stream is malformed.
    BadRelr,
    /// A relocation list is longer than [`MAX_RELOCS_PER_OBJECT`](crate::limits::MAX_RELOCS_PER_OBJECT).
    TooManyRelocs,
    /// A symbol index is outside the dynamic symbol table.
    BadSymbol,
    /// A relocation that is not valid in a shared object.
    NotInSharedObject,
    /// A phase-specific rule was violated (for example IFUNC work in an earlier phase).
    WrongPhase,
}

impl fmt::Display for RelocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undefined(index) => write!(f, "undefined symbol index {index}"),
            Self::Unsupported { arch, r_type } => {
                write!(f, "unsupported {} relocation type {r_type}", arch.name())
            }
            Self::UnexpectedCopy => f.write_str("unexpected COPY relocation in a shared object"),
            Self::IfuncForTls => f.write_str("IFUNC for TLS relocation"),
            Self::BadOffset => f.write_str("relocation offset outside PT_LOAD"),
            Self::BadRelr => f.write_str("bad DT_RELR stream"),
            Self::TooManyRelocs => f.write_str("too many relocations"),
            Self::BadSymbol => f.write_str("bad symbol index"),
            Self::NotInSharedObject => f.write_str("relocation not valid in a shared object"),
            Self::WrongPhase => f.write_str("relocation in the wrong phase"),
        }
    }
}

impl RelocError {
    /// The C library error number for this failure.
    pub fn errno(self) -> errno::Errno {
        match self {
            Self::Undefined(_) => errno_of(ESRCH),
            Self::TooManyRelocs => errno_of(E2BIG),
            _ => errno_of(ENOEXEC),
        }
    }
}

/// Errors the memory side of a relocation reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// The target address is not writable memory of the object.
    OutOfRange,
    /// The access is not naturally aligned.
    BadAlignment,
    /// A static TLS offset could not be allocated.
    NoTlsSpace,
    /// The IFUNC resolver returned an unusable address.
    BadIfunc,
    /// A copy source or range is unusable.
    BadCopy,
    /// The caller's resolver failed.
    Resolver,
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::OutOfRange => "write outside the object",
            Self::BadAlignment => "misaligned relocation write",
            Self::NoTlsSpace => "No space available for static Thread Local Storage",
            Self::BadIfunc => "bad IFUNC result",
            Self::BadCopy => "bad copy relocation",
            Self::Resolver => "symbol resolver failed",
        };
        f.write_str(text)
    }
}

impl ApplyError {
    /// The C library error number for this failure.
    pub fn errno(self) -> errno::Errno {
        match self {
            Self::OutOfRange | Self::BadAlignment | Self::BadCopy => errno_of(EFAULT),
            Self::NoTlsSpace => errno_of(ENOMEM),
            Self::BadIfunc | Self::Resolver => errno_of(ENOEXEC),
        }
    }
}

/// Errors while expanding a library name into candidate paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchError {
    /// A relative name with a slash in secure mode.
    NotAbsolute,
    /// A search path list or a path is longer than its cap.
    PathTooLong,
    /// More than [`MAX_SEARCH_CANDIDATES`](crate::limits::MAX_SEARCH_CANDIDATES) candidates.
    TooManyCandidates,
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::NotAbsolute => "absolute pathname required for a shared object",
            Self::PathTooLong => "library path too long",
            Self::TooManyCandidates => "too many library search candidates",
        };
        f.write_str(text)
    }
}

impl SearchError {
    /// The C library error number for this failure.
    pub fn errno(self) -> errno::Errno {
        match self {
            Self::NotAbsolute => errno_of(EACCES),
            Self::PathTooLong => errno_of(ENAMETOOLONG),
            Self::TooManyCandidates => errno_of(E2BIG),
        }
    }
}

#[cfg(test)]
#[path = "errors_ut.rs"]
mod tests;
