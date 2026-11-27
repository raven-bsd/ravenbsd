// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the POSIX filesystem control additions and the wire encoding.

use super::*;

fn encoded(request: &CallRequest<'_>) -> Vec<u8> {
    let mut buffer = [0u8; 640];
    let length = request.encode(&mut buffer).expect("encode");
    buffer[..length].to_vec()
}

#[test]
fn mkdir_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Mkdir {
            mode: 0o755,
            path: b"hello"
        })),
        [
            0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0xed, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x05, 0x00, b'h', b'e', b'l', b'l', b'o'
        ]
    );
}

#[test]
fn unlink_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Unlink { path: b"x" })),
        [
            0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, b'x'
        ]
    );
}

#[test]
fn rmdir_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Rmdir { path: b"d" })),
        [
            0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, b'd'
        ]
    );
}

#[test]
fn rename_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Rename {
            from: b"a",
            to: b"b"
        })),
        [
            0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, b'a', 0x01, 0x00, b'b'
        ]
    );
}

#[test]
fn truncate_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Truncate {
            size: 0x0000_0001_0000_0002
        })),
        [
            0x01, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x00, 0x00
        ]
    );
}

#[test]
fn symlink_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Symlink {
            path: b"l",
            target: b"t"
        })),
        [
            0x01, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, b'l', 0x01, 0x00, b't'
        ]
    );
}

#[test]
fn readlink_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Readlink { path: b"l" })),
        [
            0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, b'l'
        ]
    );
}

#[test]
fn chmod_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Chmod {
            mode: 0o600,
            path: b"f"
        })),
        [
            0x01, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, b'f'
        ]
    );
}

#[test]
fn sync_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Sync)),
        [
            0x01, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00
        ]
    );
}

#[test]
fn mount_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Mount {
            path: b"/tmp",
            scheme: b"ramfs:"
        })),
        [
            0x01, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x04, 0x00, b'/', b't', b'm', b'p', 0x06, 0x00, b'r', b'a', b'm', b'f',
            b's', b':'
        ]
    );
}

#[test]
fn umount_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Umount { path: b"/tmp" })),
        [
            0x01, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x04, 0x00, b'/', b't', b'm', b'p'
        ]
    );
}

#[test]
fn statfs_golden_bytes() {
    assert_eq!(
        encoded(&CallRequest::new(Call::Statfs)),
        [
            0x01, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00
        ]
    );
}

#[test]
fn every_call_roundtrips() {
    let calls = [
        Call::Mkdir {
            mode: 0o755,
            path: b"hello",
        },
        Call::Unlink { path: b"x" },
        Call::Rmdir { path: b"d" },
        Call::Rename {
            from: b"a",
            to: b"b",
        },
        Call::Truncate {
            size: 0x1234_5678_9abc_def0,
        },
        Call::Symlink {
            path: b"l",
            target: b"t",
        },
        Call::Readlink { path: b"l" },
        Call::Chmod {
            mode: 0o600,
            path: b"f",
        },
        Call::Sync,
        Call::Mount {
            path: b"/tmp",
            scheme: b"ramfs:",
        },
        Call::Umount { path: b"/tmp" },
        Call::Statfs,
        Call::Access {
            mode: 1,
            path: b"f",
        },
    ];
    for call in calls {
        let request = CallRequest {
            flags: 0x1122_3344,
            call,
        };
        let bytes = encoded(&request);
        assert_eq!(CallRequest::decode(&bytes).unwrap(), request);
    }
}

#[test]
fn truncated_request_is_rejected() {
    let bytes = encoded(&CallRequest::new(Call::Sync));
    assert_eq!(
        CallRequest::decode(&bytes[..CALL_HEADER_LEN - 1]),
        Err(ProtoError::Truncated)
    );
}

#[test]
fn trailing_request_bytes_are_rejected() {
    let mut bytes = encoded(&CallRequest::new(Call::Sync));
    bytes.push(0);
    assert_eq!(CallRequest::decode(&bytes), Err(ProtoError::TrailingBytes));
}

#[test]
fn unknown_version_is_rejected() {
    let mut bytes = encoded(&CallRequest::new(Call::Sync));
    bytes[0] = 2;
    assert_eq!(
        CallRequest::decode(&bytes),
        Err(ProtoError::UnsupportedVersion)
    );
}

#[test]
fn unknown_operation_is_rejected() {
    let mut bytes = [0u8; CALL_HEADER_LEN];
    bytes[0] = 1;
    bytes[2] = 99;
    assert_eq!(CallRequest::decode(&bytes), Err(ProtoError::UnknownOp(99)));
}

