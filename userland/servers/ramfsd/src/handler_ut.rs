// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the ramfs handler and its S11 dispatch.

use super::*;
use alloc::vec::Vec;
use raven_abi::types::Handle;
use raven_ipc::protocol::{Opcode, SchemePacket};
use raven_scheme::testing::FakeKernel;
use raven_scheme::{HandlerError, Packet, Server};

const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;
const O_RDWR: u32 = 2;
const O_CREAT: u32 = 0o100;
const O_EXCL: u32 = 0o200;
const O_TRUNC: u32 = 0o1000;
const O_APPEND: u32 = 0o2000;
const O_DIRECTORY: u32 = 0o200000;
const O_NOFOLLOW: u32 = 0o400000;

fn flags(linux: u32) -> OpenFlags {
    OpenFlags::from_linux(linux).expect("valid flags")
}

fn ctx() -> Ctx {
    Ctx {
        request: Handle(1),
        token: 1,
        buffer_len: 0,
        response_len: 0,
    }
}

fn open(handler: &mut RamfsHandler, id: u64, path: &str, flags: OpenFlags) -> Result<u64, Errno> {
    handler.expect_open(id, path);
    match handler.open_with(path, flags, 0, &ctx()) {
        Ok(Opened::File(file)) => Ok(file),
        Ok(Opened::Attach(_)) => Err(Errno::EIO),
        Err(HandlerError::Errno(error)) => Err(error),
        Err(HandlerError::Defer(_)) => Err(Errno::EAGAIN),
    }
}

fn read(handler: &mut RamfsHandler, file: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
    handler.read(file, 0, buffer).map_err(handler_error)
}

fn write(handler: &mut RamfsHandler, file: u64, data: &[u8]) -> Result<usize, Errno> {
    handler.write(file, 0, data).map_err(handler_error)
}

fn close(handler: &mut RamfsHandler, file: u64) -> Result<(), Errno> {
    handler.close(file).map_err(handler_error)
}

fn dup(handler: &mut RamfsHandler, file: u64) -> Result<u64, Errno> {
    handler.dup(file).map_err(handler_error)
}

fn handler_error(error: HandlerError) -> Errno {
    match error {
        HandlerError::Errno(error) => error,
        HandlerError::Defer(_) => Errno::EAGAIN,
    }
}

fn call(handler: &mut RamfsHandler, file: u64, request: Call<'_>) -> Result<(u16, Vec<u8>), Errno> {
    let request = CallRequest::new(request);
    let mut encoded = [0u8; 640];
    let length = request.encode(&mut encoded).map_err(|_| Errno::EINVAL)?;
    let mut response = [0u8; 640];
    let written = handler
        .call(file, &encoded[..length], &mut response)
        .map_err(handler_error)?;
    Ok((request.op(), response[..written].to_vec()))
}

fn call_status(handler: &mut RamfsHandler, file: u64, request: Call<'_>) -> Result<(), Errno> {
    let (op, response) = call(handler, file, request)?;
    match CallResponse::decode(op, &response).map_err(|_| Errno::EPROTO)? {
        CallResponse::Status(0) => Ok(()),
        CallResponse::Status(status) => Err(errno_from_status(status)),
        _ => Err(Errno::EPROTO),
    }
}

fn errno_from_status(status: i32) -> Errno {
    if (-4095..0).contains(&status) {
        Errno(u16::try_from(-status).unwrap_or(Errno::EIO.0))
    } else {
        Errno::EIO
    }
}

fn root(handler: &mut RamfsHandler) -> u64 {
    open(handler, 1, "", flags(O_RDONLY)).expect("root")
}

#[test]
fn create_write_read_round_trip() {
    let mut handler = RamfsHandler::new(|| 1000);
    let file = open(&mut handler, 10, "a", flags(O_RDWR | O_CREAT)).expect("open");
    assert_eq!(write(&mut handler, file, b"hello").expect("write"), 5);
    let reader = open(&mut handler, 11, "a", flags(O_RDONLY)).expect("reader");
    let mut data = [0u8; 5];
    assert_eq!(read(&mut handler, reader, &mut data).expect("read"), 5);
    assert_eq!(&data, b"hello");
    assert_eq!(handler.fstat(file).expect("stat").size, 5);
    close(&mut handler, reader).expect("close reader");
    close(&mut handler, file).expect("close");
}

