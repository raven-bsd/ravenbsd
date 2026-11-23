// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `FdMap` inheritance block tests.

use alloc::vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::Platform;
use crate::backend::FakeSys;
use crate::fd::inherit::{
    ENTRY_LEN_V4, ENTRY_LEN_V5, FDMAP_MAGIC, FDMAP_VERSION, FDMAP_VERSION_V1, FDMAP_VERSION_V4,
    FdMap, FdMapEntry, HEADER_LEN, NO_PATH, entry_handle, transferable,
};
use crate::fd::ofd::{OfdKind, SocketEndpoint, SocketMeta};

fn sample() -> FdMap {
    let mut first = FdMapEntry::new(0, 0x10, OfdKind::File, 0, 0);
    first.path = Some(b"/tmp/a".to_vec());
    first.event = 0x30;
    first.readiness = 5;
    let second = FdMapEntry::new(3, 0x11, OfdKind::Dir, 1, 2);
    FdMap {
        entries: vec![first, second],
    }
}

#[test]
fn empty_block_round_trip() {
    let map = FdMap::empty();
    let bytes = map.encode().expect("encode");
    assert_eq!(bytes.len(), HEADER_LEN);
    assert_eq!(&bytes[..4], &FDMAP_MAGIC);
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), FDMAP_VERSION);
    let decoded = FdMap::decode(&bytes).expect("decode");
    assert!(decoded.entries.is_empty());
}

#[test]
fn round_trip_keeps_every_field() {
    let map = sample();
    let bytes = map.encode().expect("encode");
    let decoded = FdMap::decode(&bytes).expect("decode");
    assert_eq!(decoded, map);
}

#[test]
fn pty_master_role_round_trips() {
    let mut entry = FdMapEntry::new(3, 0x61, OfdKind::Tty, 0, 2);
    entry.path = Some(b"/dev/ptmx".to_vec());
    entry.pty_master = true;
    let map = FdMap {
        entries: vec![entry],
    };
    let decoded = FdMap::decode(&map.encode().expect("encode")).expect("decode");
    assert_eq!(decoded, map);
    assert!(
        decoded.entries[0].pty_master,
        "the master role must survive exec"
    );
}

#[test]
fn restore_fdmap_restores_the_pty_master_role() {
    let platform = Platform::new(FakeSys::new());
    let mut entry = FdMapEntry::new(3, 0x62, OfdKind::Tty, 0, 2);
    entry.pty_master = true;
    platform
        .restore_fdmap(&FdMap {
            entries: vec![entry],
        })
        .expect("restore");
    let reference = platform.get(3).expect("fd3");
    assert!(
        reference.is_pty_master(),
        "the rebuilt OFD keeps the master role"
    );
    assert_eq!(reference.kind(), OfdKind::Tty);
}

#[test]
fn socket_metadata_round_trips() {
    let mut entry = FdMapEntry::new(3, 0x44, OfdKind::Socket, 0, 2);
    entry.socket = Some(SocketMeta {
        domain: 1,
        ty: 1,
        proto: 0,
        state: 5,
        local: Some(SocketEndpoint {
            ip: [10, 0, 2, 15],
            port: 22,
        }),
        peer: Some(SocketEndpoint {
            ip: [10, 0, 2, 2],
            port: 40_000,
        }),
    });
    let map = FdMap {
        entries: vec![entry],
    };
    let decoded = FdMap::decode(&map.encode().expect("encode")).expect("decode");
    assert_eq!(decoded, map);
}

#[test]
fn string_table_is_shared() {
    let mut map = sample();
    map.entries[1].path = Some(b"/tmp/a".to_vec());
    let bytes = map.encode().expect("encode");
    // Header + two entries + one NUL-terminated string.
    assert_eq!(bytes.len(), HEADER_LEN + 2 * ENTRY_LEN_V5 + 14);
    let decoded = FdMap::decode(&bytes).expect("decode");
    assert_eq!(
        decoded.entries[1].path.as_deref(),
        Some(b"/tmp/a".as_slice())
    );
}

#[test]
fn no_path_is_marked() {
    let map = FdMap {
        entries: vec![FdMapEntry::new(1, 2, OfdKind::File, 0, 0)],
    };
    let bytes = map.encode().expect("encode");
    let offset = HEADER_LEN + 14;
    assert_eq!(
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3]
        ]),
        NO_PATH
    );
}

#[test]
fn bad_magic_is_eproto() {
    let mut bytes = sample().encode().expect("encode");
    bytes[0] = b'X';
    assert_eq!(FdMap::decode(&bytes), Err(Errno::EPROTO));
}

