// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the server loop, dispatch and deferred replies.

use crate::testing::FakeKernel;
use crate::{CallReply, Ctx, Handler, HandlerError, HandlerResult, Opened, Packet, Server};
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::time::Duration;
use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_ipc::protocol::{self, Attachment, Opcode, PACKET_SIZE, REPLY_HANDLE, SchemePacket};
use raven_proto_fs::Stat;

#[derive(Default)]
struct TestHandler {
    calls: Vec<String>,
    stat: Stat,
    defer: bool,
    error: Option<Errno>,
    attach: Option<Handle>,
    token: Option<u64>,
}

impl Handler for TestHandler {
    fn open(&mut self, path: &str, flags: u32, _mode: u32, ctx: &Ctx) -> HandlerResult<Opened> {
        self.calls.push(format!("open {path} {flags}"));
        if self.defer {
            self.token = Some(ctx.token);
            return Err(HandlerError::Defer(ctx.token));
        }
        if let Some(error) = self.error {
            return Err(error.into());
        }
        match self.attach {
            Some(handle) => Ok(Opened::Attach(handle)),
            None => Ok(Opened::File(0x1234)),
        }
    }

    fn read(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        self.calls.push(format!("read {file} {offset}"));
        if self.defer {
            let token = self.token.unwrap_or(77);
            self.token = Some(token);
            return Err(HandlerError::Defer(token));
        }
        if let Some(error) = self.error {
            return Err(error.into());
        }
        let data = b"hello";
        let length = data.len().min(buffer.len());
        buffer[..length].copy_from_slice(&data[..length]);
        Ok(length)
    }

    fn write(&mut self, file: u64, offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
        self.calls
            .push(format!("write {file} {offset} {}", buffer.len()));
        if let Some(error) = self.error {
            return Err(error.into());
        }
        Ok(buffer.len())
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        self.calls.push(format!("close {file}"));
        self.error.map_or(Ok(()), |error| Err(error.into()))
    }

    fn dup(&mut self, file: u64) -> HandlerResult<u64> {
        self.calls.push(format!("dup {file}"));
        self.error.map_or(Ok(0x5678), |error| Err(error.into()))
    }

    fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
        self.calls.push(format!("fstat {file}"));
        self.error.map_or(Ok(self.stat), |error| Err(error.into()))
    }

    fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
        self.calls.push(format!("call {file} {}", request.len()));
        if let Some(error) = self.error {
            return Err(error.into());
        }
        let data = b"reply";
        let length = data.len().min(response.len());
        response[..length].copy_from_slice(&data[..length]);
        Ok(length)
    }
}

fn open_packet(path: &str) -> SchemePacket {
    let mut packet = SchemePacket::request(Opcode::Open, 1, 7);
    packet.flags = raven_proto_fs::WIRE_OPEN_READ;
    packet.set_path(path.as_bytes()).unwrap();
    packet
}

fn request(opcode: Opcode) -> SchemePacket {
    SchemePacket::request(opcode, 2, 7)
}

fn decode_sent(kernel: &FakeKernel) -> Vec<SchemePacket> {
    kernel
        .sent()
        .iter()
        .map(|bytes| protocol::decode(bytes).unwrap())
        .collect()
}

#[test]
fn register_records_the_name() {
    let kernel = FakeKernel::new();
    let _server = Server::register_with(kernel.clone(), "echo").unwrap();
    assert_eq!(kernel.registered(), vec!["echo".to_string()]);
}

#[test]
fn open_file_returns_a_server_cookie() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let packet = Packet {
        handle: Handle(9),
        packet: open_packet("hello"),
    };
    server.dispatch(packet, &mut handler).unwrap();
    let sent = decode_sent(&kernel);
    assert_eq!(sent[0].status, 0);
    assert_eq!(sent[0].server_cookie, 0x1234);
    assert_eq!(handler.calls, vec!["open hello 1"]);
    assert!(kernel.sent_attachments().is_empty());
}

