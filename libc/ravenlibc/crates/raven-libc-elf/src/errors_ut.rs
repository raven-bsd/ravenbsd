// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Golden `Display` strings and errno mappings.

use super::*;
use alloc::format;

#[test]
fn the_digest_strings_are_golden() {
    assert_eq!(DigestError::NotDynamic.to_string(), "not a dynamic object");
    assert_eq!(DigestError::BadDynamic.to_string(), "bad dynamic section");
    assert_eq!(
        DigestError::RangeOutsideLoad("DT_STRTAB").to_string(),
        "DT_STRTAB outside any PT_LOAD"
    );
    assert_eq!(DigestError::NoSymtab.to_string(), "no symbol table");
    assert_eq!(DigestError::NoHash.to_string(), "no hash table");
    assert_eq!(
        DigestError::TextRel.to_string(),
        "text relocations not supported"
    );
    assert_eq!(
        DigestError::RelNotSupported.to_string(),
        "REL relocations not supported"
    );
    assert_eq!(
        DigestError::BadVersionTable.to_string(),
        "bad version table"
    );
    assert_eq!(
        DigestError::TooManyNeeded.to_string(),
        "too many DT_NEEDED entries"
    );
    assert_eq!(
        DigestError::BadString.to_string(),
        "string table offset out of range"
    );
    assert_eq!(DigestError::BadSize.to_string(), "bad table size");
}

#[test]
fn the_elf_strings_are_golden() {
    assert_eq!(ElfError::BadMagic.to_string(), "not an ELF object");
    assert_eq!(ElfError::BadClass.to_string(), "not an ELF64 object");
    assert_eq!(
        ElfError::BadEndian.to_string(),
        "not a little-endian object"
    );
    assert_eq!(ElfError::BadVersion.to_string(), "unsupported ELF version");
    assert_eq!(ElfError::Truncated.to_string(), "truncated ELF record");
    assert_eq!(ElfError::BadPhdrSize.to_string(), "bad program header size");
    assert_eq!(
        ElfError::TooManyPhdrs.to_string(),
        "too many program headers"
    );
    assert_eq!(
        ElfError::UnsupportedType.to_string(),
        "unsupported ELF type"
    );
    assert_eq!(ElfError::BadEntry.to_string(), "bad ELF entry");
}

#[test]
fn the_lookup_and_version_strings_are_golden() {
    assert_eq!(LookupError::BadTable.to_string(), "bad hash table");
    assert_eq!(LookupError::ChainLoop.to_string(), "hash chain loop");
    assert_eq!(LookupError::BadSymbol.to_string(), "bad symbol index");
    assert_eq!(
        LookupError::BadString.to_string(),
        "bad string table offset"
    );
    assert_eq!(LookupError::BadVersion.to_string(), "bad version table");
    assert_eq!(
        VersionError::Unsatisfied {
            version: 0,
            library: 0,
            required_by: 0
        }
        .to_string(),
        "unsatisfied symbol version"
    );
    assert_eq!(VersionError::BadTable.to_string(), "bad version table");
    assert_eq!(
        VersionError::MissingDependency.to_string(),
        "version dependency not loaded"
    );
}

#[test]
fn the_reloc_strings_are_golden() {
    assert_eq!(
        RelocError::Undefined(7).to_string(),
        "undefined symbol index 7"
    );
    assert_eq!(
        RelocError::Unsupported {
            arch: crate::elf::Arch::X86_64,
            r_type: 99
        }
        .to_string(),
        "unsupported x86_64 relocation type 99"
    );
    assert_eq!(
        RelocError::UnexpectedCopy.to_string(),
        "unexpected COPY relocation in a shared object"
    );
    assert_eq!(
        RelocError::IfuncForTls.to_string(),
        "IFUNC for TLS relocation"
    );
    assert_eq!(
        RelocError::BadOffset.to_string(),
        "relocation offset outside PT_LOAD"
    );
    assert_eq!(RelocError::BadRelr.to_string(), "bad DT_RELR stream");
    assert_eq!(
        RelocError::TooManyRelocs.to_string(),
        "too many relocations"
    );
    assert_eq!(RelocError::BadSymbol.to_string(), "bad symbol index");
    assert_eq!(
        RelocError::NotInSharedObject.to_string(),
        "relocation not valid in a shared object"
    );
    assert_eq!(
        RelocError::WrongPhase.to_string(),
        "relocation in the wrong phase"
    );
}

#[test]
fn the_apply_strings_are_golden() {
    assert_eq!(
        ApplyError::OutOfRange.to_string(),
        "write outside the object"
    );
    assert_eq!(
        ApplyError::BadAlignment.to_string(),
        "misaligned relocation write"
    );
    assert_eq!(
        ApplyError::NoTlsSpace.to_string(),
        "No space available for static Thread Local Storage"
    );
    assert_eq!(ApplyError::BadIfunc.to_string(), "bad IFUNC result");
    assert_eq!(ApplyError::BadCopy.to_string(), "bad copy relocation");
    assert_eq!(ApplyError::Resolver.to_string(), "symbol resolver failed");
}

#[test]
fn the_search_strings_are_golden() {
    assert_eq!(
        SearchError::NotAbsolute.to_string(),
        "absolute pathname required for a shared object"
    );
    assert_eq!(
        SearchError::PathTooLong.to_string(),
        "library path too long"
    );
    assert_eq!(
        SearchError::TooManyCandidates.to_string(),
        "too many library search candidates"
    );
}

#[test]
fn every_error_maps_to_a_real_errno() {
    let values = [
        DigestError::TextRel.errno(),
        DigestError::TooManyNeeded.errno(),
        ElfError::BadMagic.errno(),
        LookupError::BadTable.errno(),
        VersionError::MissingDependency.errno(),
        RelocError::Undefined(1).errno(),
        RelocError::TooManyRelocs.errno(),
        ApplyError::NoTlsSpace.errno(),
        SearchError::NotAbsolute.errno(),
        SearchError::PathTooLong.errno(),
    ];
    for value in values {
        assert!(value.is_linux() || value.is_bsd_extra(), "{value:?}");
    }
    assert_eq!(ApplyError::NoTlsSpace.errno().name(), Some("ENOMEM"));
    assert_eq!(RelocError::Undefined(1).errno().name(), Some("ESRCH"));
    assert_eq!(SearchError::NotAbsolute.errno().name(), Some("EACCES"));
}

#[test]
fn formatting_a_range_error_names_the_tag() {
    let text = format!("{}", DigestError::RangeOutsideLoad("DT_RELA"));
    assert!(text.contains("DT_RELA"));
    assert!(text.ends_with("outside any PT_LOAD"));
}