#[test]
fn exclusive_create_fails_the_second_time() {
    let mut handler = RamfsHandler::new(|| 1000);
    let flags = flags(O_RDWR | O_CREAT | O_EXCL);
    let first = open(&mut handler, 10, "x", flags).expect("first");
    close(&mut handler, first).expect("close");
    assert_eq!(open(&mut handler, 11, "x", flags), Err(Errno::EEXIST));
}

#[test]
fn truncate_clears_the_file() {
    let mut handler = RamfsHandler::new(|| 1000);
    let file = open(&mut handler, 10, "t", flags(O_RDWR | O_CREAT)).expect("open");
    write(&mut handler, file, b"data").expect("write");
    let file = open(&mut handler, 11, "t", flags(O_WRONLY | O_TRUNC)).expect("trunc");
    assert_eq!(handler.fstat(file).expect("stat").size, 0);
    close(&mut handler, file).expect("close");
}

#[test]
fn append_from_two_descriptions() {
    let mut handler = RamfsHandler::new(|| 1000);
    let create = flags(O_WRONLY | O_CREAT);
    let created = open(&mut handler, 10, "a", create).expect("create");
    close(&mut handler, created).expect("close create");
    let append = flags(O_WRONLY | O_APPEND);
    let first = open(&mut handler, 12, "a", append).expect("append 1");
    let second = open(&mut handler, 13, "a", append).expect("append 2");
    write(&mut handler, first, b"one").expect("one");
    write(&mut handler, second, b"two").expect("two");
    let reader = open(&mut handler, 14, "a", flags(O_RDONLY)).expect("read");
    let mut data = [0u8; 6];
    assert_eq!(read(&mut handler, reader, &mut data).expect("read"), 6);
    assert_eq!(&data, b"onetwo");
}

#[test]
fn directory_flag_on_a_file_is_enotdir() {
    let mut handler = RamfsHandler::new(|| 1000);
    let file = open(&mut handler, 10, "f", flags(O_RDWR | O_CREAT)).expect("open");
    close(&mut handler, file).expect("close");
    assert_eq!(
        open(&mut handler, 11, "f", flags(O_RDONLY | O_DIRECTORY)),
        Err(Errno::ENOTDIR)
    );
}

#[test]
fn nofollow_rejects_a_final_symlink() {
    let mut handler = RamfsHandler::new(|| 1000);
    let root = root(&mut handler);
    call_status(
        &mut handler,
        root,
        Call::Symlink {
            path: b"link",
            target: b"file",
        },
    )
    .expect("symlink");
    assert_eq!(
        open(&mut handler, 11, "link", flags(O_RDONLY | O_NOFOLLOW)),
        Err(Errno::ELOOP)
    );
    // Intermediate symlinks still follow; not exercised here.
}

#[test]
fn unknown_ids_are_ebadf() {
    let mut handler = RamfsHandler::new(|| 1000);
    assert_eq!(read(&mut handler, 99, &mut [0; 4]), Err(Errno::EBADF));
    assert_eq!(write(&mut handler, 99, b"x"), Err(Errno::EBADF));
    assert_eq!(close(&mut handler, 99), Err(Errno::EBADF));
    assert_eq!(dup(&mut handler, 99), Err(Errno::EBADF));
    assert!(handler.fstat(99).is_err());
    assert!(handler.call(99, &[], &mut [0; 4]).is_err());
}

#[test]
fn dup_shares_the_cursor_and_first_close_keeps_the_ofd() {
    let mut handler = RamfsHandler::new(|| 1000);
    let file = open(&mut handler, 10, "d", flags(O_RDWR | O_CREAT)).expect("open");
    write(&mut handler, file, b"abcdef").expect("write");
    let file = open(&mut handler, 11, "d", flags(O_RDONLY)).expect("reenopen");
    let mut first = [0u8; 3];
    assert_eq!(read(&mut handler, file, &mut first).expect("read 1"), 3);
    let duplicate = dup(&mut handler, file).expect("dup");
    assert_eq!(duplicate, file);
    close(&mut handler, file).expect("first close");
    let mut rest = [0u8; 3];
    assert_eq!(read(&mut handler, duplicate, &mut rest).expect("read 2"), 3);
    assert_eq!(&rest, b"def");
    close(&mut handler, duplicate).expect("last close");
    assert_eq!(read(&mut handler, duplicate, &mut rest), Err(Errno::EBADF));
}

