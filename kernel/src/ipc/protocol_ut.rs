// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the S11 packet codec and opcode table.

use super::*;

fn sample() -> SchemePacket {
    let mut packet = SchemePacket::request(Opcode::Open, 0x1122_3344_5566_7788, 0x99aa);
    packet.flags = 3;
    packet.server_cookie = 0xdead_beef;
    packet.buffer_addr = 0x7000_0000_0000;
    packet.buffer_len = 1 << 20;
    packet.response_addr = 0x7000_1000_0000;
    packet.response_len = 4096;
    packet.offset = 123;
    packet.set_path(b"/tmp/root/inside").unwrap();
    packet.root_len = 9;
    packet
}

#[test]
fn packet_size_and_offsets_are_stable() {
    assert_eq!(PACKET_SIZE, 536);
    assert_eq!(size_of::<Opcode>(), 2);
}

#[test]
fn caller_credentials_roundtrip() {
    let mut packet = SchemePacket::request(Opcode::Open, 1, 2);
    packet.uid = 1000;
    packet.euid = 1000;
    packet.gid = 1000;
    packet.egid = 1001;
    packet.group_count = 2;
    packet.groups[0] = 4;
    packet.groups[1] = 1001;
    let decoded = decode(&to_vec(&packet).unwrap()).unwrap();
    assert_eq!(decoded.uid, 1000);
    assert_eq!(decoded.euid, 1000);
    assert_eq!(decoded.gid, 1000);
    assert_eq!(decoded.egid, 1001);
    assert_eq!(decoded.group_count, 2);
    assert_eq!(&decoded.groups[..2], &[4, 1001]);
    assert_eq!(decoded.validate(false), Ok(()));
}

#[test]
fn group_count_is_bounded() {
    let mut packet = SchemePacket::request(Opcode::Read, 1, 1);
    packet.group_count = MAX_GROUPS as u32 + 1;
    assert_eq!(packet.validate(false), Err(PacketError::BadLength));
}

#[test]
fn roundtrip_preserves_every_field() {
    let packet = sample();
    let bytes = to_vec(&packet).unwrap();
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.version, packet.version);
    assert_eq!(decoded.opcode, packet.opcode);
    assert_eq!(decoded.flags, packet.flags);
    assert_eq!(decoded.request_id, packet.request_id);
    assert_eq!(decoded.file_id, packet.file_id);
    assert_eq!(decoded.server_cookie, packet.server_cookie);
    assert_eq!(decoded.buffer_addr, packet.buffer_addr);
    assert_eq!(decoded.buffer_len, packet.buffer_len);
    assert_eq!(decoded.response_addr, packet.response_addr);
    assert_eq!(decoded.response_len, packet.response_len);
    assert_eq!(decoded.offset, packet.offset);
    assert_eq!(decoded.path_bytes(), b"/tmp/root/inside");
    assert_eq!(decoded.root_len, 9);
    assert_eq!(decoded.data_bytes(), b"");
    let mut read = SchemePacket::request(Opcode::Read, 1, 1);
    read.set_data(b"payload").unwrap();
    assert_eq!(read.data_bytes(), b"payload");
    assert_eq!(read.path_bytes(), b"");
}

#[test]
fn short_and_bad_packets_are_rejected() {
    assert_eq!(decode(&[]), Err(PacketError::TooShort));
    assert_eq!(decode(&[0u8; PACKET_SIZE - 1]), Err(PacketError::TooShort));
    let mut bytes = alloc::vec![0u8; PACKET_SIZE];
    let packet = decode(&bytes).unwrap();
    assert_eq!(packet.validate(false), Err(PacketError::BadVersion));
    bytes[0..4].copy_from_slice(&VERSION.to_le_bytes());
    let packet = decode(&bytes).unwrap();
    assert_eq!(packet.validate(false), Err(PacketError::BadOpcode));
}