#[test]
fn bad_version_is_eproto() {
    let mut bytes = sample().encode().expect("encode");
    bytes[4] = 9;
    assert_eq!(FdMap::decode(&bytes), Err(Errno::EPROTO));
}

#[test]
fn truncation_is_eproto() {
    let bytes = sample().encode().expect("encode");
    assert_eq!(FdMap::decode(&bytes[..HEADER_LEN]), Err(Errno::EPROTO));
    assert_eq!(FdMap::decode(&bytes[..bytes.len() - 1]), Err(Errno::EPROTO));
    assert_eq!(FdMap::decode(&[]), Err(Errno::EPROTO));
}

#[test]
fn too_many_entries_is_e2big() {
    let map = FdMap {
        entries: (0..=u16::MAX as i32)
            .map(|fd| FdMapEntry::new(fd, 1, OfdKind::File, 0, 0))
            .collect(),
    };
    assert_eq!(map.encode(), Err(Errno::E2BIG));
}

#[test]
fn nulted_path_is_einval() {
    let mut map = FdMap::empty();
    let mut entry = FdMapEntry::new(0, 1, OfdKind::File, 0, 0);
    entry.path = Some(vec![b'a', 0, b'b']);
    map.entries.push(entry);
    assert_eq!(map.encode(), Err(Errno::EINVAL));
}

#[test]
fn transferable_kinds() {
    assert!(transferable(OfdKind::File));
    assert!(transferable(OfdKind::Dir));
    assert!(transferable(OfdKind::Tty));
    assert!(transferable(OfdKind::Socket));
    assert!(!transferable(OfdKind::Epoll));
    assert!(!transferable(OfdKind::Timer));
    assert!(!transferable(OfdKind::ProcessFd));
}

#[test]
fn entry_handle_wraps_the_number() {
    let entry = FdMapEntry::new(0, 0xabcd, OfdKind::File, 0, 0);
    assert_eq!(entry_handle(&entry).0, 0xabcd);
}

#[test]
fn restore_fdmap_shares_one_ofd_per_handle() {
    let platform = Platform::new(FakeSys::new());
    let block = FdMap {
        entries: vec![
            FdMapEntry::new(0, 0x21, OfdKind::File, 0, 2),
            FdMapEntry::new(1, 0x21, OfdKind::File, 0, 2),
        ],
    };
    platform.restore_fdmap(&block).expect("restore");
    let first = platform.get(0).expect("fd0").index();
    let second = platform.get(1).expect("fd1").index();
    assert_eq!(first, second, "fds sharing a handle must share the OFD");
    assert_eq!(platform.ofd_refs(), 2);
    platform.close(0).expect("close fd0");
    assert!(platform.get(1).is_ok(), "fd1 must survive displacing fd0");
    assert_eq!(platform.ofd_refs(), 1);
    // FakeSys does not know the fabricated handle, so only the OFD/table state is asserted.
    let _ = platform.close(1);
    assert!(platform.get(1).is_err());
    assert_eq!(platform.ofd_refs(), 0);
}

#[test]
fn restored_socket_exposes_its_identity() {
    let platform = Platform::new(FakeSys::new());
    let mut entry = FdMapEntry::new(3, 0x51, OfdKind::Socket, 0, 2);
    entry.socket = Some(SocketMeta {
        domain: 1,
        ty: 1,
        proto: 0,
        state: 5,
        local: Some(SocketEndpoint {
            ip: [10, 0, 2, 15],
            port: 22,
        }),
        peer: Some(SocketEndpoint {
            ip: [10, 0, 2, 2],
            port: 40_000,
        }),
    });
    platform
        .restore_fdmap(&FdMap {
            entries: vec![entry],
        })
        .expect("restore");
    let inherited = platform.inherited_sockets();
    assert_eq!(inherited.len(), 1);
    assert_eq!(inherited[0].fd, 3);
    assert_eq!(inherited[0].socket.domain, 1);
    assert_eq!(inherited[0].socket.ty, 1);
    assert_eq!(inherited[0].socket.state, 5);
    assert_eq!(
        inherited[0].socket.peer,
        Some(SocketEndpoint {
            ip: [10, 0, 2, 2],
            port: 40_000,
        })
    );
}

#[test]
fn restored_cloexec_socket_is_closed_before_main() {
    let platform = Platform::new(FakeSys::new());
    let mut entry = FdMapEntry::new(3, 0x52, OfdKind::Socket, 1, 2);
    entry.socket = Some(SocketMeta {
        domain: 1,
        ty: 1,
        proto: 0,
        state: 5,
        local: None,
        peer: None,
    });
    platform
        .restore_fdmap(&FdMap {
            entries: vec![entry],
        })
        .expect("restore");
    assert!(platform.get(3).is_ok());
    platform.close_cloexec_fds().expect("close cloexec");
    assert!(platform.get(3).is_err());
    assert!(platform.inherited_sockets().is_empty());
}

