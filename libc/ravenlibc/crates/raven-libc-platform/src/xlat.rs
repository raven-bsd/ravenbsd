// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Translation tables fixed by `donor-map/11-syscall-platform.md` section 3.12.
//!
//! Every row is a `const` array so the numbers are visible in the source, in the generated
//! headers and in `tests/fixtures/xlat.toml`. The fixture is a hand copy: a documentation edit
//! cannot silently change behaviour, and a source edit fails the fixture test until both sides
//! are updated deliberately.

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;
use raven_proto_fs::{
    OpenFlags, WIRE_OPEN_APPEND, WIRE_OPEN_CREATE, WIRE_OPEN_DIRECTORY, WIRE_OPEN_EXCL,
    WIRE_OPEN_NOFOLLOW, WIRE_OPEN_READ, WIRE_OPEN_TRUNC, WIRE_OPEN_WRITE,
};

/// The Linux access-mode mask (POSIX `O_ACCMODE`); `O_PATH` is handled separately.
pub const ACCMODE_MASK: u32 = 0o3;

/// The outcome of translating Linux `O_*` flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenSpec {
    /// Wire open bits for `OPEN`.
    pub wire: u16,
    /// `O_CLOEXEC` lives in the fd table, not on the wire.
    pub cloexec: bool,
    /// Access mode and the mutable open-file status bits kept in the OFD.
    pub status: u32,
}

/// `O_*` rows as `(name, Linux value, disposition)`.
pub const OPEN_FLAGS: &[(&str, i32, &str)] = &[
    ("O_RDONLY", fcntl::O_RDONLY, "wire READ; status access mode"),
    (
        "O_WRONLY",
        fcntl::O_WRONLY,
        "wire WRITE; status access mode",
    ),
    (
        "O_RDWR",
        fcntl::O_RDWR,
        "wire READ|WRITE; status access mode",
    ),
    ("O_CREAT", fcntl::O_CREAT, "wire CREATE"),
    (
        "O_EXCL",
        fcntl::O_EXCL,
        "wire EXCL; ignored without O_CREAT",
    ),
    (
        "O_TRUNC",
        fcntl::O_TRUNC,
        "wire TRUNC; dropped for read-only opens",
    ),
    ("O_APPEND", fcntl::O_APPEND, "wire APPEND; status"),
    ("O_DIRECTORY", fcntl::O_DIRECTORY, "wire DIRECTORY"),
    ("O_NOFOLLOW", fcntl::O_NOFOLLOW, "wire NOFOLLOW"),
    ("O_CLOEXEC", fcntl::O_CLOEXEC, "fd table only"),
    (
        "O_NONBLOCK",
        fcntl::O_NONBLOCK,
        "status; wire bit 8 after G-12",
    ),
    ("O_SYNC", fcntl::O_SYNC, "status; wire bit 9 after G-12"),
    ("O_DSYNC", fcntl::O_DSYNC, "status; wire bit 10 after G-12"),
    ("O_NOCTTY", fcntl::O_NOCTTY, "ignored"),
    ("O_NOATIME", fcntl::O_NOATIME, "status; ignored on the wire"),
    ("O_LARGEFILE", fcntl::O_LARGEFILE, "ignored"),
    ("O_DIRECT", fcntl::O_DIRECT, "ignored"),
    ("O_ASYNC", fcntl::O_ASYNC, "ignored"),
    ("O_PATH", fcntl::O_PATH, "EOPNOTSUPP until G-12"),
    ("O_TMPFILE", fcntl::O_TMPFILE, "EOPNOTSUPP"),
];