#[test]
fn open_attach_uses_send_attach() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "file").unwrap();
    let mut handler = TestHandler {
        attach: Some(Handle(0x44)),
        ..TestHandler::default()
    };
    let packet = Packet {
        handle: Handle(9),
        packet: open_packet("hello"),
    };
    server.dispatch(packet, &mut handler).unwrap();
    let sent = kernel.sent_attachments();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, Handle(9));
    assert_eq!(
        sent[0].2,
        vec![Attachment {
            handle: 0x44,
            rights_mask: 0,
        }]
    );
    let response = protocol::decode(&sent[0].1).unwrap();
    assert_eq!(response.flags & REPLY_HANDLE, REPLY_HANDLE);
    assert!(kernel.sent().is_empty());
}

#[test]
fn read_copies_into_the_capture() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let mut capture = [0u8; 16];
    let mut packet = request(Opcode::Read);
    packet.file_id = 0x33;
    packet.offset = 12;
    packet.buffer_addr = capture.as_mut_ptr() as u64;
    packet.buffer_len = capture.len() as u64;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(&capture[..5], b"hello");
    assert_eq!(decode_sent(&kernel)[0].buffer_len, 5);
    assert_eq!(handler.calls, vec!["read 51 12"]);
}

#[test]
fn write_passes_the_capture() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let data = b"payload";
    let mut packet = request(Opcode::Write);
    packet.offset = 3;
    packet.buffer_addr = data.as_ptr() as u64;
    packet.buffer_len = data.len() as u64;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(handler.calls, vec!["write 7 3 7"]);
    assert_eq!(decode_sent(&kernel)[0].buffer_len, 7);
}

#[test]
fn close_calls_the_handler() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let mut packet = request(Opcode::Close);
    packet.file_id = 0x21;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(handler.calls, vec!["close 33"]);
    assert_eq!(decode_sent(&kernel)[0].status, 0);
}

#[test]
fn dup_returns_a_new_cookie() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: request(Opcode::Dup),
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].server_cookie, 0x5678);
}

#[test]
fn fstat_encodes_the_64_byte_record() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let handler_stat = Stat {
        ino: 7,
        size: 42,
        blocks: 1,
        mtime: 1,
        ctime: 2,
        atime: 3,
        mode: 0o100644,
        nlink: 1,
        uid: 0,
        gid: 0,
    };
    let mut handler = TestHandler {
        stat: handler_stat,
        ..TestHandler::default()
    };
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: request(Opcode::Fstat),
            },
            &mut handler,
        )
        .unwrap();
    let response = decode_sent(&kernel)[0];
    assert_eq!(response.data_len, 64);
    assert_eq!(Stat::decode(response.data_bytes()).unwrap(), handler_stat);
}

#[test]
fn call_uses_both_captures() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let request_bytes = b"request";
    let mut response_bytes = [0u8; 16];
    let mut packet = request(Opcode::Call);
    packet.buffer_addr = request_bytes.as_ptr() as u64;
    packet.buffer_len = request_bytes.len() as u64;
    packet.response_addr = response_bytes.as_mut_ptr() as u64;
    packet.response_len = response_bytes.len() as u64;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(&response_bytes[..5], b"reply");
    assert_eq!(decode_sent(&kernel)[0].buffer_len, 5);
    assert_eq!(handler.calls, vec!["call 7 7"]);
}

#[test]
fn call_attach_carries_data_and_two_handles() {
    struct AttachCallHandler;

    impl Handler for AttachCallHandler {
        fn call_attach(
            &mut self,
            _file: u64,
            _request: &[u8],
            _response: &mut [u8],
        ) -> HandlerResult<CallReply> {
            let mut reply = CallReply::with_data(b"ok")?;
            reply.attach(Handle(0x51), 0)?;
            reply.attach(Handle(0x52), 0x11)?;
            Ok(reply)
        }
    }

    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = AttachCallHandler;
    let request_bytes = b"attach";
    let mut response_bytes = [0u8; 128];
    let mut packet = request(Opcode::Call);
    packet.buffer_addr = request_bytes.as_ptr() as u64;
    packet.buffer_len = request_bytes.len() as u64;
    packet.response_addr = response_bytes.as_mut_ptr() as u64;
    packet.response_len = response_bytes.len() as u64;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    let sent = kernel.sent_attachments();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].2,
        vec![
            Attachment {
                handle: 0x51,
                rights_mask: 0,
            },
            Attachment {
                handle: 0x52,
                rights_mask: 0x11,
            },
        ]
    );
    let response = protocol::decode(&sent[0].1).unwrap();
    assert_eq!(response.data_len, 2);
    assert_eq!(response.data_bytes(), b"ok");
    assert!(kernel.sent().is_empty());
}