#[test]
fn exec_fdmap_env_carries_the_pty_master_role() {
    let platform = Platform::new(FakeSys::new());
    let path = b"/dev/ptmx".to_vec().into_boxed_slice();
    let fd = platform
        .install(Handle(0x71), OfdKind::Tty, 2, Some(path), false)
        .expect("install");
    platform.get(fd).expect("fd").set_pty_master();
    let env = platform.exec_fdmap_env().expect("env").expect("entry");
    let value = env.split(|byte| *byte == b'=').nth(1).expect("value");
    let block = FdMap::decode_env(value).expect("decode");
    let entry = block
        .entries
        .iter()
        .find(|entry| entry.fd == fd)
        .expect("inherited entry");
    assert!(entry.pty_master, "exec must serialize the master role");
}

#[test]
fn version_four_block_decodes_without_the_pty_role() {
    let map = FdMap {
        entries: vec![FdMapEntry::new(3, 0x63, OfdKind::Tty, 0, 2)],
    };
    let mut bytes = map.encode().expect("encode");
    bytes[4..6].copy_from_slice(&FDMAP_VERSION_V4.to_le_bytes());
    // A version-4 entry is version 5 minus the trailing role byte.
    bytes.remove(HEADER_LEN + ENTRY_LEN_V4);
    let decoded = FdMap::decode(&bytes).expect("v4 decode");
    assert_eq!(decoded.entries.len(), 1);
    assert!(!decoded.entries[0].pty_master);
}

#[test]
fn version_one_block_decodes_without_readiness() {
    let mut bytes = alloc::vec::Vec::new();
    bytes.extend_from_slice(&FDMAP_MAGIC);
    bytes.extend_from_slice(&FDMAP_VERSION_V1.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes());
    bytes.extend_from_slice(&7u32.to_le_bytes());
    bytes.push(0); // File
    bytes.push(0);
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&NO_PATH.to_le_bytes());
    let decoded = FdMap::decode(&bytes).expect("v1 decode");
    assert_eq!(decoded.entries.len(), 1);
    assert_eq!(decoded.entries[0].event, 0);
    assert_eq!(decoded.entries[0].readiness, 0);
}

#[test]
fn env_round_trip_is_lowercase_hex() {
    let map = sample();
    let encoded = map.encode_env().expect("encode");
    assert!(encoded.iter().all(u8::is_ascii_hexdigit));
    assert!(encoded.iter().all(|byte| !byte.is_ascii_uppercase()));
    assert_eq!(FdMap::decode_env(&encoded), Ok(map));
}

#[test]
fn env_rejects_odd_length_and_bad_digits() {
    assert_eq!(FdMap::decode_env(b"abc"), Err(Errno::EINVAL));
    assert_eq!(FdMap::decode_env(b"zz"), Err(Errno::EINVAL));
}

#[test]
fn exec_shell_slave_ofd_refs_reach_zero_after_stdio_close() {
    // K495/R4: an exec'd shell restores the same pty-slave handle on fds 0/1/2. The three fds
    // share one OFD; the child's original slave fd is gone. Closing stdio at child exit must drop
    // the last OFD reference, so the ptyd can see the slave's last close.
    let platform = Platform::new(FakeSys::new());
    let block = FdMap {
        entries: vec![
            FdMapEntry::new(0, 0x5a, OfdKind::Tty, 0, 2),
            FdMapEntry::new(1, 0x5a, OfdKind::Tty, 0, 2),
            FdMapEntry::new(2, 0x5a, OfdKind::Tty, 0, 2),
        ],
    };
    platform.restore_fdmap(&block).expect("restore");
    let ofd = platform.get(0).expect("fd0").index();
    assert_eq!(platform.get(1).expect("fd1").index(), ofd);
    assert_eq!(platform.get(2).expect("fd2").index(), ofd);
    assert_eq!(
        platform.ofd_refs(),
        3,
        "one slave OFD shared by fds 0/1/2 (ofd {ofd}, handle 0x5a)"
    );
    platform.close(0).expect("close fd0");
    platform.close(1).expect("close fd1");
    let _ = platform.close(2);
    assert_eq!(
        platform.ofd_refs(),
        0,
        "slave OFD refs after stdio close (ofd {ofd}, handle 0x5a)"
    );
    assert!(platform.get(3).is_err());
    assert!(platform.get(4).is_err());
}