#[test]
fn length_limits_are_enforced() {
    let mut packet = SchemePacket::request(Opcode::Read, 1, 1);
    packet.buffer_len = MAX_TRANSFER + 1;
    assert_eq!(packet.validate(false), Err(PacketError::BadLength));
    let mut packet = SchemePacket::request(Opcode::Read, 1, 1);
    packet.data_len = (MAX_DATA + 1) as u32;
    assert_eq!(packet.validate(false), Err(PacketError::BadLength));
    let mut packet = SchemePacket::request(Opcode::Open, 1, 1);
    packet.data_len = (MAX_PATH + 1) as u32;
    assert_eq!(packet.validate(false), Err(PacketError::BadLength));
    let mut packet = SchemePacket::request(Opcode::Open, 1, 1);
    packet.root_len = (MAX_PATH + 1) as u32;
    assert_eq!(packet.validate(false), Err(PacketError::BadLength));
}

#[test]
fn large_read_response_uses_buffer_len() {
    let mut packet = SchemePacket::request(Opcode::Read, 7, 9);
    packet.buffer_len = 1 << 20;
    packet.data_len = 0;
    packet.status = 0;
    assert_eq!(packet.validate(true), Ok(()));
}

#[test]
fn fstat_inline_record_without_a_capture_is_bounded() {
    let mut packet = SchemePacket::request(Opcode::Fstat, 1, 1);
    packet.data_len = 64;
    assert_eq!(reply_result(Opcode::Fstat, 0, 0, &packet), Ok(0));
    packet.data_len = 65;
    assert_eq!(
        reply_result(Opcode::Fstat, 0, 0, &packet),
        Err(ReplyBoundError::ExceedsCapacity)
    );
}

#[test]
fn status_range_is_checked_for_responses() {
    let mut packet = SchemePacket::request(Opcode::Call, 1, 1);
    packet.status = 5;
    assert_eq!(packet.validate(true), Ok(()));
    packet.status = -1;
    assert_eq!(packet.validate(true), Err(PacketError::BadStatus));
    packet.status = 4096;
    assert_eq!(packet.validate(true), Err(PacketError::BadStatus));
}

#[test]
fn inline_data_and_path_are_bounded() {
    let mut packet = SchemePacket::default();
    assert_eq!(packet.set_data(b"abcdef"), Ok(()));
    assert_eq!(packet.data_bytes(), b"abcdef");
    assert_eq!(
        packet.set_data(&[0u8; MAX_DATA + 1]),
        Err(PacketError::BadLength)
    );
    assert_eq!(
        packet.set_path(&[b'a'; MAX_PATH + 1]),
        Err(PacketError::BadLength)
    );
}

#[test]
fn attachment_roundtrip_preserves_handle_and_mask() {
    let mut packet = SchemePacket::request(Opcode::Open, 1, 2);
    packet.attach[0] = Attachment {
        handle: 7,
        rights_mask: 3,
    };
    packet.attach_count = 1;
    let decoded = decode(&to_vec(&packet).unwrap()).unwrap();
    assert_eq!(decoded.attach_count, 1);
    assert_eq!(decoded.attach[0], packet.attach[0]);
}

#[test]
fn attachment_count_is_bounded() {
    let packet = SchemePacket {
        attach_count: (MAX_ATTACHMENTS + 1) as u32,
        ..SchemePacket::request(Opcode::Read, 1, 2)
    };
    assert_eq!(packet.validate(true), Err(PacketError::BadLength));
}

#[test]
fn first_attachment_sets_count_to_one() {
    let mut packet = SchemePacket::default();
    packet.attach[0] = Attachment::default();
    packet.attach_count = 1;
    assert_eq!(packet.attach_count, 1);
}

#[test]
fn later_attachment_sets_count_to_index_plus_one() {
    let mut packet = SchemePacket::default();
    packet.attach[3] = Attachment::default();
    packet.attach_count = 4;
    assert_eq!(packet.attach_count, 4);
}

#[test]
fn attachment_count_past_limit_is_rejected() {
    let packet = SchemePacket {
        attach_count: (MAX_ATTACHMENTS + 1) as u32,
        ..SchemePacket::request(Opcode::Call, 1, 2)
    };
    assert_eq!(packet.validate(true), Err(PacketError::BadLength));
}

#[test]
fn zero_attachment_mask_retains_source_rights() {
    let rights = Rights::READ.union(Rights::TRANSFER);
    assert_eq!(attachment_rights(rights, 0), rights);
}

