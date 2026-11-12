// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD console server (`tty:` scheme).
//!
//! One reader thread pulls raw console bytes with `DEBUG_READ`, feeds them through the line
//! discipline in `raven-tty-core` and writes the echo back with `DEBUG_WRITE`. The scheme server
//! serves `tty:console`: `READ` returns whole lines in canonical mode (waiting for one when the
//! line is not complete yet), `WRITE` goes straight to the console, `FSTAT` reports a character
//! device and `CALL` carries the mode operations from `raven-proto-tty`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::{BTreeMap, VecDeque};
    use core::sync::atomic::{AtomicU32, Ordering};

    use raven_abi::errno::Errno;
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_proto_tty::{CALL_LEN, Request, Response, decode_request, encode_response};
    use raven_rt::entry;
    use raven_rt::sync::Mutex;
    use raven_scheme::{Ctx, Handler, HandlerError, HandlerResult, Opened, Server};
    use raven_tty_core::{Discipline, Event, Output};

    entry!(main);

    /// Console path accepted by `OPEN`.
    const CONSOLE_PATH: &str = "console";
    /// Bytes read from the UART per `DEBUG_READ`.
    const READ_CHUNK: usize = 256;
    /// Reader thread stack.
    const READER_STACK: usize = 32 * 1024;

    const OPEN_READ: u32 = 1;
    const OPEN_WRITE: u32 = 2;

    /// Wakes a blocked `READ` whenever the console state changes.
    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// Shared console state: the discipline plus what readers have not consumed yet.
    struct Console {
        discipline: Discipline,
        ready: VecDeque<u8>,
        interrupt: bool,
        eof: bool,
    }

    impl Console {
        const fn new() -> Self {
            Self {
                discipline: Discipline::new(),
                ready: VecDeque::new(),
                interrupt: false,
                eof: false,
            }
        }

        /// Apply one `feed` result: buffer its lines and latch its events.
        fn accept(&mut self, output: &Output) {
            for line in &output.lines {
                self.ready.extend(line.iter().copied());
            }
            for event in &output.events {
                match event {
                    Event::Interrupt => self.interrupt = true,
                    Event::EndOfFile => self.eof = true,
                }
            }
        }

        /// Move up to `buffer.len()` ready bytes into `buffer`.
        fn take(&mut self, buffer: &mut [u8]) -> usize {
            let count = buffer.len().min(self.ready.len());
            for slot in buffer.iter_mut().take(count) {
                if let Some(byte) = self.ready.pop_front() {
                    *slot = byte;
                }
            }
            count
        }

        /// Drop everything buffered, returning the number of bytes discarded.
        fn flush(&mut self) -> u32 {
            let pending = self.discipline.flush() + self.ready.len();
            self.ready.clear();
            self.eof = false;
            u32::try_from(pending).unwrap_or(u32::MAX)
        }
    }

    static CONSOLE: Mutex<Console> = Mutex::new(Console::new());

    /// Publish a state change: bump the sequence word and wake blocked readers.
    fn publish() {
        SEQ.fetch_add(1, Ordering::Release);
        let _ = raven_rt::futex_wake(&SEQ, u32::MAX);
    }

    /// Reader thread: pull console bytes and feed the discipline forever.
    fn reader(_argument: usize) {
        let mut buffer = [0u8; READ_CHUNK];
        loop {
            match raven_rt::debug_read(&mut buffer, 0) {
                Ok(0) => continue,
                Ok(count) => {
                    let mut output = Output::default();
                    {
                        let mut console = CONSOLE.lock();
                        console.discipline.feed(&buffer[..count], &mut output);
                        console.accept(&output);
                    }
                    if !output.echo.is_empty() {
                        let _ = raven_rt::debug_write(&output.echo);
                    }
                    if !output.lines.is_empty() || !output.events.is_empty() {
                        publish();
                    }
                }
                Err(error) => {
                    // The console is gone (or the syscall failed): report EOF to readers.
                    if error != Errno::EINTR {
                        CONSOLE.lock().eof = true;
                        publish();
                    }
                }
            }
        }
    }

    struct OpenFile {
        readable: bool,
        writable: bool,
    }

    struct ConsoleHandler {
        files: BTreeMap<u64, OpenFile>,
        next: u64,
    }

    impl ConsoleHandler {
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

    impl Handler for ConsoleHandler {
        fn open(
            &mut self,
            path: &str,
            flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            // The scheme root (`tty:` or `tty:/`) is the console: readiness probes open it.
            let path = path.strip_prefix('/').unwrap_or(path);
            if !path.is_empty() && path != CONSOLE_PATH {
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
            if buffer.is_empty() {
                return Ok(0);
            }
            loop {
                let sequence = SEQ.load(Ordering::Acquire);
                {
                    let mut console = CONSOLE.lock();
                    if console.interrupt {
                        console.interrupt = false;
                        return Err(Errno::EINTR.into());
                    }
                    if !console.ready.is_empty() {
                        return Ok(console.take(buffer));
                    }
                    if console.eof {
                        return Ok(0);
                    }
                }
                let _ = raven_rt::futex_wait(&SEQ, sequence, None);
            }
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            if !self.file(file)?.writable {
                return Err(Errno::EACCES.into());
            }
            let written = raven_rt::debug_write(buffer).map_err(HandlerError::from)?;
            Ok(written)
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

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            self.file(file)?;
            Ok(Stat {
                ino: file,
                mode: S_IFCHR | 0o620,
                nlink: 1,
                ..Stat::default()
            })
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            self.file(file)?;
            let request = decode_request(request).map_err(|_| HandlerError::from(Errno::ENOSYS))?;
            let reply = match request {
                Request::GetMode => Response::Mode(CONSOLE.lock().discipline.settings()),
                Request::SetMode { settings } => {
                    let applied = {
                        let mut console = CONSOLE.lock();
                        let discarded = console.discipline.set_settings(settings);
                        if discarded > 0 {
                            console.ready.clear();
                            console.eof = false;
                        }
                        console.discipline.settings()
                    };
                    publish();
                    Response::Mode(applied)
                }
                Request::Flush => {
                    let bytes = CONSOLE.lock().flush();
                    publish();
                    Response::Flushed { bytes }
                }
            };
            if response.len() < CALL_LEN {
                return Err(Errno::EINVAL.into());
            }
            let mut encoded = [0u8; CALL_LEN];
            encode_response(reply, &mut encoded);
            response[..CALL_LEN].copy_from_slice(&encoded);
            Ok(CALL_LEN)
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let mut server = match Server::register("tty") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("consd: {}", error.description());
                return 1;
            }
        };
        match raven_rt::thread::spawn(reader, 0, READER_STACK) {
            Ok(_reader) => {}
            Err(error) => {
                raven_rt::eprintln!("consd: reader thread: {}", error.description());
                return 1;
            }
        }
        let mut handler = ConsoleHandler::new();
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
