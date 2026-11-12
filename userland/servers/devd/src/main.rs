// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `devd`: match `pci:` devices against `/etc/drivers.toml` and launch drivers.
//!
//! The production mode registers the `dev:` scheme after scanning. The `test` argument runs the
//! matched driver a fixed number of cycles and exits, which the qtest uses to prove clean
//! MMIO-handle teardown and restart.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::BTreeMap;
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::{Handle, Rights};
    use raven_initfs::Initfs;
    use raven_proto_dev::{DEVICE_INFO_LEN, DeviceInfo, DriverSpec, parse_drivers};
    use raven_proto_fs::{S_IFREG, Stat};
    use raven_rt::entry;
    use raven_rt::spawn::{RtSys, SpawnOptions, Sys, spawn_with};
    use raven_scheme::{Client, Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    const READ: u32 = 1;
    const OPEN_OPCODE: u16 = 1;
    const OPEN_READ: u32 = 1;

    fn main(args: raven_rt::Args) -> i32 {
        let executable = args.get(0).unwrap_or("devd");
        let mode = args.get(1).unwrap_or("serve");
        if executable == "dummy" || mode == "dummy" {
            return dummy_driver();
        }
        match run(mode == "test") {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("devd: {message}");
                1
            }
        }
    }

    fn run(test: bool) -> Result<(), String> {
        let image = raven_rt::map_initfs().map_err(|error| error.description().to_string())?;
        let initfs = Initfs::parse(image).map_err(|error| alloc::format!("{error:?}"))?;
        let client = Client::new();
        let drivers = load_drivers(&initfs)?;
        let devices = devices(&client)?;
        raven_rt::println!("devd: devices={}", devices.len());
        let Some((device, driver)) = match_driver(&devices, &drivers) else {
            raven_rt::println!("devd: no matching driver");
            if test {
                return Ok(());
            }
            return serve();
        };
        raven_rt::println!(
            "devd: matched {} vendor={:#06x} device={:#06x} irq_line={} irq_pin={}",
            device.name(),
            device.vendor,
            device.device,
            device.interrupt_line,
            device.interrupt_pin
        );
        let cycles = if test { driver.cycles } else { 1 };
        let mut cycle = 0u32;
        while cycle < cycles {
            let bar = client
                .open(&alloc::format!("pci:/{}/bar0", device.name()), READ)
                .map_err(|error| format!("bar0: {}", error.description()))?;
            let mut sys = GrantSys { handle: bar };
            let mut argv: Vec<&str> = Vec::with_capacity(driver.args.len() + 1);
            argv.push(driver.name.as_str());
            argv.extend(driver.args.iter().map(String::as_str));
            let child = spawn_with(
                &initfs,
                &driver.exec,
                &SpawnOptions {
                    args: &argv,
                    env: &[],
                },
                &mut sys,
            )
            .map_err(|error| alloc::format!("spawn: {error:?}"))?;
            let mut code = -1i32;
            RtSys
                .proc_wait(child, &mut code)
                .map_err(|error| format!("wait: {error:?}"))?;
            let _ = raven_rt::handle_close(bar);
            let _ = raven_rt::handle_close(child);
            cycle = cycle.saturating_add(1);
            raven_rt::println!("devd: driver {} cycle {} exit={}", driver.name, cycle, code);
        }
        raven_rt::println!("devd: driver {} cycles={}", driver.name, cycles);
        if test { Ok(()) } else { serve() }
    }

    fn load_drivers(initfs: &Initfs<'_>) -> Result<Vec<DriverSpec>, String> {
        let Some(entry) = initfs.get("/etc/drivers.toml") else {
            return Ok(Vec::new());
        };
        let text = core::str::from_utf8(entry.data)
            .map_err(|_| "drivers.toml is not UTF-8".to_string())?;
        parse_drivers(text)
            .map_err(|error| alloc::format!("drivers.toml line {}: invalid", error.line))
    }

    fn devices(client: &Client) -> Result<Vec<DeviceInfo>, String> {
        let handle = client
            .open("pci:/", READ)
            .map_err(|error| format!("pci: {}", error.description()))?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            let read = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| format!("pci read: {}", error.description()))?;
            if read == 0 {
                break;
            }
            data.extend_from_slice(buffer.get(..read).unwrap_or(&[]));
        }
        let _ = client.close(handle);
        let mut names = Vec::new();
        for entry in raven_proto_fs::DirEntryIter::new(&data) {
            let entry = entry.map_err(|_| "pci dirent".to_string())?;
            names.push(
                core::str::from_utf8(entry.name)
                    .map_err(|_| "pci name".to_string())?
                    .to_string(),
            );
        }
        let mut result = Vec::new();
        for name in names {
            let handle = client
                .open(&alloc::format!("pci:/{name}/info"), READ)
                .map_err(|error| format!("info {name}: {}", error.description()))?;
            let mut info = [0u8; DEVICE_INFO_LEN];
            let read = client
                .read(handle, 0, &mut info)
                .map_err(|error| format!("info {name}: {}", error.description()))?;
            let _ = client.close(handle);
            let Some(device) = DeviceInfo::decode(info.get(..read).unwrap_or(&[])) else {
                return Err(format!("info {name}: bad record"));
            };
            result.push(device);
        }
        Ok(result)
    }

    fn match_driver<'a>(
        devices: &'a [DeviceInfo],
        drivers: &'a [DriverSpec],
    ) -> Option<(DeviceInfo, &'a DriverSpec)> {
        for device in devices {
            if let Some(driver) = drivers
                .iter()
                .find(|driver| driver.vendor == device.vendor && driver.device == device.device)
            {
                return Some((*device, driver));
            }
        }
        None
    }

    fn serve() -> Result<(), String> {
        let mut server =
            Server::register("dev").map_err(|error| error.description().to_string())?;
        let mut handler = DevHandler {
            files: BTreeMap::new(),
            pending: None,
        };
        loop {
            let Some(packet) = server.poll(None) else {
                continue;
            };
            if packet.packet.opcode == OPEN_OPCODE
                && let Ok(path) = core::str::from_utf8(packet.packet.path_bytes())
            {
                handler.pending = Some((packet.packet.file_id, path.to_string()));
            }
            let _ = server.dispatch(packet, &mut handler);
        }
    }

    struct OpenFile {
        data: Vec<u8>,
        cursor: usize,
    }

    struct DevHandler {
        files: BTreeMap<u64, OpenFile>,
        pending: Option<(u64, String)>,
    }

    impl Handler for DevHandler {
        fn open(
            &mut self,
            path: &str,
            flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let Some((file_id, expected)) = self.pending.take() else {
                return Err(Errno::EINVAL.into());
            };
            if expected != path || flags & OPEN_READ == 0 {
                return Err(Errno::EINVAL.into());
            }
            self.files.insert(
                file_id,
                OpenFile {
                    data: b"devd\n".to_vec(),
                    cursor: 0,
                },
            );
            Ok(Opened::File(file_id))
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            let cursor = self.files.get(&file).ok_or(Errno::EBADF)?.cursor;
            let remaining = self
                .files
                .get(&file)
                .ok_or(Errno::EBADF)?
                .data
                .get(cursor..)
                .unwrap_or(&[]);
            let length = remaining.len().min(buffer.len());
            let target = buffer.get_mut(..length).ok_or(Errno::EINVAL)?;
            target.copy_from_slice(remaining.get(..length).ok_or(Errno::EINVAL)?);
            if let Some(open) = self.files.get_mut(&file) {
                open.cursor = open.cursor.saturating_add(length);
            }
            Ok(length)
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.files
                .remove(&file)
                .map(|_| ())
                .ok_or_else(|| Errno::EBADF.into())
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let data = &self.files.get(&file).ok_or(Errno::EBADF)?.data;
            let size = data.len() as u64;
            Ok(Stat {
                ino: file.wrapping_add(1),
                size,
                mode: S_IFREG | 0o444,
                nlink: 1,
                ..Stat::default()
            })
        }
    }

    struct GrantSys {
        handle: Handle,
    }

    impl Sys for GrantSys {
        fn proc_create(&mut self) -> Result<Handle, Errno> {
            RtSys.proc_create()
        }

        fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno> {
            RtSys.memobj_create(len)
        }

        fn map(
            &mut self,
            proc: Handle,
            memobj: Handle,
            offset: usize,
            addr: usize,
            len: usize,
            flags: usize,
        ) -> Result<usize, Errno> {
            RtSys.map(proc, memobj, offset, addr, len, flags)
        }

        fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
            RtSys.unmap(proc, addr, len)
        }

        fn thread_create(
            &mut self,
            proc: Handle,
            entry: usize,
            stack_top: usize,
            arg: usize,
        ) -> Result<Handle, Errno> {
            raven_rt::handle_transfer(proc, self.handle, Rights::ALL)?;
            RtSys.thread_create(proc, entry, stack_top, arg)
        }

        fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
            RtSys.handle_close(handle)
        }

        fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
            RtSys.write_parent_mem(addr, bytes);
        }
    }

    fn dummy_driver() -> i32 {
        // `devd` transfers the BAR handle into the child before its first thread starts; a
        // fresh child has handles 0, 1 and 2 installed, so the grant lands in slot 3.
        let handle = Handle::new(3, 0);
        let mapped = match raven_rt::map_mmio(handle, 4096) {
            Ok(mapped) => mapped,
            Err(error) => {
                raven_rt::eprintln!("devd: dummy map: {}", error.description());
                return 1;
            }
        };
        let _ = mapped.first();
        match raven_rt::mmio_create(handle, 0, 4096, 0) {
            Err(Errno::EPERM) => raven_rt::println!("devd: dummy rights ok"),
            other => {
                raven_rt::eprintln!("devd: dummy rights {other:?}");
                return 1;
            }
        }
        raven_rt::println!("devd: dummy running");
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