#[test]
fn empty_and_oversized_paths_are_rejected() {
    let mut empty = encoded(&CallRequest::new(Call::Unlink { path: b"x" }));
    empty[CALL_HEADER_LEN] = 0;
    empty[CALL_HEADER_LEN + 1] = 0;
    assert_eq!(
        CallRequest::decode(&empty),
        Err(ProtoError::LengthOutOfRange)
    );

    let mut oversized = encoded(&CallRequest::new(Call::Unlink { path: b"x" }));
    oversized[CALL_HEADER_LEN] = 0x00;
    oversized[CALL_HEADER_LEN + 1] = 0x10;
    assert_eq!(
        CallRequest::decode(&oversized),
        Err(ProtoError::LengthOutOfRange)
    );
}

#[test]
fn path_of_256_bytes_is_accepted() {
    let path = vec![b'a'; 256];
    let request = CallRequest::new(Call::Unlink { path: &path });
    let mut bytes = vec![0u8; CALL_HEADER_LEN + 2 + path.len()];
    let length = request.encode(&mut bytes).expect("encode");
    assert_eq!(
        CallRequest::decode(&bytes[..length]).expect("decode").call,
        Call::Unlink { path: &path }
    );
}

#[test]
fn nul_path_byte_is_rejected() {
    let mut nul = encoded(&CallRequest::new(Call::Unlink { path: b"x" }));
    nul[CALL_HEADER_LEN + 2] = 0;
    assert_eq!(CallRequest::decode(&nul), Err(ProtoError::Malformed));
}

#[test]
fn encoding_rejects_invalid_paths() {
    let mut buffer = [0u8; 128];
    assert_eq!(
        CallRequest::new(Call::Unlink { path: b"" }).encode(&mut buffer),
        Err(ProtoError::LengthOutOfRange)
    );
    assert_eq!(
        CallRequest::new(Call::Unlink { path: b"a\0b" }).encode(&mut buffer),
        Err(ProtoError::Malformed)
    );
}

#[test]
fn response_status_golden_bytes() {
    let mut buffer = [0u8; 8];
    let length = CallResponse::Status(-2).encode(&mut buffer).unwrap();
    assert_eq!(&buffer[..length], &[0xfe, 0xff, 0xff, 0xff]);
}

#[test]
fn readlink_response_golden_bytes() {
    let mut buffer = [0u8; 16];
    let response = CallResponse::Readlink {
        status: 0,
        target: b"abc",
    };
    let length = response.encode(&mut buffer).unwrap();
    assert_eq!(
        &buffer[..length],
        &[0x00, 0x00, 0x00, 0x00, 0x03, 0x00, b'a', b'b', b'c']
    );
}