/// Translate Linux-valued `open` flags into the wire set and the OFD status.
///
/// `O_EXCL` without `O_CREAT` is ignored (POSIX leaves the combination undefined and the wire
/// rejects it); `O_TRUNC` on a read-only open is dropped for the same reason. `O_PATH` and
/// `O_TMPFILE` return `EOPNOTSUPP` until PG-24 adds the wire bits.
pub fn open_flags(linux: u32) -> Result<OpenSpec, Errno> {
    let flags = linux;
    if flags & fcntl::O_PATH as u32 != 0 {
        // O_PATH is an alternative open mode entirely; PG-24 adds wire bit 11.
        return Err(Errno::EOPNOTSUPP);
    }
    if flags & fcntl::O_TMPFILE as u32 == fcntl::O_TMPFILE as u32 {
        return Err(Errno::EOPNOTSUPP);
    }
    let access = match flags & ACCMODE_MASK {
        0 => fcntl::O_RDONLY as u32,
        1 => fcntl::O_WRONLY as u32,
        2 => fcntl::O_RDWR as u32,
        _ => return Err(Errno::EINVAL),
    };
    let mut wire = match access {
        0 => WIRE_OPEN_READ,
        1 => WIRE_OPEN_WRITE,
        _ => WIRE_OPEN_READ | WIRE_OPEN_WRITE,
    };
    let mut status = access;
    let create = flags & fcntl::O_CREAT as u32 != 0;
    if create {
        wire |= WIRE_OPEN_CREATE;
    }
    if flags & fcntl::O_EXCL as u32 != 0 && create {
        wire |= WIRE_OPEN_EXCL;
    }
    if flags & fcntl::O_TRUNC as u32 != 0 && access != fcntl::O_RDONLY as u32 {
        wire |= WIRE_OPEN_TRUNC;
    }
    if flags & fcntl::O_APPEND as u32 != 0 {
        wire |= WIRE_OPEN_APPEND;
        status |= fcntl::O_APPEND as u32;
    }
    if flags & fcntl::O_DIRECTORY as u32 != 0 {
        wire |= WIRE_OPEN_DIRECTORY;
    }
    if flags & fcntl::O_NOFOLLOW as u32 != 0 {
        wire |= WIRE_OPEN_NOFOLLOW;
    }
    for (linux_bit, status_bit) in [
        (fcntl::O_NONBLOCK, fcntl::O_NONBLOCK),
        (fcntl::O_SYNC, fcntl::O_SYNC),
        (fcntl::O_DSYNC, fcntl::O_DSYNC),
        (fcntl::O_NOATIME, fcntl::O_NOATIME),
    ] {
        if flags & linux_bit as u32 != 0 {
            status |= status_bit as u32;
        }
    }
    let known = ACCMODE_MASK
        | fcntl::O_CREAT as u32
        | fcntl::O_EXCL as u32
        | fcntl::O_TRUNC as u32
        | fcntl::O_APPEND as u32
        | fcntl::O_DIRECTORY as u32
        | fcntl::O_NOFOLLOW as u32
        | fcntl::O_CLOEXEC as u32
        | fcntl::O_NONBLOCK as u32
        | fcntl::O_SYNC as u32
        | fcntl::O_DSYNC as u32
        | fcntl::O_NOCTTY as u32
        | fcntl::O_NOATIME as u32
        | fcntl::O_LARGEFILE as u32
        | fcntl::O_DIRECT as u32
        | fcntl::O_ASYNC as u32
        | fcntl::O_PATH as u32
        | fcntl::O_TMPFILE as u32;
    if flags & !known != 0 {
        return Err(Errno::EINVAL);
    }
    // O_RSYNC is an alias of O_SYNC in the ABI; nothing extra to do.
    let parsed = OpenFlags::from_wire(wire).map_err(|error| error.errno())?;
    Ok(OpenSpec {
        wire: parsed.to_wire(),
        cloexec: flags & fcntl::O_CLOEXEC as u32 != 0,
        status,
    })
}

