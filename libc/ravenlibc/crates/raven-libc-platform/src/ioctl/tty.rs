// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Termios glue over the tty `CALL` protocol (`PG-54`).
//!
//! The pty server carries the complete terminal attribute record in `GET_TERMIOS`/`SET_TERMIOS`
//! (K444); `tcgetattr`/`tcsetattr` round-trip every field. Older tty servers that only answer
//! `GET_MODE`/`SET_MODE` (the console scheme) fall back to the canonical/raw plus echo view and
//! the documented defaults. `tcdrain`, flow control, break and window size still return
//! `ENOSYS` until the protocol grows those operations.

use raven_abi::errno::Errno;
use raven_libc_abi::structs::Termios;
use raven_proto_tty::{self, Request, Response, Settings, TERMIOS_LEN, Termios as WireTermios};

use crate::backend::SysBackend;
use crate::fd::ofd::OfdKind;
use crate::lock::Lock;
use crate::platform::Platform;

/// `ICANON` from `<bits/termios.h>`.
pub const ICANON: u32 = 0o0002;
/// `ECHO` from `<bits/termios.h>`.
pub const ECHO: u32 = 0o0010;
/// `CS8` from `<bits/termios.h>`.
pub const CS8: u32 = 0o0060;
/// `CREAD` from `<bits/termios.h>`.
pub const CREAD: u32 = 0o0200;
/// Index of `VMIN` in `c_cc`.
pub const VMIN: usize = 6;
/// Index of `VTIME` in `c_cc`.
pub const VTIME: usize = 5;
/// The only line speed the current console reports.
pub const B38400: u32 = 0o17;

/// A termios record for a canonical, echoing terminal.
pub fn default_termios() -> Termios {
    termios_from_wire(WireTermios::sane())
}

fn tty_call<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    request: Request,
) -> Result<Response, Errno> {
    let reference = platform.get(fd)?;
    if reference.kind() != OfdKind::Tty {
        return Err(Errno::ENOTTY);
    }
    let mut body = [0u8; raven_proto_tty::CALL_LEN];
    raven_proto_tty::encode_request(request, &mut body);
    let mut response = [0u8; raven_proto_tty::CALL_LEN];
    let count = match platform
        .backend()
        .call(reference.handle(), &body, &mut response)
    {
        Err(Errno::ENOSYS) => return Err(Errno::ENOTTY),
        other => other?,
    };
    if count < raven_proto_tty::CALL_LEN {
        return Err(Errno::EPROTO);
    }
    let decoded = raven_proto_tty::decode_response(response.get(..count).unwrap_or(&[]))
        .map_err(|_| Errno::EPROTO)?;
    Ok(decoded)
}

fn tty_get_termios<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<WireTermios, Errno> {
    let reference = platform.get(fd)?;
    if reference.kind() != OfdKind::Tty {
        return Err(Errno::ENOTTY);
    }
    let mut body = [0u8; TERMIOS_LEN];
    raven_proto_tty::encode_get_termios_request(&mut body);
    let mut response = [0u8; TERMIOS_LEN];
    let count = match platform
        .backend()
        .call(reference.handle(), &body, &mut response)
    {
        Err(Errno::ENOSYS) => return Err(Errno::ENOTTY),
        other => other?,
    };
    if count < TERMIOS_LEN {
        return Err(Errno::EPROTO);
    }
    raven_proto_tty::decode_termios_response(response.get(..count).unwrap_or(&[]))
        .map_err(|_| Errno::EPROTO)
}

fn tty_set_termios<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    termios: &WireTermios,
) -> Result<WireTermios, Errno> {
    let reference = platform.get(fd)?;
    if reference.kind() != OfdKind::Tty {
        return Err(Errno::ENOTTY);
    }
    let mut body = [0u8; TERMIOS_LEN];
    raven_proto_tty::encode_set_termios_request(termios, &mut body);
    let mut response = [0u8; TERMIOS_LEN];
    let count = match platform
        .backend()
        .call(reference.handle(), &body, &mut response)
    {
        Err(Errno::ENOSYS) => return Err(Errno::ENOTTY),
        other => other?,
    };
    if count < TERMIOS_LEN {
        return Err(Errno::EPROTO);
    }
    raven_proto_tty::decode_termios_response(response.get(..count).unwrap_or(&[]))
        .map_err(|_| Errno::EPROTO)
}

/// `tcgetattr`.
pub fn tcgetattr<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<Termios, Errno> {
    match tty_get_termios(platform, fd) {
        Ok(termios) => Ok(termios_from_wire(termios)),
        Err(Errno::ENOTTY) => match tty_call(platform, fd, Request::GetMode)? {
            Response::Mode(settings) => Ok(termios_from_settings(settings)),
            _ => Err(Errno::EPROTO),
        },
        Err(error) => Err(error),
    }
}

