// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Hostile unprivileged scheme server: serves a valid ELF but reports `04755` uid 0.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_proto_fs::{OpenFlags, S_IFREG, Stat};
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::{Ctx, Handler, HandlerResult, OpenTable, Opened, Server};
use raven_scheme_publisher::{FAKE_SCHEME, initfs};

entry!(main);

struct Fake {
    child: &'static [u8],
    opens: OpenTable<()>,
}

impl Handler for Fake {
    fn open_with(
        &mut self,
        path: &str,
        flags: OpenFlags,
        _mode: u32,
        _ctx: &Ctx,
    ) -> HandlerResult<Opened> {
        if path != "child" && path != "/child" {
            return Err(Errno::ENOENT.into());
        }
        let id = self.opens.insert((), flags)?;
        Ok(Opened::File(id))
    }

    fn read(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        let _ = self.opens.get(file).ok_or(Errno::EBADF)?;
        let start = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
        let Some(source) = self.child.get(start..) else {
            return Ok(0);
        };
        let length = source.len().min(buffer.len());
        buffer[..length].copy_from_slice(&source[..length]);
        Ok(length)
    }

    fn pread(&mut self, file: u64, offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        self.read(file, offset, buffer)
    }

    fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
        let _ = self.opens.get(file).ok_or(Errno::EBADF)?;
        Ok(Stat {
            ino: 1,
            size: self.child.len() as u64,
            blocks: 0,
            mtime: 0,
            ctime: 0,
            atime: 0,
            mode: S_IFREG | 0o4755,
            nlink: 1,
            uid: 0,
            gid: 0,
        })
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        self.opens.close(file)?;
        Ok(())
    }

    fn dup(&mut self, file: u64) -> HandlerResult<u64> {
        Ok(self.opens.dup(file)?)
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    let initfs = match initfs() {
        Ok(initfs) => initfs,
        Err(message) => {
            println!("fake-server: {message}");
            return 1;
        }
    };
    let Some(entry) = initfs.get("/bin/child") else {
        println!("fake-server: child missing");
        return 1;
    };
    let mut server = match Server::register(FAKE_SCHEME) {
        Ok(server) => server,
        Err(error) => {
            println!("fake-server: register errno={}", error.0);
            return 1;
        }
    };
    println!("fake-server: ready");
    let mut handler = Fake {
        child: entry.data,
        opens: OpenTable::new(),
    };
    server.run(&mut handler);
}
