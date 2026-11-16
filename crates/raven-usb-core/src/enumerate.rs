// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB enumeration: descriptors, address assignment and configuration selection.
//!
//! The state machine is `Attached -> Described -> Addressed -> Configured`. It reads the device
//! descriptor, assigns an address with [`AddressPool`], reads the first configuration header and
//! full set, then selects that configuration. All device traffic goes through
//! [`ControlTransport`]; the host tests use a synthetic device and the xHCI seam can submit the
//! same requests as TRBs.

use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::address::AddressPool;
use crate::control::{
    self, ControlError, ControlTransport, DescriptorType, SetupPacket, StandardRequest,
};
use crate::descriptor::{
    CONFIG_LEN, Configuration, ConfigurationHeader, DEVICE_LEN, Device, DeviceDescriptor, Interface,
};
use crate::error::UsbError;

/// The enumeration state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnumerationState {
    /// The port is attached but no descriptor was read.
    Attached,
    /// The device descriptor was read.
    Described,
    /// `SET_ADDRESS` completed.
    Addressed,
    /// The configuration was selected.
    Configured,
}

impl EnumerationState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Attached => "Attached",
            Self::Described => "Described",
            Self::Addressed => "Addressed",
            Self::Configured => "Configured",
        }
    }
}

impl fmt::Display for EnumerationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The enumerator's per-device state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enumerator {
    state: EnumerationState,
    address: Option<u8>,
    descriptor: Option<DeviceDescriptor>,
    configurations: Vec<Configuration>,
    active_configuration: Option<u8>,
}