#[test]
fn unlinked_file_survives_until_the_last_reference() {
    let mut handler = RamfsHandler::new(|| 1000);
    let first = open(&mut handler, 10, "u", flags(O_RDWR | O_CREAT)).expect("first");
    write(&mut handler, first, b"data").expect("write");
    let second = open(&mut handler, 11, "u", flags(O_RDONLY)).expect("second");
    let ino = handler.fs.lookup("u").expect("lookup");
    let root = root(&mut handler);
    call_status(&mut handler, root, Call::Unlink { path: b"/u" }).expect("unlink");
    assert!(handler.fs.is_unlinked(ino));
    close(&mut handler, first).expect("first close");
    assert!(handler.fs.stat(ino).is_ok());
    let mut data = [0u8; 4];
    assert_eq!(read(&mut handler, second, &mut data).expect("read"), 4);
    assert_eq!(&data, b"data");
    close(&mut handler, second).expect("last close");
    assert!(handler.fs.stat(ino).is_err());
}

#[test]
fn open_budget_is_emfile_and_recovers() {
    let mut handler = RamfsHandler::new(|| 1000);
    let root = root(&mut handler);
    call_status(
        &mut handler,
        root,
        Call::Mkdir {
            mode: 0o755,
            path: b"many",
        },
    )
    .expect("mkdir");
    close(&mut handler, root).expect("close root");
    for index in 0..MAX_OPEN_FILES {
        open(&mut handler, 10 + index as u64, "many", flags(O_RDONLY)).expect("open");
    }
    assert_eq!(
        open(&mut handler, 9999, "many", flags(O_RDONLY)),
        Err(Errno::EMFILE)
    );
    for index in 0..MAX_OPEN_FILES {
        close(&mut handler, 10 + index as u64).expect("close");
    }
    assert!(open(&mut handler, 9999, "many", flags(O_RDONLY)).is_ok());
}

#[test]
fn nonzero_offsets_are_rejected() {
    let mut handler = RamfsHandler::new(|| 1000);
    let file = open(&mut handler, 10, "o", flags(O_RDWR | O_CREAT)).expect("open");
    assert_eq!(
        handler.read(file, 1, &mut [0; 4]).map_err(handler_error),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        handler.write(file, 1, b"x").map_err(handler_error),
        Err(Errno::EINVAL)
    );
}

#[test]
fn directory_mutation_invalidates_an_open_cursor() {
    let mut handler = RamfsHandler::new(|| 1000);
    let root = root(&mut handler);
    call_status(
        &mut handler,
        root,
        Call::Mkdir {
            mode: 0o755,
            path: b"d",
        },
    )
    .expect("mkdir");
    handler.fs.create_file_at(2, "a", 0o644, 1000).expect("a");
    handler.fs.create_file_at(2, "b", 0o644, 1000).expect("b");
    let dir = open(&mut handler, 11, "d", flags(O_RDONLY)).expect("open dir");
    let mut buffer = [0u8; 64];
    assert!(read(&mut handler, dir, &mut buffer).expect("first read") > 0);
    call_status(
        &mut handler,
        root,
        Call::Mkdir {
            mode: 0o755,
            path: b"d/c",
        },
    )
    .expect("mkdir c");
    assert_eq!(read(&mut handler, dir, &mut buffer), Err(Errno::EINVAL));
}

#[test]
fn small_directory_buffers_are_einval() {
    let mut handler = RamfsHandler::new(|| 1000);
    let root = root(&mut handler);
    call_status(
        &mut handler,
        root,
        Call::Mkdir {
            mode: 0o755,
            path: b"d",
        },
    )
    .expect("mkdir");
    handler.fs.create_file_at(2, "a", 0o644, 1000).expect("a");
    let dir = open(&mut handler, 11, "d", flags(O_RDONLY)).expect("open dir");
    assert_eq!(read(&mut handler, dir, &mut [0u8; 8]), Err(Errno::EINVAL));
}

