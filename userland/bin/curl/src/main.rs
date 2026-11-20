// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `curl` utility.
//!
//! The base build fetches plain HTTP through `raven-http`/`raven-http-net` and returns
//! `EUNAVAILABLE` for HTTPS. The `tls` add-on build links `raven-tls` (OpenSSL) and enables
//! certificate- and hostname-verified HTTPS.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(all(target_os = "none", not(feature = "tls")))]
mod program {
    use alloc::string::String;

    use curl::{
        EXIT_ERROR, EXIT_HTTP_FAILED, EXIT_OK, EXIT_USAGE, FetchError, Options, fetch_with,
        format_head, is_https, parse,
    };
    use raven_abi::types::clock;
    use raven_http::Clock;
    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;

    entry!(main);

    struct SystemClock;

    impl Clock for SystemClock {
        fn now_ms(&self) -> u64 {
            match raven_rt::clock_get(clock::MONOTONIC) {
                Ok(time) => {
                    let seconds = u64::try_from(time.tv_sec).unwrap_or(0);
                    let nanos = u64::try_from(time.tv_nsec).unwrap_or(0);
                    seconds
                        .saturating_mul(1_000)
                        .saturating_add(nanos / 1_000_000)
                }
                Err(_) => 0,
            }
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(error) => {
                raven_rt::eprintln!("curl: {}\n{}", error.message(), usage());
                return EXIT_USAGE;
            }
        };
        if is_https(&options.url) {
            if !options.silent {
                raven_rt::eprintln!(
                    "curl: https unavailable (EUNAVAILABLE): TLS add-on not linked"
                );
            }
            return EXIT_ERROR;
        }
        let mut transport = raven_http_net::live_transport();
        let clock = SystemClock;
        let response = match fetch_with(&options.url, &options, &mut transport, &clock) {
            Ok(response) => response,
            Err(error) => {
                if !options.silent {
                    match error {
                        FetchError::HttpStatus(status) => {
                            raven_rt::eprintln!("curl: HTTP status {status}");
                        }
                        FetchError::Http(error) => {
                            raven_rt::eprintln!(
                                "curl: {} ({error:?})",
                                FetchError::Http(error).message()
                            );
                        }
                    }
                }
                return match error {
                    FetchError::HttpStatus(_) => EXIT_HTTP_FAILED,
                    FetchError::Http(_) => EXIT_ERROR,
                };
            }
        };
        let output = if options.headers_only {
            format_head(&response)
        } else {
            String::from_utf8_lossy(&response.body).into_owned()
        };
        match write_output(&options, output.as_bytes()) {
            Ok(()) => EXIT_OK,
            Err(status) => status,
        }
    }

    fn write_output(options: &Options, bytes: &[u8]) -> Result<(), i32> {
        match options.output.as_deref() {
            None | Some("-") => write_stdout(bytes).map_err(|_| EXIT_ERROR),
            Some(path) => write_file(path, bytes).map_err(|_| EXIT_ERROR),
        }
    }

