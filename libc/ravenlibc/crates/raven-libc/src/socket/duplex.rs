// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The target `AF_UNIX` backend over the kernel duplex endpoint pair (K208/K210).
//!
//! The kernel returns two ordinary handles; they go straight into the existing one-handle-per-OFD
//! table, so `dup`, `fork`, `close` and `poll` keep their normal semantics. This module owns the
//! pair flag word, the shutdown selector, the C `SCM_RIGHTS` marshalling and the peer credential
//! query.

use alloc::vec;
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::socket::{MSG_CMSG_CLOEXEC, MSG_NOSIGNAL, SCM_RIGHTS, SOCK_DGRAM, SOL_SOCKET};
use raven_libc_net::ErrnoExt;
use raven_libc_net::Sock;
use raven_libc_net::cmsg;
use raven_libc_net::unix::{UnixBackend, UnixCred, UnixRecv, UnixShutdown};
use raven_libc_platform::Handle;
use raven_libc_platform::backend::SysBackend;
use raven_libc_sys::arch;
use raven_libc_sys::errno::ret;

/// Raw `duplex_pair(out: *mut [u32; 2], flags: u32) -> 0` (K208; not yet in the ABI crate).
const DUPLEX_PAIR: usize = 0x110;
/// Raw `duplex_shutdown(handle: u32, how: u32) -> 0` (K208; not yet in the ABI crate).
const DUPLEX_SHUTDOWN: usize = 0x111;
/// Raw `duplex_sendmsg(handle: u32, msg: *mut DuplexMessage) -> bytes` (K210).
const DUPLEX_SENDMSG: usize = 0x112;
/// Raw `duplex_recvmsg(handle: u32, msg: *mut DuplexMessage) -> bytes` (K210).
const DUPLEX_RECVMSG: usize = 0x113;
/// Raw `duplex_peercred(handle: u32, out: *mut DuplexCred) -> 0` (K210).
const DUPLEX_PEERCRED: usize = 0x114;
/// Message-preserving datagram mode.
const DUPLEX_DATAGRAM: u32 = 1;
/// `O_NONBLOCK` from the native descriptor flags.
const DUPLEX_O_NONBLOCK: u32 = 0o4000;
/// `O_CLOEXEC` from the native descriptor flags.
const DUPLEX_O_CLOEXEC: u32 = 0o2000000;
/// Suppress the kernel `SIGPIPE` queue for one send.
const DUPLEX_MSG_NOSIGNAL: u32 = 0x4000;
/// Maximum handles in one control message.
const DUPLEX_MAX_HANDLES: usize = 16;
/// Maximum raw control bytes.
const DUPLEX_CONTROL_MAX: usize = 256;
/// Raw control record header bytes.
const CONTROL_HEADER: usize = 16;
/// Raw `DuplexMessage` bytes.
const MESSAGE_BYTES: usize = 40;
/// Raw `DuplexCred` bytes.
const CRED_BYTES: usize = 16;
/// `O_RDWR` status recorded on an adopted descriptor.
const ADOPT_STATUS: u32 = 2;

/// The user-visible K210 message header.
#[repr(C)]
struct DuplexMessage {
    data: u64,
    data_len: u64,
    control: u64,
    control_len: u64,
    flags: u32,
    handle_count: u32,
}

const _: () = assert!(core::mem::size_of::<DuplexMessage>() == MESSAGE_BYTES);

/// The user-visible K210 credential record.
#[repr(C)]
struct DuplexCred {
    pid: u64,
    uid: u32,
    gid: u32,
}