#[test]
fn call_path_operations_and_failures() {
    let mut handler = RamfsHandler::new(|| 1000);
    let root = root(&mut handler);
    call_status(
        &mut handler,
        root,
        Call::Mkdir {
            mode: 0o755,
            path: b"d",
        },
    )
    .expect("mkdir");
    call_status(
        &mut handler,
        root,
        Call::Symlink {
            path: b"d/l",
            target: b"a",
        },
    )
    .expect("symlink");
    let (op, response) =
        call(&mut handler, root, Call::Readlink { path: b"d/l" }).expect("readlink");
    match CallResponse::decode(op, &response).expect("decode") {
        CallResponse::Readlink { status: 0, target } => assert_eq!(target, b"a"),
        _ => panic!("bad readlink response"),
    }
    call_status(&mut handler, root, Call::Unlink { path: b"d/l" }).expect("unlink");
    let file = open(&mut handler, 12, "f", flags(O_RDWR | O_CREAT)).expect("create f");
    close(&mut handler, file).expect("close f");
    call_status(
        &mut handler,
        root,
        Call::Chmod {
            mode: 0o600,
            path: b"f",
        },
    )
    .expect("chmod f");
    call_status(&mut handler, root, Call::Rmdir { path: b"d" }).expect("rmdir");
    assert_eq!(
        call_status(
            &mut handler,
            root,
            Call::Mount {
                path: b"/m",
                scheme: b"ramfs:",
            }
        ),
        Err(Errno::ENOSYS)
    );
    assert_eq!(
        call_status(&mut handler, root, Call::Umount { path: b"/m" }),
        Err(Errno::ENOSYS)
    );
    assert!(call_status(&mut handler, root, Call::Unlink { path: b"missing" }).is_err());
    assert!(handler.call(root, &[0xff], &mut [0; 4]).is_err());
}

#[test]
fn call_statfs_and_sync_succeed() {
    let mut handler = RamfsHandler::new(|| 1000);
    let root = root(&mut handler);
    let (op, response) = call(&mut handler, root, Call::Statfs).expect("statfs");
    match CallResponse::decode(op, &response).expect("decode") {
        CallResponse::StatFs { status: 0, statfs } => assert!(statfs.files > 0),
        _ => panic!("bad statfs response"),
    }
    call_status(&mut handler, root, Call::Sync).expect("sync");
}

#[test]
fn truncate_call_on_a_directory_is_eisdir() {
    let mut handler = RamfsHandler::new(|| 1000);
    let root = root(&mut handler);
    call_status(
        &mut handler,
        root,
        Call::Mkdir {
            mode: 0o755,
            path: b"d",
        },
    )
    .expect("mkdir");
    let dir = open(&mut handler, 11, "d", flags(O_RDONLY)).expect("open");
    assert_eq!(
        handler.call(dir, &encode_call(Call::Truncate { size: 0 }), &mut [0; 64]),
        Err(Errno::EISDIR.into())
    );
}

#[test]
fn server_dispatch_open_and_close_use_the_fake_kernel() {
    let kernel = FakeKernel::new();
    let mut server = Server::register_with(kernel.clone(), "ramfs").expect("register");
    let mut handler = RamfsHandler::new(|| 1000);
    handler.expect_open(7, "x");
    let mut packet = SchemePacket::request(Opcode::Open, 1, 7);
    packet.flags = flags(O_RDWR | O_CREAT).to_wire();
    packet.set_path(b"x").expect("path");
    server
        .dispatch(
            Packet {
                handle: Handle(9),
                packet,
            },
            &mut handler,
        )
        .expect("dispatch open");
    let sent = kernel.sent();
    let response = raven_ipc::protocol::decode(&sent[0]).expect("decode");
    assert_eq!(response.status, 0);
    assert_eq!(response.server_cookie, 7);

    let mut close = SchemePacket::request(Opcode::Close, 2, 7);
    close.server_cookie = 7;
    server
        .dispatch(
            Packet {
                handle: Handle(10),
                packet: close,
            },
            &mut handler,
        )
        .expect("dispatch close");
    assert_eq!(
        raven_ipc::protocol::decode(&kernel.sent()[1])
            .expect("decode")
            .status,
        0
    );
}

#[test]
fn no_pending_open_is_invalid() {
    let mut handler = RamfsHandler::new(|| 1000);
    assert_eq!(
        handler
            .open_with("x", flags(O_RDONLY), 0, &ctx())
            .map_err(handler_error),
        Err(Errno::EINVAL)
    );
}

fn encode_call(call: Call<'_>) -> Vec<u8> {
    let request = CallRequest::new(call);
    let mut encoded = [0u8; 640];
    let length = request.encode(&mut encoded).expect("encode");
    encoded[..length].to_vec()
}
