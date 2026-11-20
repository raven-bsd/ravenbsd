// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thin Rust wrapper over the OpenSSL 3.5 C add-on (DR-45).
//!
//! This crate owns no TLS state machine and implements no cryptography. It exposes a small safe
//! API over the vendored `libssl`/`libcrypto`; certificate and hostname/IP verification are on by
//! default. The caller supplies the connected byte stream through [`Transport`], so the wrapper
//! never opens a socket and does not depend on the still-incomplete libc lazy-socket path.
//!
//! Target binaries link the add-on static libraries and the RavenBSD libc; host tests do not call
//! the C code.

#![cfg_attr(not(test), no_std)]
#![allow(unsafe_code)] // T2: the thin libssl/libcrypto FFI boundary; every block has a SAFETY note.
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

use alloc::boxed::Box;
use core::ffi::{c_char, c_int, c_long, c_void};

mod ffi {
    use super::*;

    pub(crate) type Ssl = c_void;
    pub(crate) type SslCtx = c_void;
    pub(crate) type SslMethod = c_void;
    pub(crate) type Bio = c_void;
    pub(crate) type BioMethod = c_void;

    /// `BIO_TYPE_SOURCE_SINK`; a custom BIO that is neither a descriptor nor a filter.
    pub(crate) const BIO_TYPE_SOURCE_SINK: c_int = 0x0400;
    /// `BIO_CTRL_FLUSH`.
    pub(crate) const BIO_CTRL_FLUSH: c_int = 11;

    pub(crate) const SSL_VERIFY_PEER: c_int = 1;
    pub(crate) const X509_V_OK: c_long = 0;
    pub(crate) const SSL_CTRL_SET_TLSEXT_HOSTNAME: c_int = 55;
    pub(crate) const TLSEXT_NAMETYPE_HOST_NAME: c_long = 0;

    pub(crate) type BioRead =
        Option<unsafe extern "C" fn(bio: *mut Bio, out: *mut c_char, len: c_int) -> c_int>;
    pub(crate) type BioWrite =
        Option<unsafe extern "C" fn(bio: *mut Bio, data: *const c_char, len: c_int) -> c_int>;
    pub(crate) type BioCtrl = Option<
        unsafe extern "C" fn(
            bio: *mut Bio,
            command: c_int,
            large: c_long,
            pointer: *mut c_void,
        ) -> c_long,
    >;
    pub(crate) type BioCreate = Option<unsafe extern "C" fn(bio: *mut Bio) -> c_int>;

    pub(crate) const SSL_ERROR_NONE: c_int = 0;
    pub(crate) const SSL_ERROR_WANT_READ: c_int = 2;
    pub(crate) const SSL_ERROR_WANT_WRITE: c_int = 3;
    pub(crate) const SSL_ERROR_SYSCALL: c_int = 5;
    pub(crate) const SSL_ERROR_ZERO_RETURN: c_int = 6;

