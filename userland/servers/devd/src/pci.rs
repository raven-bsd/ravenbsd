// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PCI driver matching shared by `devd` and its host tests.
//!
//! One discovered function is claimed by at most one driver: the first table entry whose
//! vendor/device pair matches. The result is sorted by the stable `bb:ss.f` function name so the
//! launch order does not depend on directory iteration order.

use alloc::vec::Vec;

use raven_proto_dev::{DeviceInfo, DriverSpec};

/// One PCI function and the driver table entry that claims it.
#[derive(Clone, Copy, Debug)]
pub struct Matched<'a> {
    /// The discovered function.
    pub device: DeviceInfo,
    /// The first matching driver table entry.
    pub driver: &'a DriverSpec,
}

/// Match every discovered function to its first matching driver, sorted by function name.
///
/// A driver table entry with a `compatible` string is a platform entry and never claims a PCI
/// function. Two entries with the same vendor/device do not create a second instance: the first
/// entry in table order wins.
#[must_use]
pub fn match_drivers<'a>(devices: &[DeviceInfo], drivers: &'a [DriverSpec]) -> Vec<Matched<'a>> {
    let mut matched: Vec<Matched<'a>> = devices
        .iter()
        .filter_map(|device| {
            drivers
                .iter()
                .find(|driver| {
                    driver.compatible.is_none()
                        && driver.vendor == device.vendor
                        && driver.device == device.device
                })
                .map(|driver| Matched {
                    device: *device,
                    driver,
                })
        })
        .collect();
    matched.sort_by_key(|left| left.device.name());
    matched
}
