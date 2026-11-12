// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Handler tests over a small scripted filesystem.

use alloc::vec::Vec;

use raven_abi::types::Handle;
use raven_proto_fs::{
    Call, CallRequest, CallResponse, DT_REG, DirEntry, DirEntryHeader, MAX_PATH, S_IFDIR, S_IFLNK,
    S_IFMT, S_IFREG, Stat, StatFs,
};
use raven_scheme::HandlerError;

use super::*;

const ROOT: u32 = 2;
const FILE: u32 = 3;
const DIR: u32 = 4;
const LINK: u32 = 5;

struct FakeFs {
    data: Vec<u8>,
}

impl FakeFs {
    fn new() -> Self {
        Self {
            data: b"hello\n".to_vec(),
        }
    }

    fn stat_for(ino: u32) -> Stat {
        match ino {
            FILE => Stat {
                ino: u64::from(FILE),
                size: 6,
                mode: S_IFREG | 0o644,
                nlink: 1,
                uid: 1000,
                gid: 100,
                ..Stat::default()
            },
            DIR => Stat {
                ino: u64::from(DIR),
                size: 64,
                mode: S_IFDIR | 0o755,
                nlink: 2,
                ..Stat::default()
            },
            LINK => Stat {
                ino: u64::from(LINK),
                size: 6,
                mode: S_IFLNK | 0o777,
                nlink: 1,
                ..Stat::default()
            },
            _ => Stat {
                ino: u64::from(ROOT),
                size: 128,
                mode: S_IFDIR | 0o755,
                nlink: 3,
                ..Stat::default()
            },
        }
    }
}

impl FileSystem for FakeFs {
    fn resolve(&mut self, path: &str) -> Result<u32, Errno> {
        match path {
            "" | "/" => Ok(ROOT),
            "/file" => Ok(FILE),
            "/dir" => Ok(DIR),
            "/link" => Ok(FILE),
            _ => Err(Errno::ENOENT),
        }
    }

    fn resolve_nofollow(&mut self, path: &str) -> Result<u32, Errno> {
        if path == "/link" {
            Ok(LINK)
        } else {
            self.resolve(path)
        }
    }

    fn stat(&mut self, ino: u32) -> Result<Stat, Errno> {
        Ok(Self::stat_for(ino))
    }

    fn read(&mut self, ino: u32, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        if ino != FILE {
            return Err(Errno::EISDIR);
        }
        let start = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
        let available = self.data.get(start..).unwrap_or(&[]);
        let length = available.len().min(buffer.len());
        buffer[..length].copy_from_slice(&available[..length]);
        Ok(length)
    }

    fn readdir(&mut self, ino: u32, cookie: u64, buffer: &mut [u8]) -> Result<(usize, u64), Errno> {
        if ino != DIR && ino != ROOT {
            return Err(Errno::ENOTDIR);
        }
        if cookie != 0 {
            return Ok((0, 0));
        }
        if buffer.len() < 24 {
            return Err(Errno::EINVAL);
        }
        let entry = DirEntry {
            header: DirEntryHeader {
                ino: u64::from(FILE),
                kind: DT_REG,
                name_len: 4,
                pad: [0; 6],
            },
            name: b"file",
        };
        let length = entry.encoded_len();
        entry
            .encode(&mut buffer[..length])
            .map_err(|_| Errno::EIO)?;
        Ok((length, 1))
    }

    fn readlink(&mut self, ino: u32, buffer: &mut [u8]) -> Result<usize, Errno> {
        if ino != LINK {
            return Err(Errno::EINVAL);
        }
        let target = b"target";
        buffer[..target.len()].copy_from_slice(target);
        Ok(target.len())
    }

    fn statfs(&self) -> StatFs {
        StatFs {
            block_size: 4096,
            blocks: 100,
            free_blocks: 40,
            files: 20,
            free_files: 10,
        }
    }
}

fn ctx() -> Ctx {
    Ctx {
        request: Handle(1),
        token: 1,
        buffer_len: 0,
        response_len: 0,
    }
}

fn open(handler: &mut FsHandler<FakeFs>, id: u64, path: &str, flags: OpenFlags) -> Opened {
    handler.expect_open(id, path);
    handler.open_with(path, flags, 0, &ctx()).expect("open")
}

fn opened() -> FsHandler<FakeFs> {
    let mut handler = FsHandler::new(FakeFs::new());
    match open(&mut handler, 7, "/file", OpenFlags::READ) {
        Opened::File(7) => {}
        other => panic!("unexpected open result: {other:?}"),
    }
    handler
}

#[test]
fn opens_the_root_and_files() {
    let mut handler = FsHandler::new(FakeFs::new());
    for (id, path) in [(7, ""), (8, "/"), (9, "/file"), (10, "/dir")] {
        match open(&mut handler, id, path, OpenFlags::READ) {
            Opened::File(file) => assert_eq!(file, id),
            other => panic!("unexpected open result: {other:?}"),
        }
    }
    assert_eq!(handler.opens.len(), 4);
}

#[test]
fn write_and_create_are_rejected() {
    let mut handler = FsHandler::new(FakeFs::new());
    for (index, flags) in [
        OpenFlags::WRITE,
        OpenFlags::READ_WRITE,
        OpenFlags::from_linux(0o100 | 1).expect("flags"),
    ]
    .into_iter()
    .enumerate()
    {
        handler.expect_open(20 + index as u64, "/file");
        assert_eq!(
            handler
                .open_with("/file", flags, 0, &ctx())
                .expect_err("read-only"),
            HandlerError::Errno(Errno::EROFS)
        );
    }
}

