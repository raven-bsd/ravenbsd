// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Directory module tests.

#[test]
fn getcwd_reexport_matches_the_implementation() {
    let mut buffer = [0u8; 4];
    let result = crate::dir::getcwd(b"/x", Some(&mut buffer), 4);
    assert_eq!(result, Ok(None));
    assert_eq!(&buffer[..3], b"/x\0");
}

/// The public `struct dirent` is the FreeBSD profile at the S700 pin: d_fileno at 0, d_namlen
/// at 20, d_name at 24, 280 bytes. The numbers are written from `freebsd:sys/sys/dirent.h`.
#[test]
fn public_dirent_record_is_the_freebsd_profile() {
    use raven_libc_abi::structs::Dirent;

    assert_eq!(core::mem::size_of::<Dirent>(), 280);
    assert_eq!(core::mem::align_of::<Dirent>(), 8);
    assert_eq!(core::mem::offset_of!(Dirent, d_fileno), 0);
    assert_eq!(core::mem::offset_of!(Dirent, d_off), 8);
    assert_eq!(core::mem::offset_of!(Dirent, d_reclen), 16);
    assert_eq!(core::mem::offset_of!(Dirent, d_type), 18);
    assert_eq!(core::mem::offset_of!(Dirent, d_pad0), 19);
    assert_eq!(core::mem::offset_of!(Dirent, d_namlen), 20);
    assert_eq!(core::mem::offset_of!(Dirent, d_pad1), 22);
    assert_eq!(core::mem::offset_of!(Dirent, d_name), 24);
}

/// FreeBSD `_GENERIC_DIRLEN`: header + name + NUL, rounded up to eight bytes.
#[test]
fn dirent_reclen_is_the_donor_generic_dirlen() {
    use crate::dir::{DIRENT_MAX_LEN, DIRENT_MIN_LEN, dirent_reclen};

    assert_eq!(dirent_reclen(1), 32);
    assert_eq!(dirent_reclen(2), 32);
    assert_eq!(dirent_reclen(8), 40);
    assert_eq!(dirent_reclen(9), 40);
    assert_eq!(dirent_reclen(255), 280);
    assert_eq!(dirent_reclen(1), DIRENT_MIN_LEN);
    assert_eq!(dirent_reclen(255), DIRENT_MAX_LEN);
}

fn k60(name: &[u8], ino: u64, kind: u8) -> alloc::vec::Vec<u8> {
    let mut buffer = alloc::vec![0u8; raven_proto_fs::dirent::record_len(name.len()).max(24)];
    let entry = raven_proto_fs::DirEntry {
        header: raven_proto_fs::DirEntryHeader {
            ino,
            kind,
            name_len: name.len() as u8,
            pad: [0; 6],
        },
        name,
    };
    let written = entry.encode(&mut buffer).expect("encode");
    buffer.truncate(written);
    buffer
}

#[test]
fn encode_dirents_writes_the_public_record() {
    use crate::dir::{DIRENT_NAME_OFFSET, encode_dirents};

    let raw = k60(b"a", 42, raven_proto_fs::flags::DT_REG);
    let mut out = [0u8; 32];
    assert_eq!(encode_dirents(&raw, &mut out), Ok(32));
    assert_eq!(u64::from_le_bytes(out[0..8].try_into().unwrap()), 42);
    assert_eq!(i64::from_le_bytes(out[8..16].try_into().unwrap()), 0);
    assert_eq!(u16::from_le_bytes(out[16..18].try_into().unwrap()), 32);
    assert_eq!(out[18], raven_proto_fs::flags::DT_REG);
    assert_eq!(out[19], 0);
    assert_eq!(u16::from_le_bytes(out[20..22].try_into().unwrap()), 1);
    assert_eq!(out[22..24], [0, 0]);
    assert_eq!(&out[DIRENT_NAME_OFFSET..DIRENT_NAME_OFFSET + 2], b"a\0");
    assert!(out[26..].iter().all(|byte| *byte == 0));
}

#[test]
fn encode_dirents_packs_several_entries_and_keeps_byte_names() {
    use crate::dir::encode_dirents;

    let mut raw = k60(&[0xff, 0xfe], 7, raven_proto_fs::flags::DT_REG);
    let second = k60(b"longer-name", 8, raven_proto_fs::flags::DT_DIR);
    raw.extend_from_slice(&second);
    let mut out = [0u8; 80];
    assert_eq!(encode_dirents(&raw, &mut out), Ok(32 + 40));
    assert_eq!(u16::from_le_bytes(out[20..22].try_into().unwrap()), 2);
    assert_eq!(out[24..26], [0xff, 0xfe]);
    assert_eq!(
        u64::from_le_bytes(out[32..40].try_into().unwrap()),
        8,
        "second record starts at its own offset"
    );
    assert_eq!(u16::from_le_bytes(out[48..50].try_into().unwrap()), 40);
    assert_eq!(u16::from_le_bytes(out[52..54].try_into().unwrap()), 11);
}

#[test]
fn a_maximum_name_occupies_the_maximum_record() {
    use crate::dir::{DIRENT_MAX_LEN, encode_dirents};

    let name = [b'n'; 255];
    let raw = k60(&name, 9, raven_proto_fs::flags::DT_REG);
    assert_eq!(raw.len(), 272);
    let mut out = [0u8; DIRENT_MAX_LEN];
    assert_eq!(encode_dirents(&raw, &mut out), Ok(280));
    assert_eq!(u16::from_le_bytes(out[20..22].try_into().unwrap()), 255);
    assert_eq!(out[279], 0);
}

#[test]
fn a_short_buffer_is_eio_and_an_empty_stream_is_zero() {
    use crate::dir::encode_dirents;

    let raw = k60(b"abc", 1, raven_proto_fs::flags::DT_REG);
    let mut short = [0u8; 24];
    assert_eq!(
        encode_dirents(&raw, &mut short),
        Err(raven_abi::errno::Errno::EIO)
    );
    let mut out = [0u8; 32];
    assert_eq!(encode_dirents(&[], &mut out), Ok(0));
}

#[test]
fn a_malformed_private_record_is_eproto() {
    use crate::dir::encode_dirents;

    let mut raw = k60(b"abc", 1, raven_proto_fs::flags::DT_REG);
    raw[10] = 1; // header pad must be zero
    let mut out = [0u8; 32];
    assert_eq!(
        encode_dirents(&raw, &mut out),
        Err(raven_abi::errno::Errno::EPROTO)
    );
}
