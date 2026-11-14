// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `elf_aux_info` key widths and errors.

use super::*;

#[test]
fn known_keys_have_widths() {
    for key in [
        AT_PHDR, AT_PAGESZ, AT_BASE, AT_ENTRY, AT_HWCAP, AT_RANDOM, AT_HWCAP2, AT_EXECFN,
    ] {
        assert_eq!(width(key), Some(8), "{key}");
    }
    for key in [AT_PHENT, AT_PHNUM, AT_CLKTCK, AT_SECURE] {
        assert_eq!(width(key), Some(4), "{key}");
    }
}

#[test]
fn unknown_key_is_einval() {
    let mut buffer = [0u8; 8];
    assert_eq!(
        elf_aux_info(0, &mut buffer, &|_| Some(1)),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        elf_aux_info(9999, &mut buffer, &|_| Some(1)),
        Err(Errno::EINVAL)
    );
}

#[test]
fn short_buffer_is_einval() {
    let mut buffer = [0u8; 3];
    assert_eq!(
        elf_aux_info(AT_PAGESZ, &mut buffer, &|_| Some(4096)),
        Err(Errno::EINVAL)
    );
}

#[test]
fn missing_key_is_enoent() {
    let mut buffer = [0u8; 8];
    assert_eq!(
        elf_aux_info(AT_PAGESZ, &mut buffer, &|_| None),
        Err(Errno::ENOENT)
    );
}

#[test]
fn eight_byte_value_is_copied() {
    let mut buffer = [0u8; 8];
    assert_eq!(
        elf_aux_info(AT_PAGESZ, &mut buffer, &|_| Some(4096)),
        Ok(())
    );
    assert_eq!(u64::from_ne_bytes(buffer), 4096);
}

#[test]
fn four_byte_value_is_copied() {
    let mut buffer = [0u8; 4];
    assert_eq!(elf_aux_info(AT_CLKTCK, &mut buffer, &|_| Some(100)), Ok(()));
    assert_eq!(u32::from_ne_bytes(buffer), 100);
}
