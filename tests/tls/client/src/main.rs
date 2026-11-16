// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot target TLS client for the OpenSSL add-on.
//!
//! It connects to the QEMU user-net gateway (`10.0.2.2`) where the host `openssl s_server` runs,
//! performs a certificate-verified TLS handshake through `raven-tls`, sends an HTTP request and
//! reads the response. The libc `crt1.o` supplies `main`'s process startup; transcript lines go to
//! the RavenBSD debug console because a spawned C/Rust process has no POSIX stdout fd yet.
//!
//! The byte stream is the `net:` scheme rather than a POSIX socket: the libc lazy-socket path still
//! reports `ENOSYS` for a pending OFD, and the T1 add-on only needs a connected byte stream.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use core::alloc::{GlobalAlloc, Layout};
use core::ffi::{c_int, c_long, c_void};
use core::ptr;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_net::{Call, Endpoint, Response, SocketRequest};
use raven_scheme::Client;
use raven_tls::{Connection, Error as TlsError, Transport, TransportError};

/// QEMU user-mode networking maps the host loopback to this gateway address.
const HOST: &str = "10.0.2.2";
/// Host address as octets for the `net:tcp/...` path.
const HOST_OCTETS: [u8; 4] = [10, 0, 2, 2];
/// Port the host `openssl s_server` listens on (see `tests/tls/client/start-server.sh`).
const PORT: u16 = 24443;
/// CA certificate packed at the initfs root (the server profile mounts `/etc` on ramfs).
const CA_FILE: &str = "/tls-ca.pem";
/// The socket and TCP connect may race netd readiness; retry briefly before failing the boot.
const CONNECT_ATTEMPTS: u32 = 50;
const CONNECT_DELAY_MS: u64 = 200;
/// `net:` reads report `EAGAIN` until data arrives; a blocking TLS BIO waits here.
const IO_ATTEMPTS: u32 = 2_000;
const IO_DELAY_MS: u64 = 5;
/// `READ|WRITE` open flags for a bidirectional stream socket.
const READ_WRITE: u32 = 3;

struct LibcAllocator;

// SAFETY: the four functions forward the Rust allocator contract to the RavenBSD libc heap.
unsafe impl GlobalAlloc for LibcAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.align() <= 16 {
            // SAFETY: malloc returns `layout.size()` writable bytes or NULL.
            unsafe { malloc(layout.size()).cast() }
        } else {
            let mut pointer = ptr::null_mut();
            // SAFETY: `pointer` is writable and the libc call checks the alignment.
            let status = unsafe { posix_memalign(&mut pointer, layout.align(), layout.size()) };
            if status == 0 {
                pointer.cast()
            } else {
                ptr::null_mut()
            }
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, _layout: Layout) {
        // SAFETY: `pointer` came from malloc/posix_memalign and is freed exactly once.
        unsafe { free(pointer.cast()) }
    }
}

#[global_allocator]
static ALLOCATOR: LibcAllocator = LibcAllocator;

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(pointer: *mut c_void);
    fn posix_memalign(pointer: *mut *mut c_void, alignment: usize, size: usize) -> c_int;
    fn nanosleep(request: *const Timespec, remaining: *mut Timespec) -> c_int;
}

#[repr(C)]
struct Timespec {
    seconds: c_long,
    nanoseconds: c_long,
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    output("raven-tls: panic\n");
    loop {
        core::hint::spin_loop();
    }
}

/// C process entry point supplied by the libc `crt1.o`.
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const u8) -> c_int {
    match run() {
        Ok(()) => 0,
        Err(error) => {
            report(error);
            1
        }
    }
}

enum Failure {
    Transport(Errno),
    Tls(TlsError),
}

fn run() -> Result<(), Failure> {
    output("raven-tls: connect ");
    output(HOST);
    output(":");
    number(PORT as usize);
    output("\n");
    let transport = connect_transport().map_err(Failure::Transport)?;
    let mut connection: Connection =
        raven_tls::connect(Box::new(transport), HOST, CA_FILE).map_err(Failure::Tls)?;
    output("raven-tls: handshake ok\n");

    let request = b"GET / HTTP/1.0\r\nHost: 10.0.2.2\r\n\r\n";
    let written = connection.write(request).map_err(Failure::Tls)?;
    output("raven-tls: wrote ");
    number(written);
    output(" bytes\n");
    if written != request.len() {
        return Err(Failure::Tls(TlsError::Io));
    }

    let mut response = [0u8; 768];
    let read = connection.read(&mut response).map_err(Failure::Tls)?;
    output("raven-tls: read ");
    number(read);
    output(" bytes\n");
    if read == 0 {
        return Err(Failure::Tls(TlsError::Io));
    }
    output("raven-tls: client ok\n");
    Ok(())
}