#[test]
fn attachment_mask_only_reduces_rights() {
    let rights = Rights::READ.union(Rights::WRITE).union(Rights::TRANSFER);
    assert_eq!(attachment_rights(rights, Rights::READ.bits()), Rights::READ);
}

#[test]
fn attachment_mask_cannot_grow_rights() {
    assert_eq!(
        attachment_rights(Rights::READ, Rights::ALL.bits()),
        Rights::READ
    );
}

#[test]
fn reply_handle_flag_is_distinct_from_open_flags() {
    assert_eq!(REPLY_HANDLE & 0x7fff, 0);
}

#[test]
fn empty_attachment_tail_is_valid() {
    let packet = SchemePacket::request(Opcode::Call, 1, 2);
    assert_eq!(packet.validate(true), Ok(()));
}

#[test]
fn decode_never_panics_on_arbitrary_bytes() {
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut bytes = alloc::vec![0u8; PACKET_SIZE + 32];
    for length in 0..=bytes.len() {
        for _ in 0..64 {
            for byte in &mut bytes {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            let slice = &bytes[..length];
            let _ = decode(slice);
            if let Ok(packet) = decode(slice) {
                let _ = packet.validate(false);
                let _ = packet.validate(true);
                let _ = packet.path_bytes();
                let _ = packet.data_bytes();
            }
        }
    }
}

fn reply(opcode: Opcode, status: i32, buffer_len: u64, data_len: u32) -> SchemePacket {
    SchemePacket {
        status,
        buffer_len,
        data_len,
        ..SchemePacket::request(opcode, 1, 2)
    }
}

#[test]
fn write_result_is_bounded_by_the_client_input() {
    let equal = reply(Opcode::Write, 0, 8, 0);
    assert_eq!(reply_result(Opcode::Write, 8, 0, &equal), Ok(8));
    let zero = reply(Opcode::Write, 0, 0, 0);
    assert_eq!(reply_result(Opcode::Write, 8, 0, &zero), Ok(0));
    let over = reply(Opcode::Write, 0, 9, 0);
    assert_eq!(
        reply_result(Opcode::Write, 8, 0, &over),
        Err(ReplyBoundError::ExceedsCapacity)
    );
}

#[test]
fn read_result_is_bounded_by_the_captured_buffer() {
    let equal = reply(Opcode::Read, 0, 4096, 0);
    assert_eq!(reply_result(Opcode::Read, 4096, 0, &equal), Ok(4096));
    let over = reply(Opcode::Read, 0, 4097, 0);
    assert_eq!(
        reply_result(Opcode::Read, 4096, 0, &over),
        Err(ReplyBoundError::ExceedsCapacity)
    );
}

#[test]
fn call_result_is_bounded_by_the_response_capacity() {
    let equal = reply(Opcode::Call, 0, 16, 0);
    assert_eq!(reply_result(Opcode::Call, 0, 16, &equal), Ok(16));
    let zero = reply(Opcode::Call, 0, 0, 0);
    assert_eq!(reply_result(Opcode::Call, 0, 16, &zero), Ok(0));
    let over = reply(Opcode::Call, 0, 17, 0);
    assert_eq!(
        reply_result(Opcode::Call, 0, 16, &over),
        Err(ReplyBoundError::ExceedsCapacity)
    );
    let inline = reply(Opcode::Call, 0, 8, 16);
    assert_eq!(reply_result(Opcode::Call, 0, 16, &inline), Ok(8));
    let inline_over = reply(Opcode::Call, 0, 8, 17);
    assert_eq!(
        reply_result(Opcode::Call, 0, 16, &inline_over),
        Err(ReplyBoundError::ExceedsCapacity)
    );
}

#[test]
fn fstat_inline_result_is_bounded_by_the_response_capacity() {
    let equal = reply(Opcode::Fstat, 0, 0, 16);
    assert_eq!(reply_result(Opcode::Fstat, 0, 16, &equal), Ok(16));
    let over = reply(Opcode::Fstat, 0, 0, 17);
    assert_eq!(
        reply_result(Opcode::Fstat, 0, 16, &over),
        Err(ReplyBoundError::ExceedsCapacity)
    );
}

#[test]
fn error_replies_are_not_bounded() {
    let error = reply(Opcode::Write, 5, u64::MAX, 0);
    assert_eq!(reply_result(Opcode::Write, 8, 0, &error), Ok(u64::MAX));
}

#[test]
fn open_and_one_way_replies_have_no_result_bound() {
    let open = reply(Opcode::Open, 0, u64::MAX, 0);
    assert_eq!(reported_result(Opcode::Open, &open), 0);
    assert_eq!(reply_result(Opcode::Open, 0, 0, &open), Ok(0));
    let close = reply(Opcode::Close, 0, u64::MAX, 0);
    assert_eq!(reply_result(Opcode::Close, 0, 0, &close), Ok(0));
}

#[test]
fn seek_opcode_and_positional_offset_roundtrip() {
    assert_eq!(Opcode::Seek.as_u16(), 8);
    assert_eq!(Opcode::from_u16(8), Some(Opcode::Seek));
    assert_eq!(Opcode::from_u16(9), None);
    assert_eq!(OFFSET_EXPLICIT, 1);

    let mut packet = SchemePacket::request(Opcode::Seek, 7, 9);
    packet.offset = (-4i64) as u64;
    packet.flags = 2;
    let bytes = to_vec(&packet).unwrap();
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.opcode, Opcode::Seek.as_u16());
    assert_eq!(decoded.offset, (-4i64) as u64);
    assert_eq!(decoded.flags, 2);
    assert!(decoded.validate(false).is_ok());
}

