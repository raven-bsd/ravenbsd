// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the socket request/response wire format.

use super::*;

const LOCAL: Endpoint = Endpoint {
    ip: [10, 0, 0, 1],
    port: 8080,
};

fn encode_request(request: SocketRequest<'_>) -> Vec<u8> {
    let mut buffer = [0u8; 256];
    let length = request.encode(&mut buffer).unwrap();
    buffer[..length].to_vec()
}

fn encode_response(response: Response) -> Vec<u8> {
    let mut buffer = [0u8; 256];
    let length = response.encode(&mut buffer).unwrap();
    buffer[..length].to_vec()
}

#[test]
fn open_golden_bytes() {
    let bytes = encode_request(SocketRequest::Open {
        path: "net:tcp/10.0.0.2:80",
    });
    assert_eq!(
        bytes,
        [
            0x01, 0x00, // version 1
            0x01, 0x00, // OPEN
            0x13, 0x00, // 19-byte path
            b'n', b'e', b't', b':', b't', b'c', b'p', b'/', b'1', b'0', b'.', b'0', b'.', b'0',
            b'.', b'2', b':', b'8', b'0',
        ]
    );
}

#[test]
fn read_golden_bytes() {
    assert_eq!(
        encode_request(SocketRequest::Read { len: 1500 }),
        [0x01, 0x00, 0x02, 0x00, 0xdc, 0x05, 0x00, 0x00]
    );
}

#[test]
fn write_golden_bytes() {
    assert_eq!(
        encode_request(SocketRequest::Write {
            data: b"hi",
            to: None
        }),
        [
            0x01, 0x00, 0x03, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, b'h', b'i'
        ]
    );
}

#[test]
fn write_with_endpoint_golden_bytes() {
    assert_eq!(
        encode_request(SocketRequest::Write {
            data: b"x",
            to: Some(LOCAL)
        }),
        [
            0x01, 0x00, 0x03, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 10, 0, 0, 1, 0x90, 0x1f, b'x'
        ]
    );
}

#[test]
fn bind_and_listen_golden_bytes() {
    assert_eq!(
        encode_request(SocketRequest::Call(Call::Bind(LOCAL))),
        [0x01, 0x00, 0x04, 0x00, 0x01, 0x00, 10, 0, 0, 1, 0x90, 0x1f]
    );
    assert_eq!(
        encode_request(SocketRequest::Call(Call::Listen { backlog: 8 })),
        [0x01, 0x00, 0x04, 0x00, 0x02, 0x00, 0x08, 0x00]
    );
}

#[test]
fn shutdown_and_options_golden_bytes() {
    assert_eq!(
        encode_request(SocketRequest::Call(Call::Shutdown {
            how: Shutdown::Both
        })),
        [0x01, 0x00, 0x04, 0x00, 0x05, 0x00, 0x02]
    );
    assert_eq!(
        encode_request(SocketRequest::Call(Call::SetSockOpt {
            opt: SockOpt::ReuseAddr,
            value: 1
        })),
        [
            0x01, 0x00, 0x04, 0x00, 0x08, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00
        ]
    );
    assert_eq!(
        encode_request(SocketRequest::Call(Call::GetSockOpt {
            opt: SockOpt::Error
        })),
        [0x01, 0x00, 0x04, 0x00, 0x09, 0x00, 0x05, 0x00]
    );
    assert_eq!(
        encode_request(SocketRequest::Call(Call::Event)),
        [0x01, 0x00, 0x04, 0x00, 0x0a, 0x00]
    );
}

#[test]
fn request_roundtrips() {
    for request in [
        SocketRequest::Open {
            path: "net:udp/10.0.0.2:53",
        },
        SocketRequest::Read { len: 0 },
        SocketRequest::Write {
            data: b"payload",
            to: None,
        },
        SocketRequest::Call(Call::Accept),
        SocketRequest::Call(Call::Connect(LOCAL)),
        SocketRequest::Call(Call::GetSockName),
        SocketRequest::Call(Call::GetPeerName),
        SocketRequest::Call(Call::Event),
    ] {
        let bytes = encode_request(request);
        assert_eq!(SocketRequest::decode(&bytes), Ok(request), "{request:?}");
    }
}

#[test]
fn responses_roundtrip() {
    for response in [
        Response::Ok,
        Response::Socket { id: 7 },
        Response::Read {
            len: 10,
            from: Some(LOCAL),
        },
        Response::Read { len: 0, from: None },
        Response::Write { len: 10 },
        Response::Endpoint(LOCAL),
        Response::Value(-1),
        Response::Err(Errno::EAGAIN),
    ] {
        let bytes = encode_response(response);
        assert_eq!(Response::decode(&bytes), Ok(response), "{response:?}");
    }
}

#[test]
fn response_golden_bytes() {
    assert_eq!(
        encode_response(Response::Socket { id: 0x0102_0304 }),
        [0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01]
    );
    assert_eq!(
        encode_response(Response::Read {
            len: 2,
            from: Some(LOCAL)
        }),
        [
            0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 10, 0, 0, 1, 0x90,
            0x1f
        ]
    );
    assert_eq!(
        encode_response(Response::Err(Errno::EAGAIN)),
        [0x01, 0x00, 0x0b, 0x00, 0x06, 0x00]
    );
}