#[test]
fn statfs_response_golden_bytes() {
    let mut buffer = [0u8; 48];
    let response = CallResponse::StatFs {
        status: 0,
        statfs: StatFs {
            block_size: 4096,
            blocks: 10,
            free_blocks: 5,
            files: 7,
            free_files: 3,
        },
    };
    let length = response.encode(&mut buffer).unwrap();
    assert_eq!(length, 40);
    assert_eq!(
        &buffer[..length],
        &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
}

#[test]
fn responses_roundtrip() {
    let responses = [
        CallResponse::Status(0),
        CallResponse::Status(-2),
        CallResponse::Readlink {
            status: 0,
            target: b"target",
        },
        CallResponse::StatFs {
            status: 0,
            statfs: StatFs {
                block_size: 4096,
                blocks: 1,
                free_blocks: 2,
                files: 3,
                free_files: 4,
            },
        },
    ];
    for response in responses {
        let op = match response {
            CallResponse::Readlink { .. } => OP_READLINK,
            CallResponse::StatFs { .. } => OP_STATFS,
            CallResponse::StatVfs { .. } => OP_STATVFS,
            CallResponse::Stat { .. } => OP_LSTAT,
            CallResponse::Status(_) => OP_SYNC,
        };
        let mut buffer = [0u8; 64];
        let length = response.encode(&mut buffer).unwrap();
        assert_eq!(
            CallResponse::decode(op, &buffer[..length]).unwrap(),
            response
        );
    }
}

#[test]
fn negative_status_has_no_payload() {
    let mut buffer = [0u8; 8];
    let length = CallResponse::Status(-2).encode(&mut buffer).unwrap();
    assert_eq!(
        CallResponse::decode(OP_READLINK, &buffer[..length]).unwrap(),
        CallResponse::Status(-2)
    );
}

#[test]
fn success_readlink_requires_a_target() {
    let bytes = [0u8; 4];
    assert_eq!(
        CallResponse::decode(OP_READLINK, &bytes),
        Err(ProtoError::Truncated)
    );
}

#[test]
fn success_statfs_requires_a_record() {
    let bytes = [0u8; 4];
    assert_eq!(
        CallResponse::decode(OP_STATFS, &bytes),
        Err(ProtoError::Truncated)
    );
}

#[test]
fn response_status_range_is_checked() {
    let mut buffer = [0u8; 8];
    assert_eq!(
        CallResponse::Status(1).encode(&mut buffer),
        Err(ProtoError::BadStatus)
    );
    assert_eq!(
        CallResponse::decode(OP_SYNC, &[0x00, 0xf0, 0xff, 0xff]),
        Err(ProtoError::BadStatus)
    );
}

#[test]
fn response_trailing_bytes_are_rejected() {
    let bytes = [0u8; 5];
    assert_eq!(
        CallResponse::decode(OP_SYNC, &bytes),
        Err(ProtoError::TrailingBytes)
    );
}

#[test]
fn statfs_record_roundtrips() {
    let record = StatFs {
        block_size: 4096,
        blocks: 10,
        free_blocks: 5,
        files: 7,
        free_files: 3,
    };
    let mut bytes = [0u8; STATFS_LEN];
    record.encode(&mut bytes).unwrap();
    assert_eq!(StatFs::decode(&bytes).unwrap(), record);
    assert_eq!(StatFs::decode(&bytes[..35]), Err(ProtoError::Truncated));
    assert_eq!(
        StatFs::decode(&[0u8; STATFS_LEN + 1]),
        Err(ProtoError::TrailingBytes)
    );
}

#[test]
fn path_count_table_is_complete() {
    for op in [
        OP_MKDIR,
        OP_UNLINK,
        OP_RMDIR,
        OP_RENAME,
        OP_TRUNCATE,
        OP_SYMLINK,
        OP_READLINK,
        OP_CHMOD,
        OP_SYNC,
        OP_MOUNT,
        OP_UMOUNT,
        OP_STATFS,
        OP_ACCESS,
    ] {
        assert!(path_count(op).is_some());
    }
    assert_eq!(path_count(0), None);
    assert_eq!(path_count(99), None);
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
fn public_path_limit_is_4096_including_nul() {
    assert_eq!(crate::PATH_MAX, 4096);
    assert_eq!(MAX_PATH, crate::PATH_MAX - 1);
}

#[test]
fn path_at_the_public_limit_round_trips() {
    let path = vec![b'a'; 4095];
    let request = CallRequest::new(Call::Unlink { path: &path });
    let mut bytes = vec![0u8; CALL_HEADER_LEN + 2 + path.len()];
    let length = request.encode(&mut bytes).expect("encode");
    assert_eq!(length, bytes.len());
    assert_eq!(
        CallRequest::decode(&bytes).expect("decode").call,
        Call::Unlink { path: &path }
    );
}

#[test]
fn path_over_the_public_limit_is_rejected() {
    let path = vec![b'a'; 4096];
    let mut bytes = vec![0u8; CALL_HEADER_LEN + 2 + path.len()];
    assert_eq!(
        CallRequest::new(Call::Unlink { path: &path }).encode(&mut bytes),
        Err(ProtoError::LengthOutOfRange)
    );
}

#[test]
fn non_utf8_path_bytes_are_preserved() {
    let path = b"\xff\xfe/a\x80";
    let request = CallRequest::new(Call::Mkdir { mode: 0o755, path });
    let mut bytes = [0u8; 64];
    let length = request.encode(&mut bytes).expect("encode");
    let decoded = CallRequest::decode(&bytes[..length]).expect("decode");
    assert_eq!(decoded.call, Call::Mkdir { mode: 0o755, path });
}

#[test]
fn embedded_nul_is_rejected() {
    let path = b"a\0b";
    let mut bytes = [0u8; 64];
    assert_eq!(
        CallRequest::new(Call::Unlink { path }).encode(&mut bytes),
        Err(ProtoError::Malformed)
    );
}

#[test]
fn length_prefix_overflow_is_rejected() {
    let mut bytes = [0u8; CALL_HEADER_LEN + 2];
    bytes[0..2].copy_from_slice(&crate::VERSION.to_le_bytes());
    bytes[2..4].copy_from_slice(&OP_UNLINK.to_le_bytes());
    bytes[16..18].copy_from_slice(&u16::MAX.to_le_bytes());
    assert_eq!(
        CallRequest::decode(&bytes),
        Err(ProtoError::LengthOutOfRange)
    );
}

#[test]
fn readlink_target_uses_the_public_limit() {
    let target = vec![b't'; 4095];
    let response = CallResponse::Readlink {
        status: 0,
        target: &target,
    };
    let mut bytes = vec![0u8; 4 + 2 + target.len()];
    let length = response.encode(&mut bytes).expect("encode");
    assert_eq!(
        CallResponse::decode(OP_READLINK, &bytes[..length]),
        Ok(response)
    );
    let over = vec![b't'; 4096];
    let mut bytes = vec![0u8; 4 + 2 + over.len()];
    assert_eq!(
        CallResponse::Readlink {
            status: 0,
            target: &over
        }
        .encode(&mut bytes),
        Err(ProtoError::LengthOutOfRange)
    );
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
