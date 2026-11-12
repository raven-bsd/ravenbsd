// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host TAP device for the C22 interoperability harness.
//!
//! This is the only `std`/I/O module in Carl's network work. It is isolated in
//! this host-only crate and is never a dependency of `raven-tcpip`.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use raven_tcpip::{ChecksumOffload, Device, DeviceCaps, Instant, Medium, TxError};

/// An Ethernet TAP device.
pub struct TapDevice {
    file: File,
    name: String,
    mtu: usize,
    hardware_addr: [u8; 6],
}

impl TapDevice {
    /// Attach to an existing TAP interface.
    ///
    /// The caller must have created the interface and granted access; this
    /// function never raises privileges.
    pub fn open(name: &str, mtu: usize, hardware_addr: [u8; 6]) -> std::io::Result<Self> {
        if !Path::new("/dev/net/tun").exists() {
            return Err(std::io::Error::new(
                ErrorKind::NotFound,
                "/dev/net/tun is missing; load the tun module",
            ));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/net/tun")?;
        let mut request = IfReq::new(name)?;
        // SAFETY: TUNSETIFF takes a pointer to a valid ifreq whose lifetime
        // covers the call; the fd is open and owned by `file`.
        let result =
            unsafe { libc::ioctl(file.as_raw_fd(), libc::TUNSETIFF, request.as_mut_ptr()) };
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            file,
            name: name.to_string(),
            mtu,
            hardware_addr,
        })
    }

    /// The interface name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

struct IfReq {
    storage: [u8; 40],
}

impl IfReq {
    fn new(name: &str) -> std::io::Result<Self> {
        let bytes = name.as_bytes();
        if bytes.len() >= libc::IFNAMSIZ {
            return Err(std::io::Error::new(
                ErrorKind::InvalidInput,
                "interface name is too long",
            ));
        }
        let mut request = Self { storage: [0u8; 40] };
        request.storage[..bytes.len()].copy_from_slice(bytes);
        let flags = libc::IFF_TAP | libc::IFF_NO_PI;
        let offset = libc::IFNAMSIZ;
        request.storage[offset..offset + 2].copy_from_slice(&(flags as u16).to_ne_bytes());
        Ok(request)
    }

    fn as_mut_ptr(&mut self) -> *mut libc::c_void {
        self.storage.as_mut_ptr().cast()
    }
}

impl Device for TapDevice {
    fn capabilities(&self) -> DeviceCaps {
        DeviceCaps {
            medium: Medium::Ethernet,
            mtu: self.mtu,
            hardware_addr: Some(self.hardware_addr),
            checksum_offload: ChecksumOffload::None,
        }
    }

    fn receive(&mut self, _now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        let mut buffer = [0u8; 65_536];
        match self.file.read(&mut buffer) {
            Ok(0) => false,
            Ok(length) => {
                consume(&buffer[..length]);
                true
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => false,
            Err(_) => false,
        }
    }

    fn transmit(
        &mut self,
        _now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        if len > self.mtu {
            return Err(TxError::TooLong);
        }
        let mut frame = vec![0u8; len];
        fill(&mut frame);
        match self.file.write(&frame) {
            Ok(written) if written == len => Ok(()),
            Ok(_) => Err(TxError::Busy),
            Err(error) if error.kind() == ErrorKind::WouldBlock => Err(TxError::Busy),
            Err(_) => Err(TxError::Down),
        }
    }
}

/// Describe why TAP is unavailable without trying to acquire privileges.
#[must_use]
pub fn tap_skip_reason() -> Option<String> {
    match std::env::var("RAVEN_TAP") {
        Err(_) => Some(
            "RAVEN_TAP is not set; create the TAP interface as described in INTEROP.md".to_string(),
        ),
        Ok(name) => match TapDevice::open(&name, 1500, [0x02, 0, 0, 0, 0, 0x99]) {
            Ok(_) => None,
            Err(error) => Some(format!(
                "cannot open TAP {name}: {error}; see INTEROP.md (no privilege is acquired by tests)"
            )),
        },
    }
}
