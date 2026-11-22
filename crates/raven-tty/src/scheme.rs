// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Console raw mode through the first-party `consd` tty scheme.
//!
//! The console is a scheme handle (`tty:console`), not a POSIX kernel tty: `tcgetattr(0)` returns
//! `ENOTTY` on the handle passed by `rsh`. `consd` already owns the line discipline and exposes
//! `GET_MODE`/`SET_MODE` in `raven-proto-tty` (used by `login` and `talk`). This guard saves the
//! canonical/echo settings and switches the console to raw/echo-off through that protocol.

use core::cell::UnsafeCell;
use core::ffi::c_int;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use raven_abi::types::Handle;
use raven_proto_fs::{WIRE_OPEN_READ, WIRE_OPEN_WRITE};
use raven_proto_tty::{
    CALL_LEN, Mode, Request, Response, Settings, decode_response, encode_request,
};
use raven_scheme::Client;

use crate::Error;

const SIGHUP: c_int = 1;
const SIGINT: c_int = 2;
const SIGQUIT: c_int = 3;
const SIGTERM: c_int = 15;

unsafe extern "C" {
    fn signal(signum: c_int, handler: usize) -> usize;
    fn _exit(code: c_int) -> !;
}

struct Saved(UnsafeCell<Settings>);

// SAFETY: only the enable/drop path and the signal handler touch the saved settings; `ACTIVE`
// and `HANDLE` are atomics and the console has one raw-mode owner.
unsafe impl Sync for Saved {}

static SAVED: Saved = Saved(UnsafeCell::new(Settings {
    mode: Mode::Canonical,
    echo: true,
}));
static ACTIVE: AtomicBool = AtomicBool::new(false);
static HANDLE: AtomicU32 = AtomicU32::new(0);

/// A console scheme in raw mode. Dropping it restores the saved canonical/echo settings.
pub struct SchemeRawMode {
    handle: Handle,
    original: Settings,
    handlers: [(c_int, usize); 4],
}

impl SchemeRawMode {
    /// Save the console settings and switch an existing handle to raw/echo-off.
    pub fn enable(handle: u32) -> Result<Self, Error> {
        let handle = Handle(handle);
        if ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(Error::Busy);
        }
        let original = match request(handle, Request::GetMode) {
            Ok(settings) => settings,
            Err(()) => {
                ACTIVE.store(false, Ordering::SeqCst);
                return Err(Error::GetMode);
            }
        };
        let raw = Settings {
            mode: Mode::Raw,
            echo: false,
        };
        match request(handle, Request::SetMode { settings: raw }) {
            Ok(applied) if applied == raw => {}
            _ => {
                ACTIVE.store(false, Ordering::SeqCst);
                return Err(Error::SetMode);
            }
        }
        // SAFETY: the guard claims ACTIVE before writing SAVED; the handler is installed after.
        unsafe {
            *SAVED.0.get() = original;
        }
        HANDLE.store(handle.0, Ordering::SeqCst);
        let mut handlers = [(0, 0); 4];
        for (slot, number) in handlers.iter_mut().zip([SIGHUP, SIGINT, SIGQUIT, SIGTERM]) {
            // SAFETY: the handler has the C signal ABI and only issues the console SET_MODE call
            // and `_exit`.
            let previous = unsafe { signal(number, restore_on_signal as *const () as usize) };
            *slot = (number, previous);
        }
        Ok(Self {
            handle,
            original,
            handlers,
        })
    }

    /// Restore the console now and consume the guard.
    pub fn restore(self) {
        drop(self);
    }
}

impl Drop for SchemeRawMode {
    fn drop(&mut self) {
        for (number, previous) in self.handlers {
            if previous != usize::MAX {
                // SAFETY: restoring the handler value returned by `signal`.
                let _ = unsafe { signal(number, previous) };
            }
        }
        let _ = request(
            self.handle,
            Request::SetMode {
                settings: self.original,
            },
        );
        ACTIVE.store(false, Ordering::SeqCst);
    }
}

fn request(handle: Handle, request: Request) -> Result<Settings, ()> {
    let mut body = [0u8; CALL_LEN];
    encode_request(request, &mut body);
    let mut response = [0u8; CALL_LEN];
    let written = Client::new()
        .call(handle, &body, &mut response)
        .map_err(|_| ())?;
    if written < CALL_LEN {
        return Err(());
    }
    match decode_response(&response) {
        Ok(Response::Mode(settings)) => Ok(settings),
        _ => Err(()),
    }
}

extern "C" fn restore_on_signal(signum: c_int) {
    // SAFETY: the handler reads the atomics and the saved settings, issues one SET_MODE call and
    // exits without unwinding or allocating.
    unsafe {
        if ACTIVE.load(Ordering::SeqCst) {
            let handle = Handle(HANDLE.load(Ordering::SeqCst));
            let settings = *SAVED.0.get();
            let _ = request(handle, Request::SetMode { settings });
        }
        _exit(128 + signum);
    }
}

/// The `tty:console` handle used for TUI input and raw-mode control.
pub struct Console {
    client: Client,
    handle: Handle,
    raw: Option<SchemeRawMode>,
}

impl Console {
    /// Open `tty:console` for reading and raw-mode calls.
    pub fn open() -> Result<Self, Error> {
        let flags = u32::from(WIRE_OPEN_READ | WIRE_OPEN_WRITE);
        let handle = Client::new()
            .open("tty:console", flags)
            .map_err(|_| Error::NoConsole)?;
        Ok(Self {
            client: Client::new(),
            handle,
            raw: None,
        })
    }

    /// Read console bytes.
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        self.client
            .read(self.handle, 0, buffer)
            .map_err(|_| Error::Read)
    }

    /// Switch the console to raw/echo-off for the lifetime of this handle.
    pub fn enable_raw(&mut self) -> Result<(), Error> {
        if self.raw.is_none() {
            self.raw = Some(SchemeRawMode::enable(self.handle.0)?);
        }
        Ok(())
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.take() {
            drop(raw);
        }
        let _ = self.client.close(self.handle);
    }
}