    unsafe extern "C" {
        pub(crate) fn OPENSSL_init_ssl(options: u64, settings: *const c_void) -> c_int;
        pub(crate) fn TLS_client_method() -> *const SslMethod;
        pub(crate) fn SSL_CTX_new(method: *const SslMethod) -> *mut SslCtx;
        pub(crate) fn SSL_CTX_free(ctx: *mut SslCtx);
        pub(crate) fn SSL_CTX_set_verify(
            ctx: *mut SslCtx,
            mode: c_int,
            callback: Option<unsafe extern "C" fn(c_int, *mut c_void) -> c_int>,
        );
        pub(crate) fn SSL_CTX_load_verify_locations(
            ctx: *mut SslCtx,
            ca_file: *const c_char,
            ca_path: *const c_char,
        ) -> c_int;
        pub(crate) fn SSL_new(ctx: *mut SslCtx) -> *mut Ssl;
        pub(crate) fn SSL_free(ssl: *mut Ssl);
        pub(crate) fn SSL_set_bio(ssl: *mut Ssl, rbio: *mut Bio, wbio: *mut Bio);
        pub(crate) fn SSL_connect(ssl: *mut Ssl) -> c_int;
        pub(crate) fn SSL_get_verify_result(ssl: *const Ssl) -> c_long;
        pub(crate) fn SSL_get0_param(ssl: *mut Ssl) -> *mut c_void;
        pub(crate) fn SSL_ctrl(
            ssl: *mut Ssl,
            command: c_int,
            large: c_long,
            pointer: *mut c_void,
        ) -> c_long;
        pub(crate) fn X509_VERIFY_PARAM_set1_ip_asc(param: *mut c_void, ip: *const c_char)
        -> c_int;
        pub(crate) fn SSL_write(ssl: *mut Ssl, buffer: *const c_void, length: c_int) -> c_int;
        pub(crate) fn SSL_read(ssl: *mut Ssl, buffer: *mut c_void, length: c_int) -> c_int;
        pub(crate) fn SSL_get_error(ssl: *const Ssl, ret: c_int) -> c_int;
        pub(crate) fn SSL_shutdown(ssl: *mut Ssl) -> c_int;

        pub(crate) fn BIO_meth_new(type_: c_int, name: *const c_char) -> *mut BioMethod;
        pub(crate) fn BIO_meth_free(method: *mut BioMethod);
        pub(crate) fn BIO_meth_set_read(method: *mut BioMethod, read: BioRead) -> c_int;
        pub(crate) fn BIO_meth_set_write(method: *mut BioMethod, write: BioWrite) -> c_int;
        pub(crate) fn BIO_meth_set_ctrl(method: *mut BioMethod, ctrl: BioCtrl) -> c_int;
        pub(crate) fn BIO_meth_set_create(method: *mut BioMethod, create: BioCreate) -> c_int;
        pub(crate) fn BIO_meth_set_destroy(method: *mut BioMethod, destroy: BioCreate) -> c_int;
        pub(crate) fn BIO_new(method: *const BioMethod) -> *mut Bio;
        pub(crate) fn BIO_set_data(bio: *mut Bio, data: *mut c_void);
        pub(crate) fn BIO_get_data(bio: *mut Bio) -> *mut c_void;
        pub(crate) fn BIO_set_init(bio: *mut Bio, init: c_int);
    }
}

/// A fatal transport error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportError;

/// A connected byte stream for the TLS handshake and application data.
///
/// The transport is supplied by the caller because the target network path is the `net:` scheme
/// (the libc lazy-socket path is not complete yet). A read returning `Ok(0)` is an orderly close;
/// `Err(TransportError)` is a fatal transport error. The wrapper does not retry; a blocking
/// transport should wait for data before returning.
pub trait Transport {
    /// Read up to `buffer.len()` bytes.
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, TransportError>;
    /// Write `buffer` bytes.
    fn write(&mut self, buffer: &[u8]) -> Result<usize, TransportError>;
}

struct TransportHolder {
    transport: Box<dyn Transport>,
}

/// A TLS failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The host name is too long for the fixed C buffer.
    HostTooLong,
    /// The CA path is too long for the fixed C buffer.
    CaPathTooLong,
    /// OpenSSL context creation failed.
    Context,
    /// Loading the CA bundle failed.
    CaBundle,
    /// A custom BIO could not be created (out of memory or an OpenSSL setup failure).
    Bio,
    /// TLS handshake failed.
    Handshake,
    /// The peer certificate did not verify.
    Verify(c_long),
    /// A read or write failed.
    Io,
}

/// One verified TLS connection.
pub struct Connection {
    ssl: *mut ffi::Ssl,
    ctx: *mut ffi::SslCtx,
    method: *mut ffi::BioMethod,
    holder: *mut TransportHolder,
}

