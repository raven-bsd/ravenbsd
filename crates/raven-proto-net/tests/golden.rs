// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Golden bytes taken from the tables in `README.md`.
//!
//! These are the literals a second implementation (netd or a driver) is written against. If a
//! change to the encoder is intentional, the README table and these tests change together.

use raven_proto_net::{
    BUFFER_SIZE, Call, DESC_FIRST, DESC_LAST, Desc, Endpoint, IfName, IfaceRequest, RING_MAGIC,
    RING_VERSION, Response, RingHeader, RingLayout, SockOpt, SocketRequest,
};

#[test]
fn open_message_matches_the_readme() {
    let mut buffer = [0u8; 64];
    let length = SocketRequest::Open {
        path: "net:tcp/10.0.0.2:80",
    }
    .encode(&mut buffer)
    .expect("encode");
    assert_eq!(
        &buffer[..length],
        &[
            0x01, 0x00, // version
            0x01, 0x00, // OPEN
            0x13, 0x00, // 19-byte path
            b'n', b'e', b't', b':', b't', b'c', b'p', b'/', b'1', b'0', b'.', b'0', b'.', b'0',
            b'.', b'2', b':', b'8', b'0',
        ]
    );
}

#[test]
fn udp_write_message_matches_the_readme() {
    let endpoint = Endpoint {
        ip: [192, 0, 2, 7],
        port: 53,
    };
    let mut buffer = [0u8; 64];
    let length = SocketRequest::Write {
        data: b"dns",
        to: Some(endpoint),
    }
    .encode(&mut buffer)
    .expect("encode");
    assert_eq!(
        &buffer[..length],
        &[
            0x01, 0x00, // version
            0x03, 0x00, // WRITE
            0x01, // endpoint present
            0x03, 0x00, 0x00, 0x00, // payload length
            192, 0, 2, 7, 0x35, 0x00, // endpoint
            b'd', b'n', b's',
        ]
    );
}

#[test]
fn set_sockopt_message_matches_the_readme() {
    let mut buffer = [0u8; 64];
    let length = SocketRequest::Call(Call::SetSockOpt {
        opt: SockOpt::TcpNoDelay,
        value: 1,
    })
    .encode(&mut buffer)
    .expect("encode");
    assert_eq!(
        &buffer[..length],
        &[
            0x01, 0x00, // version
            0x04, 0x00, // CALL
            0x08, 0x00, // SETSOCKOPT
            0x04, 0x00, // TCP_NODELAY
            0x01, 0x00, 0x00, 0x00, // value 1
        ]
    );
}

#[test]
fn error_response_matches_the_readme() {
    let mut buffer = [0u8; 32];
    let length = Response::error(raven_abi::errno::Errno::EAGAIN)
        .encode(&mut buffer)
        .expect("encode");
    assert_eq!(&buffer[..length], &[0x01, 0x00, 0x0b, 0x00, 0x06, 0x00]);
}

#[test]
fn interface_set_address_matches_the_readme() {
    let mut buffer = [0u8; 64];
    let length = IfaceRequest::SetAddress {
        name: IfName::new("eth0").expect("name"),
        addr: [10, 0, 0, 2],
        prefix: 24,
    }
    .encode(&mut buffer)
    .expect("encode");
    assert_eq!(
        &buffer[..length],
        &[
            0x01, 0x00, // version
            0x03, 0x00, // SET_ADDRESS
            0x04, b'e', b't', b'h', b'0', // name
            10, 0, 0, 2,  // address
            24, // netmask length
        ]
    );
}

#[test]
fn ring_header_matches_the_readme() {
    let layout = RingLayout::new(2048 + 8 * BUFFER_SIZE, 4, 4).expect("layout");
    let mut region = vec![0u8; layout.region_len()];
    layout
        .header(0x0102_0304_0506_0708, 0x1112_1314_1516_1718)
        .write_to(&mut region)
        .expect("encode");
    assert_eq!(
        &region[..56],
        &[
            b'R', b'A', b'V', b'E', b'N', b'N', b'E', b'T', // magic
            0x01, 0x00, // version
            0x38, 0x00, // header length 56
            0x04, 0x00, // rx count
            0x04, 0x00, // tx count
            0x00, 0x00, 0x00, 0x00, // rx_prod
            0x00, 0x00, 0x00, 0x00, // rx_cons
            0x00, 0x00, 0x00, 0x00, // tx_prod
            0x00, 0x00, 0x00, 0x00, // tx_cons
            0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // rx doorbell
            0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11, // tx doorbell
            0x00, 0x08, 0x00, 0x00, // pool offset 2048
            0x00, 0x40, 0x00, 0x00, // pool length 16384
        ]
    );
    let header = RingHeader::read_from(&region).expect("read");
    assert_eq!(header.magic, RING_MAGIC);
    assert_eq!(header.version, RING_VERSION);
    assert_eq!(header.rx_doorbell, 0x0102_0304_0506_0708);
}

#[test]
fn descriptor_matches_the_readme() {
    let desc = Desc::new(2048, 1514, DESC_FIRST | DESC_LAST);
    let mut buffer = [0u8; 16];
    desc.write_to(&mut buffer).expect("encode");
    assert_eq!(
        &buffer[..],
        &[
            0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // addr 2048
            0xea, 0x05, 0x00, 0x00, // len 1514
            0x03, 0x00, 0x00, 0x00, // FIRST | LAST
        ]
    );
    assert_eq!(Desc::read_from(&buffer), Some(desc));
}