    fn write_stdout(bytes: &[u8]) -> Result<(), raven_abi::errno::Errno> {
        let mut offset = 0usize;
        while offset < bytes.len() {
            let written = raven_rt::debug_write(bytes.get(offset..).unwrap_or(&[]))?;
            if written == 0 {
                return Err(raven_abi::errno::Errno::EIO);
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }

    fn write_file(path: &str, bytes: &[u8]) -> Result<(), raven_abi::errno::Errno> {
        let mut file = raven_rt::fs::open(path, O_WRONLY | O_CREAT | O_TRUNC)?;
        let mut offset = 0usize;
        while offset < bytes.len() {
            let written = file.write(bytes.get(offset..).unwrap_or(&[]))?;
            if written == 0 {
                return Err(raven_abi::errno::Errno::EIO);
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }

    fn usage() -> &'static str {
        "usage: curl [-s] [-f] [-I] [-L] [-o FILE] [--max-time SECS] [-H 'Name: value'] [--cacert FILE] URL"
    }
}

#[cfg(all(target_os = "none", feature = "tls"))]
mod tls_program {
    use alloc::string::String;
    use alloc::vec::Vec;
    use core::alloc::{GlobalAlloc, Layout};
    use core::ffi::{CStr, c_char, c_int, c_long, c_void};
    use core::ptr;

    use curl::{
        EXIT_ERROR, EXIT_HTTP_FAILED, EXIT_OK, EXIT_USAGE, FetchError, Options, fetch_with,
        format_head, is_https, parse,
    };
    use raven_http::Clock;

    /// libc allocation for OpenSSL and Rust `alloc`.
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
                let status =
                    unsafe { posix_memalign(&mut pointer, layout.align(), layout.size()) };
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

    #[repr(C)]
    struct TimeSpec {
        seconds: c_long,
        nanoseconds: c_long,
    }

    unsafe extern "C" {
        fn malloc(size: usize) -> *mut c_void;
        fn free(pointer: *mut c_void);
        fn posix_memalign(pointer: *mut *mut c_void, alignment: usize, size: usize) -> c_int;
        fn clock_gettime(clock_id: c_int, time: *mut TimeSpec) -> c_int;
    }

    #[panic_handler]
    fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
        output("curl: panic\n");
        loop {
            core::hint::spin_loop();
        }
    }

    /// C process entry point supplied by the libc `crt1.o`.
    ///
    /// # Safety
    ///
    /// Called by the C runtime with the standard `argc`/`argv` contract.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
        let mut values = Vec::new();
        for index in 0..argc.max(0) {
            // SAFETY: the C runtime supplies `argc` valid pointers in `argv`.
            let pointer = unsafe { *argv.add(usize::try_from(index).unwrap_or(0)) };
            if pointer.is_null() {
                break;
            }
            // SAFETY: `pointer` is a NUL-terminated string owned by the C runtime.
            if let Ok(value) = unsafe { CStr::from_ptr(pointer) }.to_str() {
                values.push(value);
            }
        }
        run(&values)
    }

    fn run(args: &[&str]) -> i32 {
        let options = match parse(args.iter().skip(1).copied()) {
            Ok(options) => options,
            Err(error) => {
                output("curl: ");
                output(error.message());
                output("\n");
                output(usage());
                output("\n");
                return EXIT_USAGE;
            }
        };
        let tls = is_https(&options.url);
        let request = if tls {
            match curl::https_http_url(&options.url) {
                Ok(url) => url,
                Err(error) => {
                    output("curl: ");
                    output(error.message());
                    output("\n");
                    return EXIT_USAGE;
                }
            }
        } else {
            options.url.clone()
        };
        let mut transport = curl::tls::TlsTransport::new(&options.ca_file, tls);
        let clock = SystemClock;
        let response = match fetch_with(&request, &options, &mut transport, &clock) {
            Ok(response) => response,
            Err(error) => {
                if !options.silent {
                    match error {
                        FetchError::HttpStatus(status) => {
                            output("curl: HTTP status ");
                            number(usize::from(status));
                            output("\n");
                        }
                        FetchError::Http(error) => {
                            output("curl: ");
                            output(FetchError::Http(error).message());
                            output("\n");
                        }
                    }
                    if let Some(message) = transport.last_error() {
                        output("curl: TLS: ");
                        output(message);
                        output("\n");
                    }
                }
                return match error {
                    FetchError::HttpStatus(_) => EXIT_HTTP_FAILED,
                    FetchError::Http(_) => EXIT_ERROR,
                };
            }
        };
        let head;
        let bytes: &[u8] = if options.headers_only {
            head = format_head(&response);
            head.as_bytes()
        } else {
            &response.body
        };
        let write = match options.output.as_deref() {
            None | Some("-") => write_stdout(bytes),
            Some(path) => curl::tls::write_file(path, bytes).map_err(|_| ()),
        };
        if write.is_err() {
            if !options.silent {
                output("curl: cannot write output\n");
            }
            return EXIT_ERROR;
        }
        EXIT_OK
    }

    struct SystemClock;

    impl Clock for SystemClock {
        fn now_ms(&self) -> u64 {
            let mut time = TimeSpec {
                seconds: 0,
                nanoseconds: 0,
            };
            // SAFETY: `time` is a valid timespec written by the C call.
            if unsafe { clock_gettime(1, &mut time) } != 0 {
                return 0;
            }
            let seconds = u64::try_from(time.seconds).unwrap_or(0);
            let nanos = u64::try_from(time.nanoseconds).unwrap_or(0);
            seconds
                .saturating_mul(1_000)
                .saturating_add(nanos / 1_000_000)
        }
    }

    fn write_stdout(bytes: &[u8]) -> Result<(), ()> {
        let mut offset = 0usize;
        while offset < bytes.len() {
            let written = debug_write(bytes.get(offset..).unwrap_or(&[]));
            if written == 0 {
                return Err(());
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }

    fn debug_write(bytes: &[u8]) -> usize {
        // raven-abi Syscall::DEBUG_WRITE (0x03), matching the runtime's console sink.
        let mut number: usize = 3;
        let pointer = bytes.as_ptr() as usize;
        let length = bytes.len();
        // SAFETY: DEBUG_WRITE reads `length` bytes from the live slice and returns the count.
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
        number
    }

    fn output(text: &str) {
        let _ = write_stdout(text.as_bytes());
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
        let _ = write_stdout(&digits[index..]);
    }

    fn usage() -> &'static str {
        "usage: curl [-s] [-f] [-I] [-L] [-o FILE] [--max-time SECS] [-H 'Name: value'] [--cacert FILE] URL"
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}