impl Connection {
    /// Write one plaintext buffer through TLS.
    pub fn write(&mut self, bytes: &[u8]) -> Result<usize, Error> {
        let length = c_int::try_from(bytes.len()).map_err(|_| Error::Io)?;
        // SAFETY: `ssl` is a live SSL object owned by this connection and `bytes` is a valid
        // buffer for `length` bytes; SSL_write reads it.
        let written = unsafe { ffi::SSL_write(self.ssl, bytes.as_ptr().cast(), length) };
        if written <= 0 {
            return Err(Error::Io);
        }
        Ok(written as usize)
    }

    /// Read plaintext through TLS.
    pub fn read(&mut self, bytes: &mut [u8]) -> Result<usize, Error> {
        let length = c_int::try_from(bytes.len()).map_err(|_| Error::Io)?;
        loop {
            // SAFETY: `ssl` is live and `bytes` is a valid writable buffer for `length` bytes.
            let read = unsafe { ffi::SSL_read(self.ssl, bytes.as_mut_ptr().cast(), length) };
            if read > 0 {
                return Ok(read as usize);
            }
            // SAFETY: `ssl` is live and `read` is the return value of the SSL_read above.
            let error = unsafe { ffi::SSL_get_error(self.ssl, read) };
            match error {
                // A clean close_notify or a zero-length read is EOF.
                ffi::SSL_ERROR_ZERO_RETURN | ffi::SSL_ERROR_NONE => return Ok(0),
                // The BIO is blocking, but OpenSSL can still request a retry after a control
                // record (for example a session ticket); retry the read instead of failing.
                ffi::SSL_ERROR_WANT_READ | ffi::SSL_ERROR_WANT_WRITE => continue,
                // A syscall error with a zero return is the peer closing without close_notify;
                // treat it as EOF, never as a truncated response.
                ffi::SSL_ERROR_SYSCALL if read == 0 => return Ok(0),
                _ => return Err(Error::Io),
            }
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // SAFETY: the pointers belong to this connection and are released exactly once. SSL_free
        // releases the BIO, so the BIO method and the transport holder are safe to free after it.
        unsafe {
            if !self.ssl.is_null() {
                let _ = ffi::SSL_shutdown(self.ssl);
                ffi::SSL_free(self.ssl);
            }
            if !self.ctx.is_null() {
                ffi::SSL_CTX_free(self.ctx);
            }
            if !self.method.is_null() {
                ffi::BIO_meth_free(self.method);
            }
            if !self.holder.is_null() {
                drop(Box::from_raw(self.holder));
            }
        }
    }
}

/// Run a certificate- and hostname-verified TLS handshake over `transport`, trusting `ca_file`.
///
/// The transport must already be connected to the peer named by `host`; `host` is used for SNI
/// and for certificate name verification. The caller must run this on a thread that can block in
/// the transport.
pub fn connect(
    transport: Box<dyn Transport>,
    host: &str,
    ca_file: &str,
) -> Result<Connection, Error> {
    let host_c = nul(host).ok_or(Error::HostTooLong)?;
    let ca_c = nul(ca_file).ok_or(Error::CaPathTooLong)?;
    let holder = Box::into_raw(Box::new(TransportHolder { transport }));

    // SAFETY: the C calls take pointers to the NUL-terminated buffers above; every allocated
    // object is released on the error path that owns it. `holder` stays alive until the return
    // value owns it or the error path drops it.
    unsafe {
        if ffi::OPENSSL_init_ssl(0, core::ptr::null()) != 1 {
            drop(Box::from_raw(holder));
            return Err(Error::Context);
        }
        let method = ffi::TLS_client_method();
        if method.is_null() {
            drop(Box::from_raw(holder));
            return Err(Error::Context);
        }
        let ctx = ffi::SSL_CTX_new(method);
        if ctx.is_null() {
            drop(Box::from_raw(holder));
            return Err(Error::Context);
        }
        ffi::SSL_CTX_set_verify(ctx, ffi::SSL_VERIFY_PEER, None);
        if ffi::SSL_CTX_load_verify_locations(ctx, ca_c.as_ptr(), core::ptr::null()) != 1 {
            ffi::SSL_CTX_free(ctx);
            drop(Box::from_raw(holder));
            return Err(Error::CaBundle);
        }
        let ssl = ffi::SSL_new(ctx);
        if ssl.is_null() {
            ffi::SSL_CTX_free(ctx);
            drop(Box::from_raw(holder));
            return Err(Error::Context);
        }
        if !set_verification_name(ssl, host, &host_c) {
            ffi::SSL_free(ssl);
            ffi::SSL_CTX_free(ctx);
            drop(Box::from_raw(holder));
            return Err(Error::Handshake);
        }
        let bio_method = make_bio_method();
        if bio_method.is_null() {
            ffi::SSL_free(ssl);
            ffi::SSL_CTX_free(ctx);
            drop(Box::from_raw(holder));
            return Err(Error::Bio);
        }
        let bio = ffi::BIO_new(bio_method);
        if bio.is_null() {
            ffi::BIO_meth_free(bio_method);
            ffi::SSL_free(ssl);
            ffi::SSL_CTX_free(ctx);
            drop(Box::from_raw(holder));
            return Err(Error::Bio);
        }
        ffi::BIO_set_data(bio, holder.cast());
        ffi::SSL_set_bio(ssl, bio, bio);
        if ffi::SSL_connect(ssl) != 1 {
            ffi::SSL_free(ssl);
            ffi::SSL_CTX_free(ctx);
            ffi::BIO_meth_free(bio_method);
            drop(Box::from_raw(holder));
            return Err(Error::Handshake);
        }
        let verified = ffi::SSL_get_verify_result(ssl);
        if verified != ffi::X509_V_OK {
            ffi::SSL_free(ssl);
            ffi::SSL_CTX_free(ctx);
            ffi::BIO_meth_free(bio_method);
            drop(Box::from_raw(holder));
            return Err(Error::Verify(verified));
        }
        Ok(Connection {
            ssl,
            ctx,
            method: bio_method,
            holder,
        })
    }
}

/// Set SNI and the expected verification name. Returns false when OpenSSL rejected the name.
fn set_verification_name(ssl: *mut ffi::Ssl, host: &str, host_c: &[c_char; 256]) -> bool {
    // SAFETY: `ssl` is live and `host_c` is NUL terminated.
    unsafe {
        if is_ipv4(host) {
            let param = ffi::SSL_get0_param(ssl);
            !param.is_null() && ffi::X509_VERIFY_PARAM_set1_ip_asc(param, host_c.as_ptr()) == 1
        } else {
            ffi::SSL_ctrl(
                ssl,
                ffi::SSL_CTRL_SET_TLSEXT_HOSTNAME,
                ffi::TLSEXT_NAMETYPE_HOST_NAME,
                host_c.as_ptr() as *mut c_void,
            ) > 0
                && ssl_set1_host(ssl, host_c.as_ptr()) == 1
        }
    }
}

unsafe extern "C" {
    /// `SSL_set1_host` is exported by libssl 3.x (the public macro expands to this function).
    #[link_name = "SSL_set1_host"]
    fn ssl_set1_host(ssl: *mut ffi::Ssl, host: *const c_char) -> c_int;
}

unsafe fn make_bio_method() -> *mut ffi::BioMethod {
    // SAFETY: the C constructor copies the static name; every setter failure frees the method.
    unsafe {
        let method = ffi::BIO_meth_new(ffi::BIO_TYPE_SOURCE_SINK, c"raven-tls".as_ptr());
        if method.is_null() {
            return core::ptr::null_mut();
        }
        let configured = ffi::BIO_meth_set_read(method, Some(bio_read)) == 1
            && ffi::BIO_meth_set_write(method, Some(bio_write)) == 1
            && ffi::BIO_meth_set_ctrl(method, Some(bio_ctrl)) == 1
            && ffi::BIO_meth_set_create(method, Some(bio_create)) == 1
            && ffi::BIO_meth_set_destroy(method, Some(bio_destroy)) == 1;
        if !configured {
            ffi::BIO_meth_free(method);
            return core::ptr::null_mut();
        }
        method
    }
}

unsafe extern "C" fn bio_read(bio: *mut ffi::Bio, out: *mut c_char, len: c_int) -> c_int {
    if bio.is_null() || out.is_null() || len <= 0 {
        return 0;
    }
    // SAFETY: `bio` belongs to the live SSL object and carries the holder pointer set below.
    let data = unsafe { ffi::BIO_get_data(bio) };
    if data.is_null() {
        return -1;
    }
    // SAFETY: the data pointer is the `TransportHolder` owned by the SSL's Connection.
    let holder = unsafe { &mut *data.cast::<TransportHolder>() };
    // SAFETY: OpenSSL passes a writable buffer of `len` bytes.
    let buffer = unsafe { core::slice::from_raw_parts_mut(out.cast::<u8>(), len as usize) };
    match holder.transport.read(buffer) {
        Ok(count) => c_int::try_from(count.min(len as usize)).unwrap_or(c_int::MAX),
        Err(TransportError) => -1,
    }
}

unsafe extern "C" fn bio_write(bio: *mut ffi::Bio, data: *const c_char, len: c_int) -> c_int {
    if bio.is_null() || data.is_null() || len <= 0 {
        return 0;
    }
    // SAFETY: `bio` belongs to the live SSL object and carries the holder pointer set below.
    let holder_data = unsafe { ffi::BIO_get_data(bio) };
    if holder_data.is_null() {
        return -1;
    }
    // SAFETY: the data pointer is the `TransportHolder` owned by the SSL's Connection.
    let holder = unsafe { &mut *holder_data.cast::<TransportHolder>() };
    // SAFETY: OpenSSL passes a readable buffer of `len` bytes.
    let buffer = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), len as usize) };
    match holder.transport.write(buffer) {
        Ok(count) if count > 0 => c_int::try_from(count.min(len as usize)).unwrap_or(c_int::MAX),
        Ok(_) | Err(TransportError) => -1,
    }
}

