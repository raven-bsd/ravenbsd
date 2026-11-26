// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `SOL_MPTCP` encoding tests.

use super::*;
use crate::ErrnoExt;

#[test]
fn linux_constants_are_stable() {
    assert_eq!(SOL_MPTCP, 284);
    assert_eq!(IPPROTO_MPTCP, 262);
    assert_eq!(MPTCP_INFO, 1);
    assert_eq!(MPTCP_SUBFLOW_ADDRS, 2);
    assert_eq!(OP_MPTCP_ADD_ADDR, 12);
    assert_eq!(OP_MPTCP_DEL_ADDR, 13);
    assert_eq!(OP_MPTCP_SET_FLAGS, 14);
    assert_eq!(OP_MPTCP_INFO, 15);
    assert_eq!(OP_MPTCP_SUBFLOWS, 16);
}

#[test]
fn pm_addr_roundtrip() {
    let value = PmAddr {
        family: 2,
        id: 4,
        flags: MPTCP_PM_ADDR_FLAG_BACKUP,
        addr: [10, 77, 2, 16],
        port: 0x3412,
    };
    let mut bytes = [0u8; PM_ADDR_LEN];
    assert_eq!(value.encode(&mut bytes), Ok(PM_ADDR_LEN));
    assert_eq!(PmAddr::decode(&bytes), Ok(value));
}

#[test]
fn add_path_request_has_the_mptcp_opcode() {
    let value = PmAddr {
        family: 2,
        id: 7,
        flags: 0,
        addr: [10, 77, 2, 16],
        port: 0,
    };
    let mut out = [0u8; 64];
    let used = Control::AddAddr(value).encode(&mut out).expect("encode");
    assert_eq!(used, 6 + PM_ADDR_LEN);
    assert_eq!(u16::from_le_bytes([out[0], out[1]]), 1);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 4);
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), OP_MPTCP_ADD_ADDR);
    assert_eq!(PmAddr::decode(out.get(6..).expect("payload")), Ok(value));
}

#[test]
fn info_reply_copies_the_c_layout() {
    let info = [2u8, 0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0];
    let mut reply = [0u8; 16];
    reply[0..2].copy_from_slice(&TAG_MPTCP_INFO.to_le_bytes());
    reply[2..4].copy_from_slice(&(info.len() as u16).to_le_bytes());
    reply[4..16].copy_from_slice(&info);
    let mut out = [0u8; INFO_LEN];
    assert_eq!(decode_reply(&reply, MPTCP_INFO, &mut out), Ok(INFO_LEN));
    assert_eq!(out, info);
}

#[test]
fn error_reply_maps_to_errno() {
    // The private reply carries the native error number: 45 is FreeBSD EOPNOTSUPP (the Linux
    // number 95 is not the native domain and must never be produced here).
    let reply = [6u8, 0, 45, 0];
    assert_eq!(
        decode_reply(&reply, MPTCP_INFO, &mut [0u8; 4]),
        Err(Errno::EOPNOTSUPP)
    );
}

#[test]
fn wrong_reply_tag_is_eproto() {
    let reply = [9u8, 0, 0, 0];
    assert_eq!(
        decode_reply(&reply, MPTCP_INFO, &mut [0u8; 4]),
        Err(Errno::EPROTO)
    );
}
