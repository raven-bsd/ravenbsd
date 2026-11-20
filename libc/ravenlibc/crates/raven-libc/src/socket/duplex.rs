// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The target `AF_UNIX` backend over the kernel duplex endpoint pair (K208).
//!
//! The kernel returns two ordinary handles; they go straight into the existing one-handle-per-OFD
//! table, so `dup`, `fork`, `close` and `poll` keep their normal semantics. This module owns only
//! the pair flag word and the shutdown selector.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::socket::SOCK_DGRAM;
use raven_libc_net::ErrnoExt;
use raven_libc_net::Sock;
use raven_libc_net::unix::{UnixBackend, UnixShutdown};
use raven_libc_sys::arch;
use raven_libc_sys::errno::ret;

/// Raw `duplex_pair(out: *mut [u32; 2], flags: u32) -> 0` (K208; not yet in the ABI crate).
const DUPLEX_PAIR: usize = 0x110;
/// Raw `duplex_shutdown(handle: u32, how: u32) -> 0` (K208; not yet in the ABI crate).
const DUPLEX_SHUTDOWN: usize = 0x111;
/// Message-preserving datagram mode.
const DUPLEX_DATAGRAM: u32 = 1;
/// `O_NONBLOCK` from the native descriptor flags.
const DUPLEX_O_NONBLOCK: u32 = 0o4000;
/// `O_CLOEXEC` from the native descriptor flags.
const DUPLEX_O_CLOEXEC: u32 = 0o2000000;

/// Build the kernel pair flag word from the libc socket type and descriptor flags.
pub(crate) fn duplex_flags(ty: i32, nonblock: bool, cloexec: bool) -> u32 {
    let mut flags = 0;
    if ty == SOCK_DGRAM {
        flags |= DUPLEX_DATAGRAM;
    }
    if nonblock {
        flags |= DUPLEX_O_NONBLOCK;
    }
    if cloexec {
        flags |= DUPLEX_O_CLOEXEC;
    }
    flags
}

/// The target backend: one duplex endpoint handle per `AF_UNIX` socket.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ShellUnix;

impl UnixBackend for ShellUnix {
    fn pair(&self, ty: i32) -> Result<(Sock, Sock), Errno> {
        self.pair_flags(ty, false, false)
    }

    fn pair_flags(&self, ty: i32, nonblock: bool, cloexec: bool) -> Result<(Sock, Sock), Errno> {
        let flags = duplex_flags(ty, nonblock, cloexec);
        let mut handles = [0u32; 2];
        // SAFETY: `handles` is a live two-word array and the kernel writes exactly eight bytes
        // into it. The flags word is scalar and validated by the kernel.
        let raw =
            unsafe { arch::syscall2(DUPLEX_PAIR, handles.as_mut_ptr() as usize, flags as usize) };
        ret(raw).map_err(|error| Errno(error.0))?;
        Ok((Sock(handles[0]), Sock(handles[1])))
    }

    fn bind(&self, _path: &[u8]) -> Result<Sock, Errno> {
        Err(Errno::EAFNOSUPPORT)
    }

    fn connect(&self, _path: &[u8]) -> Result<Sock, Errno> {
        Err(Errno::EAFNOSUPPORT)
    }

    fn shutdown(&self, sock: Sock, how: UnixShutdown) -> Result<(), Errno> {
        // SAFETY: both arguments are scalar handle/selector values; the kernel validates the
        // handle rights and the selector.
        let raw = unsafe {
            arch::syscall2(
                DUPLEX_SHUTDOWN,
                sock.0 as usize,
                usize::from(how.as_posix()),
            )
        };
        ret(raw).map_err(|error| Errno(error.0))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "duplex_ut.rs"]
mod tests;