unsafe extern "C" fn bio_ctrl(
    _bio: *mut ffi::Bio,
    command: c_int,
    _large: c_long,
    _pointer: *mut c_void,
) -> c_long {
    if command == ffi::BIO_CTRL_FLUSH { 1 } else { 0 }
}

unsafe extern "C" fn bio_create(bio: *mut ffi::Bio) -> c_int {
    if bio.is_null() {
        return 0;
    }
    // SAFETY: the BIO is freshly allocated by BIO_new.
    unsafe { ffi::BIO_set_init(bio, 1) };
    1
}

unsafe extern "C" fn bio_destroy(bio: *mut ffi::Bio) -> c_int {
    if bio.is_null() {
        return 0;
    }
    // SAFETY: the BIO is being released by BIO_free.
    unsafe { ffi::BIO_set_init(bio, 0) };
    1
}

fn nul(text: &str) -> Option<[c_char; 256]> {
    let bytes = text.as_bytes();
    if bytes.len() >= 255 || bytes.contains(&0) {
        return None;
    }
    let mut buffer = [0; 256];
    for (slot, byte) in buffer.iter_mut().zip(bytes.iter()) {
        *slot = *byte as c_char;
    }
    Some(buffer)
}

fn is_ipv4(text: &str) -> bool {
    let mut parts = 0;
    for part in text.split('.') {
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        let Ok(value) = part.parse::<u16>() else {
            return false;
        };
        if value > 255 {
            return false;
        }
        parts += 1;
    }
    parts == 4
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