#[test]
fn handler_default_is_enosys() {
    struct Empty;
    impl Handler for Empty {}
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: open_packet("x"),
            },
            &mut Empty,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].status, i32::from(Errno::ENOSYS.0));
}

#[test]
fn handler_error_becomes_a_positive_status() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler {
        error: Some(Errno::EIO),
        ..TestHandler::default()
    };
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: open_packet("hello"),
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].status, i32::from(Errno::EIO.0));
}

#[test]
fn unknown_opcode_is_a_protocol_error() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let mut packet = request(Opcode::Read);
    packet.opcode = 99;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].status, i32::from(Errno::EPROTO.0));
}

#[test]
fn poll_returns_none_when_the_fake_is_empty() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel, "echo").unwrap();
    assert!(server.poll(Some(Duration::ZERO)).is_none());
}

#[test]
fn poll_returns_a_queued_request() {
    let kernel = FakeKernel::new();
    let mut bytes = [0u8; PACKET_SIZE];
    protocol::encode(&open_packet("x"), &mut bytes).unwrap();
    kernel.push_request(&bytes);
    let mut server = Server::register_with(kernel, "echo").unwrap();
    let packet = server.poll(Some(Duration::ZERO)).unwrap();
    assert_eq!(packet.packet.path_bytes(), b"x");
}

#[test]
fn deferred_reply_is_sent_by_the_completer() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler {
        defer: true,
        ..TestHandler::default()
    };
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: request(Opcode::Read),
            },
            &mut handler,
        )
        .unwrap();
    assert!(kernel.sent().is_empty());
    let token = handler.token.unwrap();
    server.completer().complete(token, Ok(4)).unwrap();
    assert_eq!(decode_sent(&kernel)[0].status, 0);
    assert_eq!(decode_sent(&kernel)[0].buffer_len, 4);
}

#[test]
fn deferred_error_sets_the_status() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler {
        defer: true,
        ..TestHandler::default()
    };
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: open_packet("x"),
            },
            &mut handler,
        )
        .unwrap();
    let token = handler.token.unwrap();
    server
        .completer()
        .complete(token, Err(Errno::EAGAIN))
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].status, i32::from(Errno::EAGAIN.0));
}

#[test]
fn deferred_completion_rejects_unknown_and_repeated_tokens() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    assert_eq!(server.completer().complete(99, Ok(1)), Err(Errno::EINVAL));
    let mut handler = TestHandler {
        defer: true,
        ..TestHandler::default()
    };
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: open_packet("x"),
            },
            &mut handler,
        )
        .unwrap();
    let token = handler.token.unwrap();
    let completer = server.completer();
    completer.complete(token, Ok(1)).unwrap();
    assert_eq!(completer.complete(token, Ok(1)), Err(Errno::EINVAL));
}

#[test]
fn completer_is_cloneable() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler {
        defer: true,
        ..TestHandler::default()
    };
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: open_packet("x"),
            },
            &mut handler,
        )
        .unwrap();
    let token = handler.token.unwrap();
    let first = server.completer();
    let second = first.clone();
    second.complete(token, Ok(2)).unwrap();
    assert_eq!(decode_sent(&kernel)[0].buffer_len, 2);
}

#[test]
fn read_with_zero_buffer_is_supported() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let mut packet = request(Opcode::Read);
    packet.buffer_len = 0;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].buffer_len, 0);
}

#[test]
fn call_without_response_buffer_is_supported() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let request_bytes = b"req";
    let mut packet = request(Opcode::Call);
    packet.buffer_addr = request_bytes.as_ptr() as u64;
    packet.buffer_len = request_bytes.len() as u64;
    packet.response_len = 0;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].buffer_len, 0);
}

