// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the POSIX filesystem control additions.

use super::*;

fn encoded(request: &CallRequest<'_>) -> Vec<u8> {
    let mut buffer = [0u8; 640];
    let length = request.encode(&mut buffer).expect("encode");
    buffer[..length].to_vec()
}

fn roundtrip(call: Call<'_>) {
    let request = CallRequest::new(call);
    let bytes = encoded(&request);
    let decoded = CallRequest::decode(&bytes).expect("decode");
    assert_eq!(decoded.call, call);
}

#[test]
fn link_round_trips_with_two_paths() {
    roundtrip(Call::Link {
        path: b"new",
        target: b"old",
    });
}

#[test]
fn chown_variants_round_trip() {
    roundtrip(Call::Chown {
        uid: 1000,
        gid: ID_NO_CHANGE,
        path: b"file",
    });
    roundtrip(Call::Lchown {
        uid: ID_NO_CHANGE,
        gid: 100,
        path: b"link",
    });
    roundtrip(Call::Fchown {
        uid: 1000,
        gid: 100,
    });
}

#[test]
fn special_nodes_round_trip() {
    roundtrip(Call::Mkfifo {
        mode: 0o640,
        path: b"fifo",
    });
    roundtrip(Call::Mknod {
        mode: 0o020600,
        dev: 0x1234,
        path: b"null",
    });
}

#[test]
fn utimens_round_trips_every_selector() {
    roundtrip(Call::Utimens {
        path: b"file",
        atime: SetTime::Set(1_700_000_000),
        mtime: SetTime::Set(1_700_000_001),
        follow: true,
    });
    roundtrip(Call::Utimens {
        path: b"file",
        atime: SetTime::Now,
        mtime: SetTime::Omit,
        follow: false,
    });
}

#[test]
fn utimens_flags_are_encoded() {
    let request = CallRequest::new(Call::Utimens {
        path: b"f",
        atime: SetTime::Now,
        mtime: SetTime::Omit,
        follow: false,
    });
    let bytes = encoded(&request);
    let flags = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    assert_eq!(flags, UTIME_NOW | UTIME_MTIME_OMIT | UTIME_NOFOLLOW);
}

#[test]
fn utimens_rejects_unknown_or_conflicting_flags() {
    let mut bytes = encoded(&CallRequest::new(Call::Utimens {
        path: b"f",
        atime: SetTime::Set(1),
        mtime: SetTime::Set(2),
        follow: true,
    }));
    bytes[4..8].copy_from_slice(&(UTIME_NOW | UTIME_OMIT).to_le_bytes());
    assert_eq!(CallRequest::decode(&bytes), Err(ProtoError::Malformed));
    bytes[4..8].copy_from_slice(&(1u32 << 31).to_le_bytes());
    assert_eq!(CallRequest::decode(&bytes), Err(ProtoError::Malformed));
}

#[test]
fn statvfs_response_round_trips() {
    let statvfs = StatVfs {
        block_size: 4096,
        fragment_size: 4096,
        blocks: 100,
        free_blocks: 40,
        available_blocks: 35,
        files: 20,
        free_files: 10,
        available_files: 9,
        fsid: 7,
        flags: 0,
        name_max: 255,
    };
    let response = CallResponse::StatVfs { status: 0, statvfs };
    let mut bytes = [0u8; 128];
    let length = response.encode(&mut bytes).expect("encode");
    assert_eq!(length, 4 + STATVFS_LEN);
    assert_eq!(
        CallResponse::decode(OP_STATVFS, &bytes[..length]),
        Ok(response)
    );
}

#[test]
fn statvfs_rejects_short_payloads() {
    let mut bytes = [0u8; 4 + STATVFS_LEN - 1];
    bytes[..4].copy_from_slice(&0i32.to_le_bytes());
    assert_eq!(
        CallResponse::decode(OP_STATVFS, &bytes),
        Err(ProtoError::Truncated)
    );
}

#[test]
fn mmap_round_trips_with_offset_and_length() {
    roundtrip(Call::Mmap {
        offset: 0x1000,
        len: 0x4000,
        shared: true,
    });
    roundtrip(Call::Mmap {
        offset: 0,
        len: 0x1000,
        shared: false,
    });
}

#[test]
fn mmap_flags_are_limited_to_shared() {
    let mut bytes = encoded(&CallRequest::new(Call::Mmap {
        offset: 0,
        len: 0x1000,
        shared: false,
    }));
    bytes[24..28].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(CallRequest::decode(&bytes), Err(ProtoError::Malformed));
}

#[test]
fn lstat_response_round_trips() {
    let stat = Stat {
        ino: 7,
        size: 3,
        blocks: 1,
        mtime: 1,
        ctime: 2,
        atime: 3,
        mode: 0o120777,
        nlink: 1,
        uid: 0,
        gid: 0,
    };
    let response = CallResponse::Stat { status: 0, stat };
    let mut bytes = [0u8; 128];
    let length = response.encode(&mut bytes).expect("encode");
    assert_eq!(length, 4 + STAT_LEN);
    assert_eq!(
        CallResponse::decode(OP_LSTAT, &bytes[..length]),
        Ok(response)
    );
}