#[test]
fn unknown_version_is_rejected() {
    let mut bytes = encode_request(SocketRequest::Read { len: 1 });
    bytes[0] = 2;
    assert_eq!(SocketRequest::decode(&bytes), Err(DecodeError::version()));
    assert_eq!(Response::decode(&bytes), Err(DecodeError::version()));
}

#[test]
fn unknown_kind_and_tag_are_rejected() {
    let mut bytes = encode_request(SocketRequest::Read { len: 1 });
    bytes[2] = 9;
    assert_eq!(SocketRequest::decode(&bytes), Err(DecodeError::tag()));
    let mut bytes = encode_response(Response::Ok);
    bytes[4] = 9;
    assert_eq!(Response::decode(&bytes), Err(DecodeError::tag()));
}

#[test]
fn truncation_is_rejected_everywhere() {
    let bytes = encode_request(SocketRequest::Call(Call::Bind(LOCAL)));
    for length in 0..bytes.len() {
        assert!(SocketRequest::decode(&bytes[..length]).is_err(), "{length}");
    }
    let bytes = encode_response(Response::Endpoint(LOCAL));
    for length in 0..bytes.len() {
        assert!(Response::decode(&bytes[..length]).is_err(), "{length}");
    }
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut bytes = encode_request(SocketRequest::Read { len: 1 });
    bytes.push(0);
    assert_eq!(SocketRequest::decode(&bytes), Err(DecodeError::trailing()));
    let mut bytes = encode_response(Response::Ok);
    bytes.push(0);
    assert_eq!(Response::decode(&bytes), Err(DecodeError::trailing()));
}

#[test]
fn oversized_lengths_are_rejected() {
    let bytes = [
        0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, // READ 65536
    ];
    assert_eq!(SocketRequest::decode(&bytes), Err(DecodeError::length()));
    let bytes = [
        0x01, 0x00, 0x04, 0x00, 0x02, 0x00, 0x00, 0x00, // LISTEN 0
    ];
    assert_eq!(SocketRequest::decode(&bytes), Err(DecodeError::length()));
}

#[test]
fn read_only_option_cannot_be_set() {
    let mut buffer = [0u8; 32];
    assert_eq!(
        SocketRequest::Call(Call::SetSockOpt {
            opt: SockOpt::Error,
            value: 0
        })
        .encode(&mut buffer),
        Err(WireError::Invalid)
    );
}

#[test]
fn empty_path_is_rejected() {
    let mut buffer = [0u8; 32];
    assert_eq!(
        SocketRequest::Open { path: "" }.encode(&mut buffer),
        Err(WireError::TooLong)
    );
    assert_eq!(
        SocketRequest::Open { path: "file:/x" }.encode(&mut buffer),
        Err(WireError::Invalid)
    );
}

#[test]
fn option_codes_roundtrip() {
    for opt in [
        SockOpt::ReuseAddr,
        SockOpt::RcvBuf,
        SockOpt::SndBuf,
        SockOpt::TcpNoDelay,
        SockOpt::Error,
    ] {
        assert_eq!(SockOpt::from_code(opt.code()), Some(opt));
    }
    assert_eq!(SockOpt::from_code(0), None);
}

#[test]
fn icmp_echo_golden_bytes_and_bounds() {
    assert_eq!(
        encode_request(SocketRequest::IcmpEcho {
            id: 0x1234,
            seq: 7,
            timeout_ms: 2_000,
            data: b"ping",
        }),
        [
            0x01, 0x00, 0x05, 0x00, 0x34, 0x12, 0x07, 0x00, 0xd0, 0x07, 0x00, 0x00, 0x04, 0x00,
            b'p', b'i', b'n', b'g'
        ]
    );
    for timeout_ms in [0, MAX_ICMP_TIMEOUT_MS + 1] {
        let mut encoded = [0u8; 64];
        assert!(
            SocketRequest::IcmpEcho {
                id: 1,
                seq: 1,
                timeout_ms,
                data: b"x",
            }
            .encode(&mut encoded)
            .is_err()
        );
    }
    let mut encoded = [0u8; 8 + MAX_ICMP_PAYLOAD];
    assert!(
        SocketRequest::IcmpEcho {
            id: 1,
            seq: 1,
            timeout_ms: 1000,
            data: &vec![0u8; MAX_ICMP_PAYLOAD + 1],
        }
        .encode(&mut encoded)
        .is_err()
    );
}

#[test]
fn icmp_reply_golden_bytes_and_roundtrip() {
    let reply = Response::Icmp {
        source: Endpoint {
            ip: [127, 0, 0, 1],
            port: 0,
        },
        id: 0x1234,
        seq: 7,
        rtt_ms: 5,
        len: 4,
    };
    assert_eq!(
        encode_response(reply),
        [
            0x01, 0x00, 0x00, 0x00, 0x07, 0x00, 0x7f, 0x00, 0x00, 0x01, 0x00, 0x00, 0x34, 0x12,
            0x07, 0x00, 0x05, 0x00, 0x00, 0x00, 0x04, 0x00
        ]
    );
    assert_eq!(Response::decode(&encode_response(reply)), Ok(reply));
    let request = SocketRequest::IcmpEcho {
        id: 0x1234,
        seq: 7,
        timeout_ms: 2_000,
        data: b"ping",
    };
    let bytes = encode_request(request);
    assert_eq!(SocketRequest::decode(&bytes), Ok(request));
}