fn connect_transport() -> Result<NetTransport, Errno> {
    let mut attempt = 0;
    loop {
        match NetTransport::connect() {
            Ok(transport) => return Ok(transport),
            Err(_) if attempt + 1 < CONNECT_ATTEMPTS => {
                attempt += 1;
                sleep_ms(CONNECT_DELAY_MS);
            }
            Err(error) => return Err(error),
        }
    }
}

struct NetTransport {
    client: Client,
    handle: Handle,
}

impl NetTransport {
    fn connect() -> Result<Self, Errno> {
        let client = Client::new();
        let path = alloc::format!(
            "net:tcp/{}.{}.{}.{}:{}",
            HOST_OCTETS[0],
            HOST_OCTETS[1],
            HOST_OCTETS[2],
            HOST_OCTETS[3],
            PORT
        );
        let handle = client.open(&path, READ_WRITE)?;
        let request = SocketRequest::Call(Call::Connect(Endpoint {
            ip: HOST_OCTETS,
            port: PORT,
        }));
        let mut encoded = [0u8; 64];
        let length = match request.encode(&mut encoded) {
            Ok(length) => length,
            Err(_) => {
                let _ = client.close(handle);
                return Err(Errno::EINVAL);
            }
        };
        let mut response = [0u8; 64];
        let count = match client.call(handle, &encoded[..length], &mut response) {
            Ok(count) => count,
            Err(error) => {
                let _ = client.close(handle);
                return Err(error);
            }
        };
        match Response::decode(&response[..count]) {
            Ok(Response::Ok) => Ok(Self { client, handle }),
            Ok(Response::Err(error)) => {
                let _ = client.close(handle);
                Err(error)
            }
            _ => {
                let _ = client.close(handle);
                Err(Errno::EPROTO)
            }
        }
    }
}

impl Transport for NetTransport {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, TransportError> {
        for _ in 0..IO_ATTEMPTS {
            match self.client.read(self.handle, 0, buffer) {
                Ok(count) => return Ok(count),
                Err(error) if retryable(error) => sleep_ms(IO_DELAY_MS),
                Err(_) => return Err(TransportError),
            }
        }
        Err(TransportError)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, TransportError> {
        for _ in 0..IO_ATTEMPTS {
            match self.client.write(self.handle, 0, buffer) {
                Ok(count) if count > 0 => return Ok(count),
                Ok(_) => sleep_ms(IO_DELAY_MS),
                Err(error) if retryable(error) => sleep_ms(IO_DELAY_MS),
                Err(_) => return Err(TransportError),
            }
        }
        Err(TransportError)
    }
}

impl Drop for NetTransport {
    fn drop(&mut self) {
        let _ = self.client.close(self.handle);
    }
}

fn retryable(error: Errno) -> bool {
    matches!(error, Errno::EAGAIN | Errno::ENOTCONN | Errno::EINPROGRESS)
}

fn report(error: Failure) {
    output("raven-tls: error ");
    match error {
        Failure::Transport(error) => {
            output("transport-");
            number(error.0 as usize);
            output("\n");
        }
        Failure::Tls(error) => match error {
            TlsError::HostTooLong => output("host-too-long\n"),
            TlsError::CaPathTooLong => output("ca-path-too-long\n"),
            TlsError::Context => output("context\n"),
            TlsError::CaBundle => output("ca-bundle\n"),
            TlsError::Bio => output("bio\n"),
            TlsError::Handshake => output("handshake\n"),
            TlsError::Verify(code) => {
                output("verify-");
                number(code as usize);
                output("\n");
            }
            TlsError::Io => output("io\n"),
        },
    }
}

fn output(text: &str) {
    let _ = debug_write(text.as_bytes());
}

fn number(value: usize) {
    let mut digits = [0u8; 20];
    let mut index = digits.len();
    let mut value = value;
    loop {
        index -= 1;
        digits[index] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let _ = debug_write(&digits[index..]);
}

fn debug_write(bytes: &[u8]) -> Result<(), ()> {
    // raven-abi Syscall::DEBUG_WRITE (0x03): debug_write(ptr, len).
    let mut number: usize = 3;
    let pointer = bytes.as_ptr() as usize;
    let length = bytes.len();
    // SAFETY: DEBUG_WRITE reads `length` bytes from the valid slice pointer and returns the
    // number written; the registers follow the RavenBSD syscall ABI.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") number,
            in("rdi") pointer,
            in("rsi") length,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    let _ = number;
    Ok(())
}

fn sleep_ms(milliseconds: u64) {
    let request = Timespec {
        seconds: (milliseconds / 1000) as c_long,
        nanoseconds: ((milliseconds % 1000) * 1_000_000) as c_long,
    };
    // SAFETY: `request` is a valid timespec and `remaining` is NULL, which libc allows.
    unsafe {
        let _ = nanosleep(&request, ptr::null_mut());
    }
}