/// `tcsetattr`.
pub fn tcsetattr<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    termios: &Termios,
) -> Result<(), Errno> {
    let wire = wire_from_termios(termios);
    match tty_set_termios(platform, fd, &wire) {
        Ok(_) => Ok(()),
        Err(Errno::ENOTTY) => {
            let settings = settings_from(termios);
            match tty_call(platform, fd, Request::SetMode { settings })? {
                Response::Mode(_) => Ok(()),
                _ => Err(Errno::EPROTO),
            }
        }
        Err(error) => Err(error),
    }
}

/// `tcflush`: the tty protocol discards input and reports the byte count.
pub fn tcflush<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    _queue: i32,
) -> Result<(), Errno> {
    match tty_call(platform, fd, Request::Flush)? {
        Response::Flushed { .. } => Ok(()),
        _ => Err(Errno::EPROTO),
    }
}

/// `tcdrain`: `ENOSYS` until PG-54.
pub fn tcdrain<B: SysBackend, L: Lock>(platform: &Platform<B, L>, fd: i32) -> Result<(), Errno> {
    let _ = platform.get(fd)?;
    Err(Errno::ENOSYS)
}

/// `tcflow`: `ENOSYS` until PG-54.
pub fn tcflow<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    _action: i32,
) -> Result<(), Errno> {
    let _ = platform.get(fd)?;
    Err(Errno::ENOSYS)
}

/// `tcsendbreak`: `ENOSYS` until PG-54.
pub fn tcsendbreak<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    _duration: i32,
) -> Result<(), Errno> {
    let _ = platform.get(fd)?;
    Err(Errno::ENOSYS)
}

/// `cfmakeraw`.
pub fn cfmakeraw(termios: &mut Termios) {
    termios.c_iflag = 0;
    termios.c_oflag = 0;
    termios.c_lflag = 0;
    termios.c_cflag &= !CS8;
    termios.c_cflag |= CS8;
    if let Some(slot) = termios.c_cc.get_mut(VMIN) {
        *slot = 1;
    }
    if let Some(slot) = termios.c_cc.get_mut(VTIME) {
        *slot = 0;
    }
}

/// `cfgetispeed`.
pub const fn cfgetispeed(termios: &Termios) -> u32 {
    termios.__c_ispeed
}

/// `cfgetospeed`.
pub const fn cfgetospeed(termios: &Termios) -> u32 {
    termios.__c_ospeed
}

/// `cfsetispeed`.
pub fn cfsetispeed(termios: &mut Termios, speed: u32) -> i32 {
    termios.__c_ispeed = speed;
    0
}

/// `cfsetospeed`.
pub fn cfsetospeed(termios: &mut Termios, speed: u32) -> i32 {
    termios.__c_ospeed = speed;
    0
}

/// `tcgetwinsize`: `ENOSYS` until PG-54.
pub fn tcgetwinsize<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<(), Errno> {
    let _ = platform.get(fd)?;
    Err(Errno::ENOSYS)
}

/// `tcsetwinsize`: `ENOSYS` until PG-54.
pub fn tcsetwinsize<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<(), Errno> {
    let _ = platform.get(fd)?;
    Err(Errno::ENOSYS)
}

fn termios_from_settings(settings: Settings) -> Termios {
    let mut termios = default_termios();
    if settings.mode == raven_proto_tty::Mode::Raw {
        termios.c_lflag = 0;
        termios.c_iflag = 0;
        termios.c_oflag = 0;
    } else {
        termios.c_lflag |= ICANON;
    }
    if settings.echo {
        termios.c_lflag |= ECHO;
    } else {
        termios.c_lflag &= !ECHO;
    }
    termios
}

fn settings_from(termios: &Termios) -> Settings {
    Settings {
        mode: if termios.c_lflag & ICANON != 0 {
            raven_proto_tty::Mode::Canonical
        } else {
            raven_proto_tty::Mode::Raw
        },
        echo: termios.c_lflag & ECHO != 0,
    }
}

fn termios_from_wire(wire: WireTermios) -> Termios {
    let mut termios = Termios::ZERO;
    termios.c_iflag = wire.iflag;
    termios.c_oflag = wire.oflag;
    termios.c_cflag = wire.cflag;
    termios.c_lflag = wire.lflag;
    termios.c_line = wire.line;
    termios.c_cc = wire.cc;
    termios.__c_ispeed = wire.ispeed;
    termios.__c_ospeed = wire.ospeed;
    termios
}

fn wire_from_termios(termios: &Termios) -> WireTermios {
    WireTermios {
        iflag: termios.c_iflag,
        oflag: termios.c_oflag,
        cflag: termios.c_cflag,
        lflag: termios.c_lflag,
        line: termios.c_line,
        cc: termios.c_cc,
        ispeed: termios.__c_ispeed,
        ospeed: termios.__c_ospeed,
    }
}

#[cfg(test)]
#[path = "termios_ut.rs"]
mod tests;