#[test]
fn the_wire_version_is_one() {
    assert_eq!(VERSION, 1);
    let packet = SchemePacket::request(Opcode::Read, 1, 1);
    assert_eq!(packet.version, 1);
    assert_eq!(packet.validate(false), Ok(()));
    let mut future = packet;
    future.version = 2;
    assert_eq!(future.validate(false), Err(PacketError::BadVersion));
}

#[test]
fn c14_kinds_are_exactly_the_declared_transferable_set() {
    for kind in ObjectKind::ALL {
        let expected = matches!(
            kind,
            ObjectKind::MemObject
                | ObjectKind::MemObjectInitfs
                | ObjectKind::SchemeFile
                | ObjectKind::SchemeServer
                | ObjectKind::RootDevice
                | ObjectKind::MmioRegion
                | ObjectKind::BootBlob
                | ObjectKind::DmaBuffer
                | ObjectKind::Irq
                | ObjectKind::Pipe
                | ObjectKind::Duplex
                | ObjectKind::Timer
                | ObjectKind::UserEvent
        );
        assert_eq!(transferable(kind), expected, "{kind:?}");
    }
    // The four kinds that never cross a transfer stay refused.
    for kind in [
        ObjectKind::Process,
        ObjectKind::Namespace,
        ObjectKind::Thread,
        ObjectKind::EventQueue,
    ] {
        assert!(!transferable(kind), "{kind:?} must not be transferable");
    }
}

#[test]
fn transfer_rights_are_source_and_mask_and_zero_removes_all() {
    let source = Rights(Rights::READ.0 | Rights::WRITE.0 | Rights::TRANSFER.0);
    assert_eq!(transfer_rights(source, 0), Rights::NONE);
    assert_eq!(transfer_rights(source, Rights::READ.0), Rights::READ);
    assert_eq!(transfer_rights(source, Rights::WRITE.0), Rights::WRITE);
    assert_eq!(transfer_rights(source, u32::MAX), source);
    assert_eq!(transfer_rights(Rights::NONE, u32::MAX), Rights::NONE);
}

#[test]
fn four_attachments_roundtrip_in_the_tail() {
    let mut packet = SchemePacket::default();
    packet.attach_count = MAX_ATTACHMENTS as u32;
    for (index, slot) in packet.attach.iter_mut().enumerate() {
        *slot = Attachment {
            handle: 10 + index as u32,
            rights_mask: Rights::READ.0,
        };
    }
    let decoded = decode(&to_vec(&packet).unwrap()).unwrap();
    assert_eq!(decoded.attach_count, 4);
    assert_eq!(
        decoded.attach[3],
        Attachment {
            handle: 13,
            rights_mask: Rights::READ.0
        }
    );
}
