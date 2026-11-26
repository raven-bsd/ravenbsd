// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `devd`: match `pci:` devices against `/etc/drivers.toml` and launch drivers.
//!
//! The production mode registers the `dev:` scheme after scanning. The `test` argument runs the
//! matched driver a fixed number of cycles and exits, which the qtest uses to prove clean
//! MMIO-handle teardown and restart. The `dtb` argument decodes the platform bus from
//! `/etc/pi4.dtb`; `dtb launch` creates the declared MMIO/DMA/IRQ grants and spawns each matched
//! platform driver. It is the aarch64/Pi 4 path and does not touch the PCI scheme.

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
    use raven_devd::dtb;
    use raven_devd::manifest;
    use raven_devd::pci::{Matched, match_drivers};
    use raven_devd::platform;
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
    const DTB_PATH: &str = "/etc/pi4.dtb";

    fn main(args: raven_rt::Args) -> i32 {
        let executable = args.get(0).unwrap_or("devd");
        let mode = args.get(1).unwrap_or("serve");
        let action = args.get(2).unwrap_or("");
        if executable == "dummy" || mode == "dummy" {
            return dummy_driver();
        }
        match run(mode, action) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("devd: {message}");
                1
            }
        }
    }

    fn run(mode: &str, action: &str) -> Result<(), String> {
        if mode == "dtb" {
            return run_dtb(action == "launch");
        }
        let test = mode == "test";
        let image = raven_rt::map_initfs().map_err(|error| error.description().to_string())?;
        let initfs = Initfs::parse(image).map_err(|error| alloc::format!("{error:?}"))?;
        let client = Client::new();
        let drivers = load_drivers(&initfs)?;
        let devices = devices(&client)?;
        raven_rt::println!("devd: devices={}", devices.len());
        let matched = match_drivers(&devices, &drivers);
        if matched.is_empty() {
            raven_rt::println!("devd: no matching driver");
            if test {
                return Ok(());
            }
            return serve();
        }
        for entry in &matched {
            let device = &entry.device;
            raven_rt::println!(
                "devd: matched {} vendor={:#06x} device={:#06x} irq_line={} irq_pin={}",
                device.name(),
                device.vendor,
                device.device,
                device.interrupt_line,
                device.interrupt_pin
            );
        }
        if test {
            run_tests(&client, &initfs, &matched)
        } else {
            run_production(&client, &initfs, &matched)
        }
    }

    /// Test mode: run every matched driver for its declared cycles, then report the first error.
    ///
    /// A failing driver does not stop the later matches from being reported; the caller still
    /// sees a non-zero exit because the first error is returned after the loop.
    fn run_tests(
        client: &Client,
        initfs: &Initfs<'_>,
        matched: &[Matched<'_>],
    ) -> Result<(), String> {
        let mut failure: Option<String> = None;
        for entry in matched {
            if let Err(error) = run_test_driver(client, initfs, entry)
                && failure.is_none()
            {
                failure = Some(alloc::format!("{}: {error}", entry.driver.name));
            }
        }
        failure.map_or(Ok(()), Err)
    }

    fn run_test_driver(
        client: &Client,
        initfs: &Initfs<'_>,
        entry: &Matched<'_>,
    ) -> Result<(), String> {
        let driver = entry.driver;
        let cycles = driver.cycles;
        let mut cycle = 0u32;
        while cycle < cycles {
            let code = match launch_driver(client, initfs, &entry.device, driver) {
                Ok(code) => code,
                Err(error) => {
                    raven_rt::println!("devd: driver {} failed: {error}", driver.name);
                    return Err(error);
                }
            };
            cycle = cycle.saturating_add(1);
            raven_rt::println!("devd: driver {} cycle {} exit={}", driver.name, cycle, code);
        }
        raven_rt::println!("devd: driver {} cycles={}", driver.name, cycles);
        Ok(())
    }

    /// Production mode: spawn every matched driver before waiting, then supervise them.
    ///
    /// Spawning first keeps a long-lived driver (virtio-net, GENET) from starving the functions
    /// after it. Each function gets its own handles and its own child. A spawn error aborts the
    /// children that already started and names the driver; a non-zero exit aborts the rest after
    /// the other launches have been reported. A clean exit from every one-shot driver reaches the
    /// `dev:` region as before.
    fn run_production(
        client: &Client,
        initfs: &Initfs<'_>,
        matched: &[Matched<'_>],
    ) -> Result<(), String> {
        let mut children: Vec<(String, Handle)> = Vec::with_capacity(matched.len());
        let mut failures: Vec<String> = Vec::new();
        for entry in matched {
            match spawn_driver(client, initfs, &entry.device, entry.driver) {
                Ok(child) => children.push((entry.driver.name.clone(), child)),
                Err(error) => {
                    // Report every matched function before failing the boot, so a broken entry
                    // does not hide the launch state of the drivers after it.
                    raven_rt::println!("devd: driver {} launch failed: {error}", entry.driver.name);
                    failures.push(alloc::format!("{}: {error}", entry.driver.name));
                }
            }
        }
        if !failures.is_empty() {
            for (_, child) in children.drain(..) {
                abort_child(child);
            }
            return Err(failures.join("; "));
        }
        let mut remaining = children.into_iter();
        while let Some((name, child)) = remaining.next() {
            let mut code = -1i32;
            let waited = RtSys.proc_wait(child, &mut code);
            let _ = raven_rt::handle_close(child);
            if let Err(error) = waited {
                for (_, child) in remaining {
                    abort_child(child);
                }
                return Err(alloc::format!("{name} wait: {error:?}"));
            }
            raven_rt::println!("devd: driver {name} exit={code}");
            if code != 0 {
                for (_, child) in remaining {
                    abort_child(child);
                }
                return Err(alloc::format!("{name} exited {code}"));
            }
        }
        serve()
    }

    /// Stop a child spawned before a later launch or wait failed.
    fn abort_child(child: Handle) {
        let _ = raven_rt::proc_terminate(child, 1);
        let mut code = -1i32;
        let _ = RtSys.proc_wait(child, &mut code);
        let _ = raven_rt::handle_close(child);
    }

    /// Decode the platform device tree and, when `launch` is set, start the matched drivers.
    ///
    /// Without `launch` this is the K32 discovery path: print the decoded devices and the
    /// compatible matches, then exit. With `launch` every matched driver is spawned with its
    /// declared grants before the first wait, so one long-lived driver (the GENET `nic:` server)
    /// cannot starve the drivers after it. The one-shot `dtb launch` then waits for each child
    /// and exits nonzero if any driver fails.
    fn run_dtb(launch: bool) -> Result<(), String> {
        let image = raven_rt::map_initfs().map_err(|error| error.description().to_string())?;
        let initfs = Initfs::parse(image).map_err(|error| alloc::format!("{error:?}"))?;
        let drivers = load_drivers(&initfs)?;
        let Some(entry) = initfs.get(DTB_PATH) else {
            return Err(alloc::format!("{DTB_PATH} is missing from the initfs"));
        };
        let devices =
            dtb::parse(entry.data).map_err(|error| alloc::format!("{DTB_PATH}: {error}"))?;
        raven_rt::println!("devd: dtb devices={}", devices.len());
        for device in &devices {
            let reg = device.first_reg().map_or_else(
                || "none".to_string(),
                |reg| alloc::format!("{:#x}+{:#x}", reg.base, reg.size),
            );
            raven_rt::println!(
                "devd: dtb device {} compatible={} reg={} dma={} irq={}",
                device.path,
                device.compatible().unwrap_or("none"),
                reg,
                device.dma.len(),
                device.irq_label()
            );
        }
        let matched = dtb::match_drivers(&devices, &drivers);
        for (device, driver) in &matched {
            raven_rt::println!(
                "devd: dtb driver {} compatible={} device={}",
                driver.name,
                driver.compatible.as_deref().unwrap_or("none"),
                device.path
            );
        }
        if launch {
            let mut children = Vec::with_capacity(matched.len());
            for (device, driver) in &matched {
                match spawn_platform(&initfs, device, driver) {
                    Ok(child) => children.push(child),
                    Err(error) => {
                        for child in children.drain(..) {
                            abort_platform(child);
                        }
                        return Err(error);
                    }
                }
            }
            let mut remaining = children.into_iter();
            while let Some(child) = remaining.next() {
                if let Err(error) = finish_platform(child) {
                    for child in remaining {
                        abort_platform(child);
                    }
                    return Err(error);
                }
            }
        }
        raven_rt::println!("devd: dtb matched={}", matched.len());
        Ok(())
    }

    /// A spawned platform driver and its devd-side process handle.
    struct PlatformChild {
        name: String,
        child: Handle,
    }

    /// Create the declared grants, transfer them to a fresh child and start the driver.
    ///
    /// `devd` holds the root-device capability in the platform profile. The grant set comes from
    /// [`platform::PlatformPlan::grants`], so only the selected `reg` window, the requested DMA
    /// buffer and the device's first GIC interrupt are created. The child receives the handles in
    /// that order (BAR, DMA, IRQ); the argv tokens keep the PCI shape.
    fn spawn_platform(
        initfs: &Initfs<'_>,
        device: &dtb::DeviceSpec,
        driver: &DriverSpec,
    ) -> Result<PlatformChild, String> {
        let plan = platform::plan(device, &driver.args)
            .map_err(|error| alloc::format!("{}: {error}", driver.name))?;
        let root = Handle::ROOT_DEVICE;
        let mut handles = Vec::with_capacity(3);
        let mut dma_size = 0usize;
        let mut irq_line = 0u32;
        for grant in plan.grants() {
            let created = match grant {
                platform::Grant::Bar { base, size, .. } => page_len(size).and_then(|length| {
                    raven_rt::mmio_create(root, base, length, 0)
                        .map_err(|error| error.description().to_string())
                }),
                platform::Grant::Dma { len } => {
                    raven_rt::dma_alloc(root, len, platform::PLATFORM_PAGE_SIZE, 32, 0)
                        .map_err(|error| error.description().to_string())
                }
                platform::Grant::Irq { intid } => {
                    raven_rt::irq_create(root, raven_rt::IRQ_LEGACY, intid, None)
                        .map_err(|error| error.description().to_string())
                }
            };
            let handle = match created {
                Ok(handle) => handle,
                Err(error) => {
                    close_all(&handles);
                    return Err(alloc::format!(
                        "{} resource {grant:?}: {error}",
                        driver.name
                    ));
                }
            };
            if let platform::Grant::Dma { .. } = grant {
                let info = match raven_rt::dma_info(handle) {
                    Ok(info) => info,
                    Err(error) => {
                        let _ = raven_rt::handle_close(handle);
                        close_all(&handles);
                        return Err(alloc::format!(
                            "{} dma info: {}",
                            driver.name,
                            error.description()
                        ));
                    }
                };
                dma_size = match usize::try_from(info.len) {
                    Ok(length) => length,
                    Err(_) => {
                        let _ = raven_rt::handle_close(handle);
                        close_all(&handles);
                        return Err(alloc::format!("{} dma length overflow", driver.name));
                    }
                };
            }
            if let platform::Grant::Irq { intid } = grant {
                irq_line = intid;
            }
            handles.push(handle);
        }

        let tokens = plan.tokens();
        let mut argv: Vec<&str> = Vec::with_capacity(tokens.len() + 1);
        argv.push(driver.name.as_str());
        argv.extend(tokens.iter().map(String::as_str));
        if let (Some(bar), Some(reg)) = (plan.bars.first(), plan.regs.first()) {
            if plan.bars.len() == 1 {
                raven_rt::println!(
                    "devd: dtb launch {} bar={} size={:#x} dma={} irq={}",
                    driver.name,
                    bar,
                    reg.size,
                    dma_size,
                    irq_line
                );
            } else {
                raven_rt::println!(
                    "devd: dtb launch {} bars={} bar={} size={:#x} dma={} irq={}",
                    driver.name,
                    plan.bars.len(),
                    bar,
                    reg.size,
                    dma_size,
                    irq_line
                );
            }
        }
        let mut sys = GrantSys { handles };
        let child = match spawn_with(
            initfs,
            &driver.exec,
            &SpawnOptions {
                args: &argv,
                env: &[],
            },
            &mut sys,
        ) {
            Ok(child) => child,
            Err(error) => {
                close_all(&sys.handles);
                return Err(alloc::format!("{} spawn: {error:?}", driver.name));
            }
        };
        // A successful spawn transferred the handles into the child. Closing the stale values
        // here could close a slot reused by the next launch, so the child owns them now.
        sys.handles.clear();
        Ok(PlatformChild {
            name: driver.name.clone(),
            child,
        })
    }

    /// Wait for one platform driver and report a nonzero exit as a launch failure.
    fn finish_platform(child: PlatformChild) -> Result<(), String> {
        let mut code = -1i32;
        let waited = RtSys.proc_wait(child.child, &mut code);
        let _ = raven_rt::handle_close(child.child);
        waited.map_err(|error| alloc::format!("{} wait: {error:?}", child.name))?;
        raven_rt::println!("devd: dtb launch {} exit={}", child.name, code);
        if code != 0 {
            return Err(alloc::format!("{} exited {code}", child.name));
        }
        Ok(())
    }

    /// Stop a child that was spawned before a later launch failed.
    fn abort_platform(child: PlatformChild) {
        let _ = raven_rt::proc_terminate(child.child, 1);
        let mut code = -1i32;
        let _ = RtSys.proc_wait(child.child, &mut code);
        let _ = raven_rt::handle_close(child.child);
    }

    fn page_len(size: u64) -> Result<usize, String> {
        let page = platform::PLATFORM_PAGE_SIZE;
        let length = size
            .checked_add(page as u64 - 1)
            .ok_or_else(|| "platform window is too large".to_string())?
            & !(page as u64 - 1);
        if length == 0 {
            return Err("platform window is empty".to_string());
        }
        usize::try_from(length).map_err(|_| "platform window is too large".to_string())
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
    /// One prepared driver launch: the child argv and the handles to transfer.
    struct Prepared {
        exec: String,
        argv: Vec<String>,
        handles: Vec<Handle>,
    }

    /// Open one function's resources and build the child argv.
    ///
    /// Every handle opened here is transferred to the child by [`spawn_prepared`] and then closed
    /// in the parent; `opened` is moved into the result on success and closed on error.
    fn prepare_launch(
        client: &Client,
        device: &DeviceInfo,
        driver: &DriverSpec,
    ) -> Result<Prepared, String> {
        let mut opened: Vec<Handle> = Vec::with_capacity(4);
        match prepare_launch_inner(client, device, driver, &mut opened) {
            Ok(argv) => Ok(Prepared {
                exec: driver.exec.clone(),
                argv,
                handles: opened,
            }),
            Err(error) => {
                close_all(&opened);
                Err(error)
            }
        }
    }

    fn prepare_launch_inner(
        client: &Client,
        device: &DeviceInfo,
        driver: &DriverSpec,
        opened: &mut Vec<Handle>,
    ) -> Result<Vec<String>, String> {
        let options = split_driver_arguments(&driver.args)?;
        let name = device.name();
        let bars = read_bars(client, &name)?;
        let Some(&(kind, size)) = bars.get(options.bar) else {
            return Err(alloc::format!("bar{} does not exist", options.bar));
        };
        if kind != "memory" || size == 0 {
            return Err(alloc::format!(
                "bar{} is {kind} (size {size}), not a memory BAR",
                options.bar
            ));
        }
        let _ = open_resource(client, &name, &alloc::format!("bar{}", options.bar), opened)?;
        let config = open_resource(client, &name, "config", opened)?;
        raven_rt::println!(
            "devd: driver {} bar={} size={}",
            driver.name,
            options.bar,
            size
        );

        let mut argv: Vec<String> = Vec::with_capacity(options.rest.len() + 6);
        argv.push(driver.name.clone());
        argv.extend(options.rest.iter().cloned());
        argv.push(alloc::format!("bar={}", options.bar));
        argv.push(alloc::format!("bar_size={size}"));
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
            argv.push(alloc::format!("dma={}", info.len));
        }
        if options.irq {
            // Prefer one MSI-X vector per function: two functions on the same legacy INTx line
            // cannot both claim the exclusive GSI. The mapping also proves that the config page
            // is writable, which the driver needs to set the MSI-X enable bit.
            let mut msix = None;
            match raven_rt::map_mmio_mut(config, platform::PLATFORM_PAGE_SIZE) {
                Ok(view) => match msix_capability(view) {
                    Some(capability) => {
                        raven_rt::println!(
                            "devd: driver {} msix capability table_bar={} table={:#x} entries={}",
                            driver.name,
                            capability.table_bar,
                            capability.table_offset,
                            capability.table_size
                        );
                        msix = Some(capability);
                    }
                    None => {
                        raven_rt::println!("devd: driver {} msix capability absent", driver.name)
                    }
                },
                Err(error) => raven_rt::println!(
                    "devd: driver {} msix config map: {}",
                    driver.name,
                    error.description()
                ),
            }
            let mut msix_done = false;
            if let Some(capability) = msix {
                let table_bar = capability.table_bar;
                let (table_kind, table_bar_size) = match bars.get(table_bar) {
                    Some(&(kind, bar_size)) => (kind, usize::try_from(bar_size).unwrap_or(0)),
                    None => ("absent", 0),
                };
                let table_fits = table_kind == "memory"
                    && capability.table_offset.saturating_add(32) <= table_bar_size;
                if capability.table_size < 2 {
                    raven_rt::println!(
                        "devd: driver {} msix has {} entries; need 2",
                        driver.name,
                        capability.table_size
                    );
                } else if !table_fits {
                    raven_rt::println!(
                        "devd: driver {} msix table bar{} is {} size {}; table at {:#x} does not fit",
                        driver.name,
                        table_bar,
                        table_kind,
                        table_bar_size,
                        capability.table_offset
                    );
                } else {
                    let table = client
                        .open(&alloc::format!("pci:/{name}/bar{table_bar}"), READ)
                        .map_err(|error| {
                            alloc::format!("msix bar{table_bar}: {}", error.description())
                        });
                    match table {
                        Ok(table_handle) => {
                            let mut info = raven_rt::IrqInfo::default();
                            match raven_rt::irq_create(
                                Handle::ROOT_DEVICE,
                                raven_rt::IRQ_MSIX,
                                0,
                                Some(&mut info),
                            ) {
                                Ok(handle) => {
                                    opened.push(handle);
                                    opened.push(table_handle);
                                    argv.push(String::from("msix=1"));
                                    argv.push(alloc::format!("msix_addr_lo={}", info.address_lo));
                                    argv.push(alloc::format!("msix_addr_hi={}", info.address_hi));
                                    argv.push(alloc::format!("msix_data={}", info.data));
                                    argv.push(alloc::format!("msix_bar={table_bar}"));
                                    argv.push(alloc::format!("msix_bar_size={table_bar_size}"));
                                    argv.push(alloc::format!(
                                        "msix_table={}",
                                        capability.table_offset
                                    ));
                                    raven_rt::println!(
                                        "devd: driver {} msix vector={} table={:#x} bar={}",
                                        driver.name,
                                        info.vector,
                                        capability.table_offset,
                                        table_bar
                                    );
                                    msix_done = true;
                                }
                                Err(error) => {
                                    let _ = raven_rt::handle_close(table_handle);
                                    raven_rt::println!(
                                        "devd: driver {} msix unavailable: {}; using legacy irq",
                                        driver.name,
                                        error.description()
                                    );
                                }
                            }
                        }
                        Err(error) => raven_rt::println!(
                            "devd: driver {} msix table open: {error}",
                            driver.name
                        ),
                    }
                }
            }
            if !msix_done {
                let _ = open_resource(client, &name, "irq", opened)?;
                raven_rt::println!(
                    "devd: driver {} irq line={}",
                    driver.name,
                    device.interrupt_line
                );
                argv.push(alloc::format!("irq={}", device.interrupt_line));
            }
        }
        Ok(argv)
    }

    /// One function's MSI-X table location from the PCI configuration page.
    struct MsixCapability {
        table_bar: usize,
        table_offset: usize,
        table_size: u16,
    }

    /// Walk the PCI capability list for MSI-X (`id 0x11`).
    ///
    /// The caller has already mapped the configuration page. A missing or malformed capability
    /// keeps the legacy INTx path.
    fn msix_capability(config: &[u8]) -> Option<MsixCapability> {
        let status = u16::from_le_bytes(config.get(0x06..0x08)?.try_into().ok()?);
        if status & (1 << 4) == 0 {
            return None;
        }
        let mut pointer = usize::from(*config.get(0x34)? & 0xFC);
        let mut seen = 0u32;
        while pointer != 0 && seen < 48 {
            let id = *config.get(pointer)?;
            let next = usize::from(*config.get(pointer.saturating_add(1))? & 0xFC);
            if id == 0x11 {
                let control = u16::from_le_bytes(
                    config
                        .get(pointer.saturating_add(2)..pointer.saturating_add(4))?
                        .try_into()
                        .ok()?,
                );
                let table = u32::from_le_bytes(
                    config
                        .get(pointer.saturating_add(4)..pointer.saturating_add(8))?
                        .try_into()
                        .ok()?,
                );
                return Some(MsixCapability {
                    table_bar: usize::try_from(table & 7).ok()?,
                    table_offset: usize::try_from(table & !7).ok()?,
                    table_size: (control & 0x7FF).saturating_add(1),
                });
            }
            pointer = next;
            seen = seen.saturating_add(1);
        }
        None
    }

    /// Spawn a driver from a prepared launch and release the parent's handles.
    ///
    /// `HANDLE_TRANSFER` copies each handle into the child, so the parent closes its own values
    /// whether the spawn succeeds or fails. The child owns the transferred copies.
    fn spawn_prepared(initfs: &Initfs<'_>, prepared: &Prepared) -> Result<Handle, String> {
        let argv: Vec<&str> = prepared.argv.iter().map(String::as_str).collect();
        let mut sys = GrantSys {
            handles: prepared.handles.clone(),
        };
        let spawned = spawn_with(
            initfs,
            &prepared.exec,
            &SpawnOptions {
                args: &argv,
                env: &[],
            },
            &mut sys,
        );
        close_all(&prepared.handles);
        spawned.map_err(|error| alloc::format!("spawn: {error:?}"))
    }

    fn spawn_driver(
        client: &Client,
        initfs: &Initfs<'_>,
        device: &DeviceInfo,
        driver: &DriverSpec,
    ) -> Result<Handle, String> {
        let prepared = prepare_launch(client, device, driver)?;
        spawn_prepared(initfs, &prepared)
    }

    /// One launch/exit cycle for test mode.
    fn launch_driver(
        client: &Client,
        initfs: &Initfs<'_>,
        device: &DeviceInfo,
        driver: &DriverSpec,
    ) -> Result<i32, String> {
        let child = spawn_driver(client, initfs, device, driver)?;
        let mut code = -1i32;
        let waited = RtSys.proc_wait(child, &mut code);
        let _ = raven_rt::handle_close(child);
        waited.map_err(|error| alloc::format!("wait: {error:?}"))?;
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
        let manifest =
            manifest::parse(text).map_err(|error| alloc::format!("drivers.toml: {error}"))?;
        let drivers = parse_drivers(&manifest.drivers)
            .map_err(|error| alloc::format!("drivers.toml line {}: invalid", error.line))?;
        manifest::validate(&drivers, &manifest.abi)
            .map_err(|error| alloc::format!("drivers.toml: {error}"))?;
        Ok(drivers)
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
