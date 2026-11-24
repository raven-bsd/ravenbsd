// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB HID boot-keyboard console service.
//!
//! The S60/S62 decoder and the bounded [`ConsoleInput`] queue live in this crate's library. This
//! target binary owns the queue and serves it as `kbd:`:
//!
//! * `READ` drains queued ASCII bytes; an empty queue returns `EAGAIN`.
//! * `WRITE` appends bytes for the xHCI driver; a full queue returns `ENOSPC`.
//! * `OPEN` is the scheme root only.
//!
//! The xHCI driver pushes decoded boot-keyboard bytes through this scheme, and CON1 can consume
//! them without a shared-memory side channel. A missing keyboard leaves the queue empty and never
//! blocks UART boot/login.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::BTreeMap;

    use raven_abi::errno::Errno;
    use raven_rt::entry;
    use raven_rt::sync::Mutex;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};
    use usb_hid::{ConsoleInput, accepts_console_path};

    entry!(main);

    const OPEN_READ: u32 = 1;
    const OPEN_WRITE: u32 = 2;

    static QUEUE: Mutex<ConsoleInput> = Mutex::new(ConsoleInput::new());

    struct OpenFile {
        readable: bool,
        writable: bool,
    }

    struct KbdHandler {
        files: BTreeMap<u64, OpenFile>,
        next: u64,
    }

    impl KbdHandler {
        fn new() -> Self {
            Self {
                files: BTreeMap::new(),
                next: 1,
            }
        }

        fn file(&self, file: u64) -> Result<&OpenFile, Errno> {
            self.files.get(&file).ok_or(Errno::EBADF)
        }
    }

    impl Handler for KbdHandler {
        fn open(
            &mut self,
            path: &str,
            flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let path = path.strip_prefix('/').unwrap_or(path);
            if !accepts_console_path(path) {
                return Err(Errno::ENOENT.into());
            }
            let readable = flags & OPEN_READ != 0;
            let writable = flags & OPEN_WRITE != 0;
            if !readable && !writable {
                return Err(Errno::EACCES.into());
            }
            let file = self.next;
            self.next = self.next.wrapping_add(1);
            self.files.insert(file, OpenFile { readable, writable });
            Ok(Opened::File(file))
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            if !self.file(file)?.readable {
                return Err(Errno::EACCES.into());
            }
            let count = QUEUE.lock().read(buffer);
            if count == 0 {
                Err(Errno::EAGAIN.into())
            } else {
                Ok(count)
            }
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            if !self.file(file)?.writable {
                return Err(Errno::EACCES.into());
            }
            let mut queue = QUEUE.lock();
            for byte in buffer {
                if !queue.push(*byte) {
                    return Err(Errno::ENOSPC.into());
                }
            }
            Ok(buffer.len())
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.files
                .remove(&file)
                .map(|_| ())
                .ok_or(Errno::EBADF.into())
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            let (readable, writable) = {
                let open = self.file(file)?;
                (open.readable, open.writable)
            };
            let duplicate = self.next;
            self.next = self.next.wrapping_add(1);
            self.files
                .insert(duplicate, OpenFile { readable, writable });
            Ok(duplicate)
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let mut server = match Server::register("kbd") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("usb-hid: {}", error.description());
                return 1;
            }
        };
        raven_rt::println!("usb-hid: kbd queue ready");
        let mut handler = KbdHandler::new();
        loop {
            let Some(packet) = server.poll(None) else {
                continue;
            };
            let _ = server.dispatch(packet, &mut handler);
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