#[test]
fn directory_flag_requires_a_directory() {
    let mut handler = FsHandler::new(FakeFs::new());
    handler.expect_open(7, "/file");
    assert_eq!(
        handler
            .open_with(
                "/file",
                OpenFlags::from_linux(0o200000).expect("flags"),
                0,
                &ctx()
            )
            .expect_err("not a directory"),
        HandlerError::Errno(Errno::ENOTDIR)
    );
}

#[test]
fn read_advances_the_file_cursor() {
    let mut handler = opened();
    let mut response = [0u8; 16];
    assert_eq!(handler.read(7, 0, &mut response[..8]).expect("read"), 6);
    assert_eq!(&response[..6], b"hello\n");
    assert_eq!(handler.read(7, 0, &mut response[..8]).expect("eof"), 0);
    assert_eq!(
        handler.read(7, 1, &mut response[..8]).expect_err("offset"),
        HandlerError::Errno(Errno::EINVAL)
    );
    assert_eq!(
        handler
            .read(8, 0, &mut response[..8])
            .expect_err("bad file"),
        HandlerError::Errno(Errno::EBADF)
    );
}

#[test]
fn directory_read_advances_the_cookie() {
    let mut handler = FsHandler::new(FakeFs::new());
    match open(&mut handler, 7, "/dir", OpenFlags::READ) {
        Opened::File(7) => {}
        other => panic!("unexpected open result: {other:?}"),
    }
    let mut buffer = [0u8; 64];
    let read = handler.read(7, 0, &mut buffer).expect("readdir");
    assert_eq!(read, 24);
    assert_eq!(handler.read(7, 0, &mut buffer).expect("eof"), 0);
}

#[test]
fn fstat_and_dup_keep_the_inode() {
    let mut handler = opened();
    let stat = handler.fstat(7).expect("fstat");
    assert_eq!(stat.ino, u64::from(FILE));
    assert_eq!(stat.mode & S_IFMT, S_IFREG);
    let dup = handler.dup(7).expect("dup");
    assert_eq!(dup, 7);
    let stat = handler.fstat(dup).expect("fstat dup");
    assert_eq!(stat.ino, u64::from(FILE));
    handler.close(dup).expect("close dup");
    assert!(handler.fstat(7).is_ok());
    handler.close(7).expect("close last");
    assert!(handler.fstat(7).is_err());
}

#[test]
fn readlink_call_returns_the_target() {
    let mut handler = opened();
    let request = CallRequest::new(Call::Readlink { path: b"/link" });
    let mut encoded = [0u8; 64];
    let length = request.encode(&mut encoded).expect("encode");
    let mut response = [0u8; MAX_PATH + 8];
    let length = handler
        .call(7, &encoded[..length], &mut response)
        .expect("call");
    match CallResponse::decode(request.op(), &response[..length]).expect("response") {
        CallResponse::Readlink { status: 0, target } => assert_eq!(target, b"target"),
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn write_calls_return_erofs_and_sync_is_a_noop() {
    let mut handler = opened();
    let mut encoded = [0u8; 64];
    let mut response = [0u8; 64];

    for call in [
        Call::Mkdir {
            mode: 0o755,
            path: b"/new",
        },
        Call::Unlink { path: b"/file" },
        Call::Rmdir { path: b"/dir" },
        Call::Rename {
            from: b"/file",
            to: b"/other",
        },
        Call::Symlink {
            path: b"/new",
            target: b"file",
        },
        Call::Chmod {
            mode: 0o600,
            path: b"/file",
        },
    ] {
        let request = CallRequest::new(call);
        let length = request.encode(&mut encoded).expect("encode");
        let length = handler
            .call(7, &encoded[..length], &mut response)
            .expect("call");
        match CallResponse::decode(request.op(), &response[..length]).expect("response") {
            CallResponse::Status(status) => {
                assert_eq!(status, -i32::from(Errno::EROFS.0))
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    let request = CallRequest::new(Call::Truncate { size: 0 });
    let length = request.encode(&mut encoded).expect("encode");
    let length = handler
        .call(7, &encoded[..length], &mut response)
        .expect("call");
    assert_eq!(
        CallResponse::decode(request.op(), &response[..length]).expect("response"),
        CallResponse::Status(-i32::from(Errno::EBADF.0))
    );

    let request = CallRequest::new(Call::Sync);
    let length = request.encode(&mut encoded).expect("encode");
    let length = handler
        .call(7, &encoded[..length], &mut response)
        .expect("call");
    assert_eq!(
        CallResponse::decode(request.op(), &response[..length]).expect("response"),
        CallResponse::Status(0)
    );
}

#[test]
fn statfs_call_returns_the_record() {
    let mut handler = opened();
    let request = CallRequest::new(Call::Statfs);
    let mut encoded = [0u8; 64];
    let length = request.encode(&mut encoded).expect("encode");
    let mut response = [0u8; 64];
    let length = handler
        .call(7, &encoded[..length], &mut response)
        .expect("call");
    match CallResponse::decode(request.op(), &response[..length]).expect("response") {
        CallResponse::StatFs { status: 0, statfs } => {
            assert_eq!(statfs.block_size, 4096);
            assert_eq!(statfs.blocks, 100);
            assert_eq!(statfs.free_files, 10);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn calls_need_an_open_file() {
    let mut handler = FsHandler::new(FakeFs::new());
    let request = CallRequest::new(Call::Sync);
    let mut encoded = [0u8; 32];
    let length = request.encode(&mut encoded).expect("encode");
    let mut response = [0u8; 32];
    assert_eq!(
        handler
            .call(7, &encoded[..length], &mut response)
            .expect_err("not open"),
        HandlerError::Errno(Errno::EBADF)
    );
}
