// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB HID boot-keyboard userspace driver skeleton (S60).
//!
//! The class/report decoder and the [`raven_usb_hid::BootKeyboard`] model are complete and
//! host-tested. The xHCI transfer-ring adapter that would bind them to the RP1 controller is the
//! follow-up in Phil's lane; this skeleton deliberately has no ring and reports the missing
//! binding instead of inventing an enumeration path. It keeps `#![forbid(unsafe_code)]`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_rt::println;
    use usb_hid::{BOOT_KEYBOARD_DESCRIPTOR, Error, KeyboardService, SetupPacket, TransferRing};

    /// A placeholder until the xHCI adapter lands.
    struct StubRing;

    impl TransferRing for StubRing {
        fn control_out(&mut self, _setup: SetupPacket, _data: &[u8]) -> Result<(), Error> {
            Err(Error::NoDevice)
        }

        fn control_in(&mut self, _setup: SetupPacket, _data: &mut [u8]) -> Result<usize, Error> {
            Err(Error::NoDevice)
        }

        fn interrupt_in(&mut self, _endpoint: u8, _length: usize) -> Result<(), Error> {
            Err(Error::NoDevice)
        }

        fn next_report(&mut self) -> Option<[u8; 8]> {
            None
        }
    }

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        let mut service = KeyboardService::new(StubRing);
        let _ = service.init(&BOOT_KEYBOARD_DESCRIPTOR);
        println!("usb-hid: boot-keyboard service ready (no transfer ring)");
        println!("usb-hid: xHCI binding is Phil's follow-up; enumeration UNVERIFIED");
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