/// What `fcntl` does with a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FcntlAction {
    /// Allocate the lowest fd at or above `arg`.
    DupFd,
    /// As [`FcntlAction::DupFd`], with `FD_CLOEXEC`.
    DupFdCloexec,
    /// Read the fd flag word.
    GetFd,
    /// Write the fd flag word.
    SetFd,
    /// Read the OFD status.
    GetFl,
    /// Change the mutable part of the OFD status.
    SetFl,
    /// Query a record lock.
    GetLk,
    /// Set a record lock, non-blocking.
    SetLk,
    /// Set a record lock, blocking.
    SetLkw,
    /// `F_OFD_GETLK`.
    OfdGetLk,
    /// `F_OFD_SETLK`.
    OfdSetLk,
    /// `F_OFD_SETLKW`.
    OfdSetLkw,
    /// Read the pipe capacity.
    GetPipeSz,
    /// Change the pipe capacity.
    SetPipeSz,
    /// Add a memory-object seal.
    AddSeals,
    /// Read the memory-object seals.
    GetSeals,
    /// A command Raven does not implement; `EINVAL`.
    Invalid,
}

/// `fcntl` command rows as `(name, Linux value, action)`.
pub const FCNTL_COMMANDS: &[(&str, i32, FcntlAction)] = &[
    ("F_DUPFD", fcntl::F_DUPFD, FcntlAction::DupFd),
    (
        "F_DUPFD_CLOEXEC",
        fcntl::F_DUPFD_CLOEXEC,
        FcntlAction::DupFdCloexec,
    ),
    ("F_GETFD", fcntl::F_GETFD, FcntlAction::GetFd),
    ("F_SETFD", fcntl::F_SETFD, FcntlAction::SetFd),
    ("F_GETFL", fcntl::F_GETFL, FcntlAction::GetFl),
    ("F_SETFL", fcntl::F_SETFL, FcntlAction::SetFl),
    ("F_GETLK", fcntl::F_GETLK, FcntlAction::GetLk),
    ("F_SETLK", fcntl::F_SETLK, FcntlAction::SetLk),
    ("F_SETLKW", fcntl::F_SETLKW, FcntlAction::SetLkw),
    ("F_OFD_GETLK", fcntl::F_OFD_GETLK, FcntlAction::OfdGetLk),
    ("F_OFD_SETLK", fcntl::F_OFD_SETLK, FcntlAction::OfdSetLk),
    ("F_OFD_SETLKW", fcntl::F_OFD_SETLKW, FcntlAction::OfdSetLkw),
    ("F_GETPIPE_SZ", fcntl::F_GETPIPE_SZ, FcntlAction::GetPipeSz),
    ("F_SETPIPE_SZ", fcntl::F_SETPIPE_SZ, FcntlAction::SetPipeSz),
    ("F_ADD_SEALS", fcntl::F_ADD_SEALS, FcntlAction::AddSeals),
    ("F_GET_SEALS", fcntl::F_GET_SEALS, FcntlAction::GetSeals),
    ("F_GETOWN", fcntl::F_GETOWN, FcntlAction::Invalid),
    ("F_SETOWN", fcntl::F_SETOWN, FcntlAction::Invalid),
    ("F_GETSIG", fcntl::F_GETSIG, FcntlAction::Invalid),
    ("F_SETSIG", fcntl::F_SETSIG, FcntlAction::Invalid),
    ("F_GETOWN_EX", fcntl::F_GETOWN_EX, FcntlAction::Invalid),
    ("F_SETOWN_EX", fcntl::F_SETOWN_EX, FcntlAction::Invalid),
    ("F_SETLEASE", fcntl::F_SETLEASE, FcntlAction::Invalid),
    ("F_GETLEASE", fcntl::F_GETLEASE, FcntlAction::Invalid),
    ("F_NOTIFY", fcntl::F_NOTIFY, FcntlAction::Invalid),
];

/// Classify a Linux `fcntl` command.
pub fn fcntl_action(command: i32) -> FcntlAction {
    let mut index = 0usize;
    while let Some((_, value, action)) = FCNTL_COMMANDS.get(index) {
        if *value == command {
            return *action;
        }
        index = index.saturating_add(1);
    }
    FcntlAction::Invalid
}