const _: () = assert!(core::mem::size_of::<DuplexCred>() == CRED_BYTES);

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
#[derive(Clone, Copy, Default)]
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

    fn unix_sendmsg<B: raven_libc_net::NetBackend + ?Sized>(
        &self,
        _backend: &B,
        sock: Sock,
        data: &[u8],
        control: &[u8],
        flags: i32,
    ) -> Result<usize, Errno> {
        let raw_control = encode_control(control)?;
        let mut message = DuplexMessage {
            data: data.as_ptr() as u64,
            data_len: data.len() as u64,
            control: raw_control.as_ptr() as u64,
            control_len: raw_control.len() as u64,
            flags: if flags & MSG_NOSIGNAL != 0 {
                DUPLEX_MSG_NOSIGNAL
            } else {
                0
            },
            handle_count: 0,
        };
        // SAFETY: the message points at live caller slices; the kernel validates every range and
        // clones only handles the caller may transfer.
        let result = unsafe {
            arch::syscall2(
                DUPLEX_SENDMSG,
                sock.0 as usize,
                (&mut message as *mut DuplexMessage) as usize,
            )
        };
        ret(result).map_err(|error| Errno(error.0))
    }

    fn unix_recvmsg<B: raven_libc_net::NetBackend + ?Sized>(
        &self,
        _backend: &B,
        sock: Sock,
        data: &mut [u8],
        control_capacity: usize,
        flags: i32,
    ) -> Result<UnixRecv, Errno> {
        let capacity = control_capacity.min(DUPLEX_CONTROL_MAX);
        let mut control = vec![0u8; capacity];
        let mut message = DuplexMessage {
            data: data.as_mut_ptr() as u64,
            data_len: data.len() as u64,
            control: control.as_mut_ptr() as u64,
            control_len: capacity as u64,
            flags: 0,
            handle_count: 0,
        };
        // SAFETY: the message points at live caller buffers; the kernel validates them and writes
        // the used control length and installed handle count back into the header.
        let result = unsafe {
            arch::syscall2(
                DUPLEX_RECVMSG,
                sock.0 as usize,
                (&mut message as *mut DuplexMessage) as usize,
            )
        };
        let len = ret(result).map_err(|error| Errno(error.0))?;
        let used = usize::try_from(message.control_len).unwrap_or(0);
        let count = usize::try_from(message.handle_count).unwrap_or(0);
        if count == 0 {
            return Ok(UnixRecv {
                len,
                control: Vec::new(),
            });
        }
        let raw = parse_raw_control(&control, used, count)?;
        let cspace = cmsg::space(count.saturating_mul(4));
        if cspace > control_capacity {
            for handle in &raw {
                close_raw(*handle);
            }
            return Err(Errno::EMSGSIZE);
        }
        let platform = crate::ffi::platform::global();
        let mut fds = Vec::with_capacity(count);
        for (index, handle) in raw.iter().enumerate() {
            match platform.adopt(
                Handle(*handle),
                ADOPT_STATUS,
                None,
                (flags & (MSG_CMSG_CLOEXEC as i32)) != 0,
            ) {
                Ok(fd) => {
                    // A net scheme socket is one data handle plus one readiness doorbell. The
                    // sender's fd table owned both, but `SCM_RIGHTS` carries only the data
                    // handle, so a blocking read on the received TCP fd would see the scheme's
                    // `EAGAIN` and never wait. Re-attach the receiver's own event, exactly as
                    // `socket`/`accept` do. A kernel duplex handle rejects the scheme CALL; the
                    // error is harmless there because its own handle is pollable.
                    //
                    // Donor divergence (recorded for review): NetBSD
                    // `sys/kern/uipc_usrreq.c:unp_internalize` installs the existing socket
                    // object, whose receive queue is directly waitable (`sbwait`). Raven's
                    // scheme ABI splits that object from the client data handle, so the event
                    // handle must be re-created on adoption.
                    // `probe_kind` recognises a tty and otherwise reports `Unknown`; a net
                    // socket is not distinguishable from another scheme handle there. The
                    // `Call::Event` probe is the discriminator: a net socket returns a valid
                    // event, a plain file/duplex handle returns an error that is ignored.
                    if let Ok(reference) = platform.get(fd)
                        && matches!(
                            reference.kind(),
                            raven_libc_platform::OfdKind::Socket
                                | raven_libc_platform::OfdKind::Unknown
                        )
                    {
                        let index = reference.index();
                        let data = reference.handle();
                        drop(reference);
                        if let Ok(event) = raven_libc_net::backend_platform::event_handle(
                            platform.backend(),
                            Sock(data.0),
                        ) {
                            let _ = platform.set_ofd_event(index, event);
                        }
                    }
                    fds.push(fd);
                }
                Err(error) => {
                    close_raw(*handle);
                    for rest in raw.get(index.saturating_add(1)..).unwrap_or(&[]) {
                        close_raw(*rest);
                    }
                    for fd in &fds {
                        let _ = platform.close(*fd);
                    }
                    return Err(Errno(error.0));
                }
            }
        }
        let mut payload = Vec::with_capacity(count.saturating_mul(4));
        for fd in &fds {
            payload.extend_from_slice(&fd.to_ne_bytes());
        }
        let mut ccontrol = vec![0u8; cspace];
        let used_control = cmsg::encode_cmsg(SOL_SOCKET, SCM_RIGHTS, &payload, &mut ccontrol)
            .ok_or(Errno::EMSGSIZE)?;
        Ok(UnixRecv {
            len,
            control: ccontrol.get(..used_control).unwrap_or(&[]).to_vec(),
        })
    }

    fn peercred(&self, sock: Sock) -> Result<UnixCred, Errno> {
        let mut cred = DuplexCred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        // SAFETY: `cred` is a live 16-byte record and the kernel writes exactly that size.
        let result = unsafe {
            arch::syscall2(
                DUPLEX_PEERCRED,
                sock.0 as usize,
                (&mut cred as *mut DuplexCred) as usize,
            )
        };
        ret(result).map_err(|error| Errno(error.0))?;
        Ok(UnixCred {
            pid: cred.pid,
            uid: cred.uid,
            gid: cred.gid,
        })
    }
}

