// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `wget` utility.
//!
//! The base build downloads plain HTTP through `raven-http`/`raven-http-net`; the `tls` add-on
//! build links `raven-tls` (OpenSSL) for certificate- and hostname-verified HTTPS.

#![cfg_attr(any(target_os = "none", target_os = "ravenbsd"), no_std)]
#![cfg_attr(any(target_os = "none", target_os = "ravenbsd"), no_main)]
#![cfg_attr(
    not(all(feature = "tls", any(target_os = "none", target_os = "ravenbsd"))),
    forbid(unsafe_code)
)]

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
extern crate alloc;

#[cfg(all(target_os = "none", not(feature = "tls")))]
mod program {
    use alloc::format;
    use alloc::string::String;

    use raven_abi::types::clock;
    use raven_http::Clock;
    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use wget::{
        EXIT_ERROR, EXIT_OK, EXIT_USAGE, Options, default_filename, fetch_with, parse, summary,
    };

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
        if wget::version_requested(args.iter().skip(1)) {
            raven_rt::println!("{}", wget::VERSION_LINE);
            return EXIT_OK;
        }
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(error) => {
                raven_rt::eprintln!("wget: {}\n{}", error.message(), usage());
                return EXIT_USAGE;
            }
        };
        if curl::is_https(&options.url) {
            if !options.quiet {
                raven_rt::eprintln!(
                    "wget: https unavailable (EUNAVAILABLE): TLS add-on not linked"
                );
            }
            return EXIT_ERROR;
        }
        let mut transport = curl::live_http_transport();
        let clock = SystemClock;
        let response = match fetch_with(&options, &mut transport, &clock) {
            Ok(response) => response,
            Err(error) => {
                if !options.quiet {
                    raven_rt::eprintln!("wget: {}", error.message());
                }
                return EXIT_ERROR;
            }
        };
        let filename = match output_name(&options) {
            Ok(filename) => filename,
            Err(()) => {
                if !options.quiet {
                    raven_rt::eprintln!("wget: invalid output name");
                }
                return EXIT_ERROR;
            }
        };
        if write_output(&filename, &response.body).is_err() {
            if !options.quiet {
                raven_rt::eprintln!("wget: cannot write {filename}");
            }
            return EXIT_ERROR;
        }
        if !options.quiet {
            raven_rt::eprintln!("{}", summary(&options.url, &filename, response.body.len()));
        }
        EXIT_OK
    }

    fn output_name(options: &Options) -> Result<String, ()> {
        match options.output.as_deref() {
            Some(path) => Ok(String::from(path)),
            None => default_filename(&options.url).map_err(|_| ()),
        }
    }

    fn write_output(filename: &str, bytes: &[u8]) -> Result<(), raven_abi::errno::Errno> {
        if filename == "-" {
            return write_stdout(bytes);
        }
        let resolved = raven_rt::fs::resolve(filename);
        let full = if raven_rt::fs::has_scheme(&resolved) {
            resolved
        } else {
            format!("file:{resolved}")
        };
        let mut file = raven_rt::fs::open(&full, O_WRONLY | O_CREAT | O_TRUNC)?;
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

    fn usage() -> &'static str {
        "usage: wget [-q] [-O FILE] [--max-time SECS] [--cacert FILE] URL"
    }
}

#[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "tls"))]
mod tls_program {
    use alloc::string::String;
    use alloc::vec::Vec;
    use core::alloc::{GlobalAlloc, Layout};
    use core::ffi::{CStr, c_char, c_int, c_long, c_void};
    use core::ptr;

    use curl::FetchError;
    use raven_http::Clock;
    use wget::{
        EXIT_ERROR, EXIT_OK, EXIT_USAGE, Options, default_filename, fetch_with, parse, summary,
    };

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
        fn write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
    }

    #[panic_handler]
    fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
        output("wget: panic\n");
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
                output("wget: ");
                output(error.message());
                output("\n");
                output(usage());
                output("\n");
                return EXIT_USAGE;
            }
        };
        let tls = curl::is_https(&options.url);
        let mut transport = curl::tls::TlsTransport::new(&options.ca_file, tls);
        let clock = SystemClock;
        let response = match fetch_with(&options, &mut transport, &clock) {
            Ok(response) => response,
            Err(error) => {
                if !options.quiet {
                    match error {
                        FetchError::HttpStatus(status) => {
                            output("wget: HTTP status ");
                            number(usize::from(status));
                            output("\n");
                        }
                        FetchError::Http(error) => {
                            output("wget: ");
                            output(FetchError::Http(error).message());
                            output("\n");
                        }
                    }
                    if let Some(message) = transport.last_error() {
                        output("wget: TLS: ");
                        output(message);
                        output("\n");
                    }
                }
                return EXIT_ERROR;
            }
        };
        let filename = match output_name(&options) {
            Ok(filename) => filename,
            Err(()) => {
                if !options.quiet {
                    output("wget: invalid output name\n");
                }
                return EXIT_ERROR;
            }
        };
        let written = if filename == "-" {
            write_stdout(&response.body).map_err(|()| ())
        } else {
            curl::tls::write_file(&filename, &response.body).map_err(|_| ())
        };
        if written.is_err() {
            if !options.quiet {
                output("wget: cannot write ");
                output(&filename);
                output("\n");
            }
            return EXIT_ERROR;
        }
        if !options.quiet {
            output(&summary(&options.url, &filename, response.body.len()));
            output("\n");
        }
        EXIT_OK
    }

    fn output_name(options: &Options) -> Result<String, ()> {
        match options.output.as_deref() {
            Some(path) => Ok(String::from(path)),
            None => default_filename(&options.url).map_err(|_| ()),
        }
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
            let slice = bytes.get(offset..).unwrap_or(&[]);
            // SAFETY: fd 1 is the process stdout and `slice` is a live readable buffer.
            let written = unsafe { write(1, slice.as_ptr().cast(), slice.len()) };
            if written > 0 {
                offset = offset.saturating_add(usize::try_from(written).unwrap_or(0));
                continue;
            }
            // The shell-spawned add-on child may have no usable fd 1; the base client writes
            // through DEBUG_WRITE, so use the same console sink instead of losing every line.
            // SAFETY: the kernel DEBUG_WRITE copies `len` readable bytes from `slice` and returns
            // the byte count; the scalar arguments follow the raven-abi convention.
            let debug = unsafe { debug_write(slice.as_ptr(), slice.len()) };
            if debug <= 0 {
                return Err(());
            }
            offset = offset.saturating_add(usize::try_from(debug).unwrap_or(0));
        }
        Ok(())
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn debug_write(bytes: *const u8, len: usize) -> isize {
        let result: isize;
        // SAFETY: DEBUG_WRITE (0x03) takes a readable pointer/length; the kernel validates them.
        unsafe {
            core::arch::asm!(
                "syscall",
                inlateout("rax") 3isize => result,
                in("rdi") bytes,
                in("rsi") len,
                lateout("rcx") _,
                lateout("r11") _,
                options(nostack),
            );
        }
        result
    }

    #[cfg(not(target_arch = "x86_64"))]
    unsafe fn debug_write(_bytes: *const u8, _len: usize) -> isize {
        -1
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
        "usage: wget [-q] [-O FILE] [--max-time SECS] [--cacert FILE] URL"
    }
}

#[cfg(all(not(target_os = "none"), not(feature = "tls")))]
fn main() {}