#[test]
fn close_error_is_reported() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler {
        error: Some(Errno::EIO),
        ..TestHandler::default()
    };
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: request(Opcode::Close),
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].status, i32::from(Errno::EIO.0));
}

#[test]
fn dup_error_is_reported() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler {
        error: Some(Errno::EBADF),
        ..TestHandler::default()
    };
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: request(Opcode::Dup),
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].status, i32::from(Errno::EBADF.0));
}

#[test]
fn fstat_error_is_reported() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler {
        error: Some(Errno::EIO),
        ..TestHandler::default()
    };
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: request(Opcode::Fstat),
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].status, i32::from(Errno::EIO.0));
}

#[test]
fn call_error_is_reported() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler {
        error: Some(Errno::ENOSYS),
        ..TestHandler::default()
    };
    let mut response = [0u8; 4];
    let mut packet = request(Opcode::Call);
    packet.response_addr = response.as_mut_ptr() as u64;
    packet.response_len = response.len() as u64;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].status, i32::from(Errno::ENOSYS.0));
}

#[test]
fn malformed_open_path_is_einval() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let mut packet = open_packet("ok");
    packet.path[0] = 0xff;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].status, i32::from(Errno::EINVAL.0));
}

#[test]
fn open_flags_are_forwarded() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let mut packet = open_packet("x");
    packet.flags = 3;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(handler.calls, vec!["open x 3"]);
}

#[test]
fn open_validates_wire_flags_before_the_handler() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    let mut packet = open_packet("x");
    packet.flags = 0x0100;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert!(handler.calls.is_empty());
    assert_eq!(decode_sent(&kernel)[0].status, i32::from(Errno::EINVAL.0));

    let mut packet = open_packet("x");
    packet.flags = 0;
    server
        .dispatch(
            Packet {
                handle: Handle(10),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert!(handler.calls.is_empty());
    assert_eq!(decode_sent(&kernel)[1].status, i32::from(Errno::EINVAL.0));
}

#[test]
fn open_with_receives_typed_flags_and_create_mode() {
    struct TypedHandler {
        seen: Option<(u16, u32)>,
    }

    impl Handler for TypedHandler {
        fn open_with(
            &mut self,
            _path: &str,
            flags: raven_proto_fs::OpenFlags,
            mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            self.seen = Some((flags.to_wire(), mode));
            Ok(Opened::File(1))
        }
    }

    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel, "echo").unwrap();
    let mut handler = TypedHandler { seen: None };
    let mut packet = open_packet("created");
    packet.flags = raven_proto_fs::WIRE_OPEN_READ
        | raven_proto_fs::WIRE_OPEN_CREATE
        | raven_proto_fs::WIRE_OPEN_EXCL;
    let expected = packet.flags;
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(handler.seen, Some((expected, 0o644)));
}

#[test]
fn handler_stat_default_is_zero() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut handler = TestHandler::default();
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: request(Opcode::Fstat),
            },
            &mut handler,
        )
        .unwrap();
    assert_eq!(decode_sent(&kernel)[0].data_len, 64);
}

#[test]
fn packet_decoding_rejects_short_input_in_poll() {
    let kernel = FakeKernel::new();
    kernel.push_request(&[0u8; 3]);
    let mut server = Server::register_with(kernel, "echo").unwrap();
    assert!(server.poll(Some(Duration::ZERO)).is_none());
}

#[test]
fn server_completer_has_a_fresh_token_per_request() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "echo").unwrap();
    let mut first = TestHandler {
        defer: true,
        ..TestHandler::default()
    };
    let mut second = TestHandler {
        defer: true,
        ..TestHandler::default()
    };
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet: open_packet("a"),
            },
            &mut first,
        )
        .unwrap();
    server
        .dispatch(
            Packet {
                handle: Handle(10),
                packet: open_packet("b"),
            },
            &mut second,
        )
        .unwrap();
    assert_ne!(first.token, second.token);
}