/// Encode the C `SCM_RIGHTS` records as K210 raw control records.
fn encode_control(control: &[u8]) -> Result<Vec<u8>, Errno> {
    if control.is_empty() {
        return Ok(Vec::new());
    }
    let platform = crate::ffi::platform::global();
    let mut out = Vec::new();
    let mut total = 0usize;
    for item in cmsg::iter_cmsgs(control) {
        let item = item.map_err(|error| Errno(error.0))?;
        if item.level != SOL_SOCKET || item.ty != SCM_RIGHTS {
            return Err(Errno::EOPNOTSUPP);
        }
        if !item.data.len().is_multiple_of(4) {
            return Err(Errno::EINVAL);
        }
        let count = item.data.len().checked_div(4).ok_or(Errno::EINVAL)?;
        if count == 0 {
            continue;
        }
        let record = CONTROL_HEADER.saturating_add(count.saturating_mul(4));
        if total.saturating_add(count) > DUPLEX_MAX_HANDLES
            || out.len().saturating_add(record) > DUPLEX_CONTROL_MAX
        {
            return Err(Errno::EMSGSIZE);
        }
        out.extend_from_slice(&(record as u32).to_le_bytes());
        out.extend_from_slice(&(SOL_SOCKET as u32).to_le_bytes());
        out.extend_from_slice(&(SCM_RIGHTS as u32).to_le_bytes());
        out.extend_from_slice(&(count as u32).to_le_bytes());
        for chunk in item.data.chunks_exact(4) {
            let raw: [u8; 4] = chunk.try_into().map_err(|_| Errno::EINVAL)?;
            let fd = i32::from_ne_bytes(raw);
            let reference = platform.get(fd).map_err(|error| Errno(error.0))?;
            out.extend_from_slice(&reference.handle().0.to_le_bytes());
        }
        total = total.saturating_add(count);
    }
    Ok(out)
}

/// Validate one raw control record and return its handle ids.
fn parse_raw_control(control: &[u8], used: usize, count: usize) -> Result<Vec<u32>, Errno> {
    if used < CONTROL_HEADER || used > control.len() {
        return Err(Errno::EPROTO);
    }
    let record = usize::try_from(u32_at(control, 0)?).unwrap_or(usize::MAX);
    let level = u32_at(control, 4)?;
    let kind = u32_at(control, 8)?;
    let stated = usize::try_from(u32_at(control, 12)?).unwrap_or(usize::MAX);
    if record < CONTROL_HEADER
        || record > used
        || record != CONTROL_HEADER.saturating_add(stated.saturating_mul(4))
        || stated != count
        || level != SOL_SOCKET as u32
        || kind != SCM_RIGHTS as u32
    {
        return Err(Errno::EPROTO);
    }
    let mut handles = Vec::with_capacity(count);
    for index in 0..count {
        handles.push(u32_at(
            control,
            CONTROL_HEADER.saturating_add(index.saturating_mul(4)),
        )?);
    }
    Ok(handles)
}

/// Close a raw kernel handle that was not installed as an fd.
fn close_raw(raw: u32) {
    let _ = crate::ffi::platform::global().backend().close(Handle(raw));
}

/// Read one little-endian `u32` from a bounded offset.
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, Errno> {
    let slice = bytes
        .get(offset..offset.saturating_add(4))
        .ok_or(Errno::EPROTO)?;
    let array: [u8; 4] = slice.try_into().map_err(|_| Errno::EPROTO)?;
    Ok(u32::from_le_bytes(array))
}

#[cfg(test)]
#[path = "duplex_ut.rs"]
mod tests;
