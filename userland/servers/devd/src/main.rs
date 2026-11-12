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
            let options = split_driver_arguments(&driver.args)?;
            let bars = read_bars(&client, &device.name())?;
            let Some(&(kind, size)) = bars.get(options.bar) else {
                return Err(alloc::format!("bar{} does not exist", options.bar));
            };
            if kind != "memory" || size == 0 {
                return Err(alloc::format!(
                    "bar{} is {kind} (size {size}), not a memory BAR",
                    options.bar
                ));
            }
            // Every handle opened for this launch is closed here, whether the driver ran, the
            // spawn failed or a resource was missing: a restart cycle cannot leak DMA or IRQ
            // handles (B13c).
            let mut opened: Vec<Handle> = Vec::with_capacity(4);
            let launched = launch_driver(
                &client,
                &initfs,
                &device,
                driver,
                &options,
                size,
                &mut opened,
            );
            close_all(&opened);
            let code = launched?;
            cycle = cycle.saturating_add(1);
            raven_rt::println!("devd: driver {} cycle {} exit={}", driver.name, cycle, code);
        }
        raven_rt::println!("devd: driver {} cycles={}", driver.name, cycles);
        if test { Ok(()) } else { serve() }
    }

    /// The launch-time arguments `devd` consumes from the driver's own list.
    struct DriverOptions {
        bar: usize,
        dma: bool,
        irq: bool,
        rest: Vec<String>,
    }

    /// Split the driver's `args` into the selectors `devd` acts on and the tokens passed to the
    /// child.
    ///
    /// `bar=N` selects the PCI BAR (default 0, the first one). `resources=dma+irq` asks for the
    /// per-function DMA buffer and legacy IRQ line (default `none`). Neither token is passed on;
    /// `devd` appends `bar=<N>`, `bar_size=<bytes>`, and the requested `dma=<len>` and
    /// `irq=<line>` so a driver never has to guess them.
    fn split_driver_arguments(args: &[String]) -> Result<DriverOptions, String> {
        let mut options = DriverOptions {
            bar: 0,
            dma: false,
            irq: false,
            rest: Vec::with_capacity(args.len()),
        };
        for argument in args {
            if let Some(value) = argument.strip_prefix("bar=") {
                let bar: usize = value
                    .parse()
                    .map_err(|_| alloc::format!("invalid bar selector `{argument}`"))?;
                if bar >= 6 {
                    return Err(alloc::format!("bar{bar} is not a PCI BAR index"));
                }
                options.bar = bar;
                continue;
            }
            if let Some(value) = argument.strip_prefix("resources=") {
                // `+` rather than `,`: the `/etc/drivers.toml` parser splits an `args` array on
                // commas before it unquotes the elements.
                if value == "none" {
                    options.dma = false;
                    options.irq = false;
                } else {
                    for part in value.split('+') {
                        match part {
                            "dma" => options.dma = true,
                            "irq" => options.irq = true,
                            other => {
                                return Err(alloc::format!(
                                    "invalid resource `{other}` in `{argument}`"
                                ));
                            }
                        }
                    }
                }
                continue;
            }
            options.rest.push(argument.clone());
        }
        Ok(options)
    }

    /// Open one resource endpoint and remember the handle for the close pass.
    fn open_resource(
        client: &Client,
        device: &str,
        endpoint: &str,
        opened: &mut Vec<Handle>,
    ) -> Result<Handle, String> {
        let handle = client
            .open(&alloc::format!("pci:/{device}/{endpoint}"), READ)
            .map_err(|error| format!("{endpoint}: {}", error.description()))?;
        opened.push(handle);
        Ok(handle)
    }

    fn close_all(handles: &[Handle]) {
        for handle in handles {
            let _ = raven_rt::handle_close(*handle);
        }
    }

    /// One launch/exit cycle: open the requested resources, transfer them to the child, wait.
    ///
    /// The caller closes everything in `opened` once this returns, so a failed spawn or a failed
    /// wait releases the BAR, configuration page, DMA buffer and IRQ exactly once. The returned
    /// value is the driver's exit status.
    fn launch_driver(
        client: &Client,
        initfs: &Initfs<'_>,
        device: &DeviceInfo,
        driver: &DriverSpec,
        options: &DriverOptions,
        bar_size: u64,
        opened: &mut Vec<Handle>,
    ) -> Result<i32, String> {
        let name = device.name();
        let bar = open_resource(client, &name, &alloc::format!("bar{}", options.bar), opened)?;
        let config = open_resource(client, &name, "config", opened)?;
        raven_rt::println!(
            "devd: driver {} bar={} size={}",
            driver.name,
            options.bar,
            bar_size
        );

        let mut dma_token = None;
        let mut irq_token = None;
        let mut dma = None;
        let mut irq = None;
        if options.dma {
            let handle = open_resource(client, &name, "dma", opened)?;
            let info = raven_rt::dma_info(handle)
                .map_err(|error| format!("dma info: {}", error.description()))?;
            raven_rt::println!(
                "devd: driver {} dma bus={:#x} len={}",
                driver.name,
                info.bus_addr,
                info.len
            );
            dma_token = Some(alloc::format!("dma={}", info.len));
            dma = Some(handle);
        }
        if options.irq {
            let handle = open_resource(client, &name, "irq", opened)?;
            raven_rt::println!(
                "devd: driver {} irq line={}",
                driver.name,
                device.interrupt_line
            );
            irq_token = Some(alloc::format!("irq={}", device.interrupt_line));
            irq = Some(handle);
        }

        let mut handles: Vec<Handle> = Vec::with_capacity(4);
        handles.push(bar);
        handles.push(config);
        if let Some(handle) = dma {
            handles.push(handle);
        }
        if let Some(handle) = irq {
            handles.push(handle);
        }

        let mut argv: Vec<&str> = Vec::with_capacity(options.rest.len() + 6);
        argv.push(driver.name.as_str());
        argv.extend(options.rest.iter().map(String::as_str));
        let bar_token = alloc::format!("bar={}", options.bar);
        let size_token = alloc::format!("bar_size={bar_size}");
        argv.push(&bar_token);
        argv.push(&size_token);
        if let Some(token) = &dma_token {
            argv.push(token.as_str());
        }
        if let Some(token) = &irq_token {
            argv.push(token.as_str());
        }
        let mut sys = GrantSys { handles };
        let child = spawn_with(
            initfs,
            &driver.exec,
            &SpawnOptions {
                args: &argv,
                env: &[],
            },
            &mut sys,
        )
        .map_err(|error| alloc::format!("spawn: {error:?}"))?;
        let mut code = -1i32;
        if let Err(error) = RtSys.proc_wait(child, &mut code) {
            let _ = raven_rt::handle_close(child);
            return Err(format!("wait: {error:?}"));
        }
        let _ = raven_rt::handle_close(child);
        Ok(code)
    }

    /// Read `pci:/<dev>/bars`: one `index kind size` line per BAR.
    fn read_bars(client: &Client, device: &str) -> Result<Vec<(&'static str, u64)>, String> {
        let path = alloc::format!("pci:/{device}/bars");
        let handle = client
            .open(&path, READ)
            .map_err(|error| format!("bars: {}", error.description()))?;
        let mut text = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            let read = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| format!("bars read: {}", error.description()))?;
            if read == 0 {
                break;
            }
            text.extend_from_slice(buffer.get(..read).unwrap_or(&[]));
        }
        let _ = client.close(handle);
        let text = core::str::from_utf8(&text).map_err(|_| "bars: not UTF-8".to_string())?;
        let mut bars = Vec::new();
        for line in text.lines() {
            let mut fields = line.split_whitespace();
            let (Some(index), Some(kind), Some(size)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let index: usize = index.parse().map_err(|_| "bars: bad index".to_string())?;
            let size: u64 = size.parse().map_err(|_| "bars: bad size".to_string())?;
            let kind = match kind {
                "memory" => "memory",
                "io" => "io",
                _ => "absent",
            };
            if bars.len() <= index {
                bars.resize(index + 1, ("absent", 0u64));
            }
            if let Some(slot) = bars.get_mut(index) {
                *slot = (kind, size);
            }
        }
        Ok(bars)
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
        handles: Vec<Handle>,
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
            // The grants land in the child's first free slots, in order: a fresh child has
            // handles 0 (self), 1 (initfs) and 2 (namespace), so the first grant is handle 3 and
            // the second is handle 4. Drivers rely on that convention (see `devd(8)`).
            for handle in &self.handles {
                raven_rt::handle_transfer(proc, *handle, Rights::ALL)?;
            }
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