/// What `ioctl` does with a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoctlAction {
    /// Read `struct termios` over the tty protocol (`PG-54`).
    TcGets,
    /// Write `struct termios` (`PG-54`).
    TcSets,
    /// Write and drain (`PG-54`).
    TcSetSw,
    /// Write and flush (`PG-54`).
    TcSetSf,
    /// Read the window size (`PG-54`).
    TiocGWinsz,
    /// Bytes available for reading.
    FionRead,
    /// Set `O_NONBLOCK` in the fd table.
    FionBio,
    /// Set `FD_CLOEXEC`.
    Fioclex,
    /// Clear `FD_CLOEXEC`.
    Fionclex,
    /// Unknown request: `ENOTTY`.
    Unknown,
}

/// Linux ioctl request values fixed by doc 11 section 3.2.
pub const TCGETS: u32 = 0x5401;
/// `TCSETS`.
pub const TCSETS: u32 = 0x5402;
/// `TCSETSW`.
pub const TCSETSW: u32 = 0x5403;
/// `TCSETSF`.
pub const TCSETSF: u32 = 0x5404;
/// `TIOCGWINSZ`.
pub const TIOCGWINSZ: u32 = 0x5413;
/// `FIONREAD`.
pub const FIONREAD: u32 = 0x541B;
/// `FIONBIO`.
pub const FIONBIO: u32 = 0x5421;
/// `FIONCLEX`.
pub const FIONCLEX: u32 = 0x5450;
/// `FIOCLEX`.
pub const FIOCLEX: u32 = 0x5451;

/// `ioctl` request rows as `(name, Linux value, action)`.
pub const IOCTL_REQUESTS: &[(&str, u32, IoctlAction)] = &[
    ("TCGETS", TCGETS, IoctlAction::TcGets),
    ("TCSETS", TCSETS, IoctlAction::TcSets),
    ("TCSETSW", TCSETSW, IoctlAction::TcSetSw),
    ("TCSETSF", TCSETSF, IoctlAction::TcSetSf),
    ("TIOCGWINSZ", TIOCGWINSZ, IoctlAction::TiocGWinsz),
    ("FIONREAD", FIONREAD, IoctlAction::FionRead),
    ("FIONBIO", FIONBIO, IoctlAction::FionBio),
    ("FIONCLEX", FIONCLEX, IoctlAction::Fionclex),
    ("FIOCLEX", FIOCLEX, IoctlAction::Fioclex),
];

/// Classify an ioctl request.
pub fn ioctl_action(request: u32) -> IoctlAction {
    match request {
        TCGETS => IoctlAction::TcGets,
        TCSETS => IoctlAction::TcSets,
        TCSETSW => IoctlAction::TcSetSw,
        TCSETSF => IoctlAction::TcSetSf,
        TIOCGWINSZ => IoctlAction::TiocGWinsz,
        FIONREAD => IoctlAction::FionRead,
        FIONBIO => IoctlAction::FionBio,
        FIONCLEX => IoctlAction::Fionclex,
        FIOCLEX => IoctlAction::Fioclex,
        _ => IoctlAction::Unknown,
    }
}

/// Decode a `CALL` response status: `0` is success, `-4095..=-1` an errno, anything else
/// `EPROTO`.
pub fn call_status(status: i32) -> Result<(), Errno> {
    match status {
        0 => Ok(()),
        -4095..=-1 => Err(Errno((-status) as u16)),
        _ => Err(Errno::EPROTO),
    }
}

/// Decode a scheme packet status: a positive errno `0..=4095`.
pub fn packet_status(status: u16) -> Result<(), Errno> {
    if status == 0 {
        Ok(())
    } else if status <= 4095 {
        Err(Errno(status))
    } else {
        Err(Errno::EPROTO)
    }
}

#[cfg(test)]
#[path = "xlat_ut.rs"]
mod tests;