impl Enumerator {
    /// A new, attached enumerator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: EnumerationState::Attached,
            address: None,
            descriptor: None,
            configurations: Vec::new(),
            active_configuration: None,
        }
    }

    /// Current state.
    #[must_use]
    pub const fn state(&self) -> EnumerationState {
        self.state
    }

    /// Assigned address, if `SET_ADDRESS` completed.
    #[must_use]
    pub const fn address(&self) -> Option<u8> {
        self.address
    }

    /// The device descriptor, if read.
    #[must_use]
    pub const fn device(&self) -> Option<&DeviceDescriptor> {
        self.descriptor.as_ref()
    }

    /// The parsed configurations, in index order.
    #[must_use]
    pub fn configurations(&self) -> &[Configuration] {
        &self.configurations
    }

    /// The selected `bConfigurationValue`.
    #[must_use]
    pub const fn active_configuration(&self) -> Option<u8> {
        self.active_configuration
    }

    /// The active configuration, if selected.
    #[must_use]
    pub fn configuration(&self) -> Option<&Configuration> {
        let value = self.active_configuration?;
        self.configurations
            .iter()
            .find(|configuration| configuration.header.configuration_value == value)
    }

    /// The active configuration's interface `number`/`alternate`.
    #[must_use]
    pub fn interface(&self, number: u8, alternate: u8) -> Option<&Interface> {
        self.configuration()
            .and_then(|configuration| configuration.interface(number, alternate))
    }

    /// Enumerate a device from `Attached`.
    ///
    /// # Errors
    ///
    /// Returns a descriptor/control/address/state error. The address is released when the failure
    /// happens after `SET_ADDRESS` but before the configuration is selected.
    pub fn enumerate(
        &mut self,
        transport: &mut dyn ControlTransport,
        pool: &mut AddressPool,
    ) -> Result<(), UsbError> {
        self.expect(EnumerationState::Attached)?;
        let descriptor = self.read_device(transport)?;
        self.descriptor = Some(descriptor);
        self.state = EnumerationState::Described;

        let address = pool.assign()?;
        let Some(setup) = control::set_address(address) else {
            let _ = pool.release(address);
            return Err(UsbError::Address(crate::address::AddressError::Invalid(
                address,
            )));
        };
        if let Err(error) = transport.control_no_data(setup) {
            let _ = pool.release(address);
            return Err(error);
        }
        self.address = Some(address);
        self.state = EnumerationState::Addressed;

        let result = self
            .read_configuration(transport)
            .and_then(|configuration| {
                self.configurations.clear();
                self.configurations.push(configuration);
                self.select_configuration(transport, 0)
            });
        if result.is_err()
            && let Some(address) = self.address.take()
        {
            let _ = pool.release(address);
        }
        result
    }

    /// Send `SET_INTERFACE` for the active configuration.
    ///
    /// # Errors
    ///
    /// Returns a state/control error, or a descriptor error when the interface/alternate does not
    /// exist.
    pub fn set_interface(
        &mut self,
        transport: &mut dyn ControlTransport,
        number: u8,
        alternate: u8,
    ) -> Result<(), UsbError> {
        self.expect(EnumerationState::Configured)?;
        if self.interface(number, alternate).is_none() {
            return Err(UsbError::State {
                expected: "an existing interface/alternate",
                actual: "missing interface",
            });
        }
        transport.control_no_data(control::set_interface(number, alternate))
    }

    /// Release the assigned address (detach or failed enumeration).
    ///
    /// # Errors
    ///
    /// Returns an address error when the pool does not know the address.
    pub fn release(&mut self, pool: &mut AddressPool) -> Result<(), UsbError> {
        let Some(address) = self.address.take() else {
            return Ok(());
        };
        pool.release(address)?;
        self.state = EnumerationState::Described;
        self.active_configuration = None;
        Ok(())
    }

    fn expect(&self, expected: EnumerationState) -> Result<(), UsbError> {
        if self.state != expected {
            return Err(UsbError::State {
                expected: expected.as_str(),
                actual: self.state.as_str(),
            });
        }
        Ok(())
    }

    fn read_device(
        &self,
        transport: &mut dyn ControlTransport,
    ) -> Result<DeviceDescriptor, UsbError> {
        let setup = control::get_descriptor(DescriptorType::Device, 0, 0, DEVICE_LEN as u16);
        let mut buffer = [0u8; DEVICE_LEN];
        let got = transport.control(setup, &mut buffer)?;
        if got < DEVICE_LEN {
            return Err(UsbError::Control(ControlError::ShortTransfer {
                expected: DEVICE_LEN,
                got,
            }));
        }
        Ok(DeviceDescriptor::parse(&buffer[..got])?)
    }

    fn read_configuration(
        &self,
        transport: &mut dyn ControlTransport,
    ) -> Result<Configuration, UsbError> {
        let setup = control::get_descriptor(DescriptorType::Configuration, 0, 0, CONFIG_LEN as u16);
        let mut header_bytes = [0u8; CONFIG_LEN];
        let got = transport.control(setup, &mut header_bytes)?;
        if got < CONFIG_LEN {
            return Err(UsbError::Control(ControlError::ShortTransfer {
                expected: CONFIG_LEN,
                got,
            }));
        }
        let header = ConfigurationHeader::parse(&header_bytes[..got])?;
        let total = usize::from(header.total_length);
        let setup =
            control::get_descriptor(DescriptorType::Configuration, 0, 0, header.total_length);
        let mut buffer = vec![0u8; total];
        let got = transport.control(setup, &mut buffer)?;
        if got < total {
            return Err(UsbError::Control(ControlError::ShortTransfer {
                expected: total,
                got,
            }));
        }
        Ok(Configuration::parse(&buffer[..got])?)
    }

    fn select_configuration(
        &mut self,
        transport: &mut dyn ControlTransport,
        index: usize,
    ) -> Result<(), UsbError> {
        let Some(configuration) = self.configurations.get(index) else {
            return Err(UsbError::NoConfiguration);
        };
        let value = configuration.header.configuration_value;
        let Some(setup) = control::set_configuration(value) else {
            return Err(UsbError::ConfigurationNotFound(value));
        };
        transport.control_no_data(setup)?;
        self.active_configuration = Some(value);
        self.state = EnumerationState::Configured;
        Ok(())
    }
}

impl Default for Enumerator {
    fn default() -> Self {
        Self::new()
    }
}

/// The descriptor aggregate after enumeration.
#[must_use]
pub fn device(enumerator: &Enumerator) -> Option<Device> {
    let descriptor = *enumerator.device()?;
    Device::new(descriptor, enumerator.configurations().to_vec()).ok()
}

/// The standard request implied by a setup packet, if it is a standard request.
#[must_use]
pub fn standard_request(setup: SetupPacket) -> Option<StandardRequest> {
    StandardRequest::from_setup(setup)
}

#[cfg(test)]
#[path = "enumerate_ut.rs"]
mod tests;
