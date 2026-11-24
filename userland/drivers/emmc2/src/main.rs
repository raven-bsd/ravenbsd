// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! EMMC2 userspace driver and `disk:` scheme entry.
//!
//! The real mode maps the EMMC2 register window and the DMA buffer through `raven_driver`, runs
//! the J1/J2 card initialisation, and serves `disk:` with SDMA transfers. The `test` argument
//! selects the host-backed mode used by the `pi4-emmc2` qtest: a memory disk with a deterministic
//! sector-stamped pattern and a checksum marker, no MMIO. Both modes use the same `BlockService`
//! and scheme handler.
//!
//! The board path is UNVERIFIED until a Pi 4 log exists; the qtest exercises the scheme, block
//! framing, pattern and checksum paths, not the silicon.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;

    use emmc2::block::{BlockDevice, BlockError, BlockPlan, BlockService, MemoryDisk};
    use emmc2::options::Options;
    use emmc2::pattern::{self, Hasher, SECTOR_SIZE};
    use emmc2::{CardReady, DriverError, Host, init_card};
    use raven_abi::errno::Errno;
    use raven_abi::types::clock;
    use raven_driver::{DmaRegion, Irq, MmioRegion};
    use raven_pi4::dma::{DmaError, plan_sdma};
    use raven_pi4::fdt::DmaWindow;
    use raven_pi4::registers::{
        AccessWidth, ERROR_STATUS_MASK, ErrorInterrupt, NORMAL_CMD_COMPLETE,
        NORMAL_ERROR_INTERRUPT, NORMAL_STATUS_MASK, NORMAL_TRANSFER_COMPLETE, NormalInterrupt,
        Quirks, Register,
    };
    use raven_pi4::timing::DmaMode;
    use raven_pi4::transfer::{TransferErrorKind, TransferEvent};
    use raven_proto_block::{BlockInfo, Op, Request, encode_reply};
    use raven_proto_fs::OpenFlags;
    use raven_rt::entry;
    use raven_scheme::{
        Client, Ctx, Handler, HandlerError, HandlerResult, OpenTable, Opened, Server,
    };

    entry!(main);

    /// Memory-disk size used by the test mode.
    const TEST_SECTORS: usize = 8192;
    /// First sector of the deterministic pattern.
    const PATTERN_FIRST: usize = 2048;
    /// Number of pattern sectors.
    const PATTERN_SECTORS: usize = 2048;
    /// Pattern fill byte.
    const PATTERN_FILL: u8 = 0xa5;
    /// Reserved marker sector.
    const MARKER_LBA: usize = 1900;
    /// The S11 `OPEN` opcode.
    const OPEN_OPCODE: u16 = 1;

    fn main(args: raven_rt::Args) -> i32 {
        match run(args) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("emmc2: {message}");
                1
            }
        }
    }

    fn run(args: raven_rt::Args) -> Result<(), String> {
        let args: Vec<&str> = args.iter().collect();
        let options = Options::parse(&args)?;
        if options.test {
            run_test(&options)
        } else {
            run_real(options)
        }
    }

    /// Host-backed qtest mode: a memory disk with a deterministic pattern and marker.
    /// Host-backed mode: a deterministic pattern disk or an image loaded from `initfs:`.
    fn run_test(options: &Options) -> Result<(), String> {
        let mut data = match &options.disk {
            Some(path) => {
                let image = load_image(path)?;
                raven_rt::println!(
                    "emmc2: backend=memory image={path} sectors={}",
                    image.len() / SECTOR_SIZE
                );
                image
            }
            None => {
                let mut data = vec![0u8; TEST_SECTORS * SECTOR_SIZE];
                let pattern_start = PATTERN_FIRST * SECTOR_SIZE;
                let pattern_end = (PATTERN_FIRST + PATTERN_SECTORS) * SECTOR_SIZE;
                let pattern_slice = data
                    .get_mut(pattern_start..pattern_end)
                    .ok_or_else(|| "pattern range".to_string())?;
                pattern::fill_pattern(PATTERN_FIRST as u64, PATTERN_FILL, pattern_slice);
                let mut hasher = Hasher::new();
                hasher.update(pattern_slice);
                let checksum = hasher.finish();
                let marker = pattern::marker_encode(checksum);
                let marker_start = MARKER_LBA * SECTOR_SIZE;
                let marker_end = marker_start + SECTOR_SIZE;
                data.get_mut(marker_start..marker_end)
                    .ok_or_else(|| "marker range".to_string())?
                    .copy_from_slice(&marker);
                raven_rt::println!("emmc2: backend=memory sectors={TEST_SECTORS}");
                raven_rt::println!(
                    "emmc2: pattern sectors={PATTERN_SECTORS} checksum={checksum:#018x}"
                );
                raven_rt::println!("emmc2: marker ok");
                data
            }
        };

        let disk = MemoryDisk::new(&mut data, false).map_err(describe)?;
        let service = BlockService::new(disk).map_err(describe_block)?;
        let mut handler = DiskHandler::new(service, !options.persist);
        serve(&mut handler)
    }

    /// Load a whole test image through the `file:`/`initfs:` client.
    fn load_image(path: &str) -> Result<Vec<u8>, String> {
        const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
        let client = Client::new();
        let handle = client
            .open(path, 1)
            .map_err(|error| format!("open {path}: {}", error.description()))?;
        let mut data = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = match client.read(handle, 0, &mut chunk) {
                Ok(read) => read,
                Err(error) => {
                    let _ = client.close(handle);
                    return Err(format!("read {path}: {}", error.description()));
                }
            };
            if read == 0 {
                break;
            }
            let next = data
                .len()
                .checked_add(read)
                .ok_or_else(|| "image length overflow".to_string())?;
            if next > MAX_IMAGE_BYTES {
                let _ = client.close(handle);
                return Err(format!("image {path} exceeds {MAX_IMAGE_BYTES} bytes"));
            }
            data.extend_from_slice(
                chunk
                    .get(..read)
                    .ok_or_else(|| "short image chunk".to_string())?,
            );
        }
        let _ = client.close(handle);
        if data.is_empty() || !data.len().is_multiple_of(SECTOR_SIZE) {
            return Err(format!(
                "image {path} is not a nonempty whole number of 512-byte sectors"
            ));
        }
        Ok(data)
    }

    /// Real path: map the device windows, initialise the card and serve SDMA transfers.
    fn run_real(options: Options) -> Result<(), String> {
        let mmio_window = raven_rt::map_mmio_mut(options.mmio, options.mmio_size)
            .map_err(|error| format!("mmio map: {}", error.description()))?;
        let mmio = MmioRegion::new(mmio_window);
        let mut host = MmioHost {
            mmio,
            window: options.window,
            clock_hz: options.clock_hz,
            faults: 0,
        };
        let ready = init_card(&mut host).map_err(describe_driver)?;
        host.write(
            Register::HostControl,
            u32::from(ready.timing.host_control(Quirks::PI4_EMMC2, DmaMode::Sdma)),
        );
        let normal = NORMAL_CMD_COMPLETE | NORMAL_TRANSFER_COMPLETE | NORMAL_ERROR_INTERRUPT;
        host.write(Register::NormalInterruptStatusEnable, u32::from(normal));
        host.write(Register::NormalInterruptSignalEnable, u32::from(normal));
        host.write(
            Register::ErrorInterruptStatusEnable,
            u32::from(ERROR_STATUS_MASK),
        );
        host.write(
            Register::ErrorInterruptSignalEnable,
            u32::from(ERROR_STATUS_MASK),
        );
        let dma_info = raven_rt::dma_info(options.dma)
            .map_err(|error| format!("dma info: {}", error.description()))?;
        let dma_window = raven_rt::map_mmio_mut(options.dma, options.dma_size)
            .map_err(|error| format!("dma map: {}", error.description()))?;
        let dma = DmaRegion::new(dma_window, dma_info.bus_addr);
        let irq = Irq::new(options.irq);
        let capacity = ready.card.capacity_bytes / 512u64;
        if capacity == 0 {
            return Err("card reports zero capacity".to_string());
        }
        raven_rt::println!(
            "emmc2: card rca={:#06x} capacity={} high_capacity={}",
            ready.card.rca,
            capacity,
            ready.card.high_capacity
        );
        raven_rt::println!(
            "emmc2: dma bus=0x{:x} len={} window={:#x}/{:#x}/{:#x}",
            dma_info.bus_addr,
            dma_info.len,
            options.window.bus_base,
            options.window.cpu_base,
            options.window.size
        );
        let device = CardDevice {
            host,
            dma,
            irq,
            ready,
            info: BlockInfo {
                sector_size: 512,
                read_only: 0,
                sectors: capacity,
                reserved: 0,
            },
        };
        let service = BlockService::new(device).map_err(describe_block)?;
        let mut handler = DiskHandler::new(service, false);
        serve(&mut handler)
    }

    /// The EMMC2 register and DMA-window seam over the runtime mappings.
    struct MmioHost {
        mmio: MmioRegion,
        window: DmaWindow,
        clock_hz: Option<u32>,
        faults: u32,
    }

    impl Host for MmioHost {
        fn read(&mut self, register: Register) -> u32 {
            let offset = register.offset();
            let value = match register.width() {
                AccessWidth::Byte => self.mmio.read_u8(offset).map(u32::from),
                AccessWidth::HalfWord => self.mmio.read_u16(offset).map(u32::from),
                AccessWidth::Word => self.mmio.read_u32(offset),
            };
            match value {
                Ok(value) => value,
                Err(_) => {
                    self.faults = self.faults.saturating_add(1);
                    0
                }
            }
        }

        fn write(&mut self, register: Register, value: u32) {
            let offset = register.offset();
            let result = match register.width() {
                AccessWidth::Byte => self.mmio.write_u8(offset, value as u8),
                AccessWidth::HalfWord => self.mmio.write_u16(offset, value as u16),
                AccessWidth::Word => self.mmio.write_u32(offset, value),
            };
            if result.is_err() {
                self.faults = self.faults.saturating_add(1);
            }
        }

        fn now_ns(&mut self) -> u64 {
            match raven_rt::clock_get(clock::MONOTONIC) {
                Ok(time) => {
                    let seconds = u64::try_from(time.tv_sec).unwrap_or(0);
                    let nanos = u64::try_from(time.tv_nsec).unwrap_or(0);
                    seconds.saturating_mul(1_000_000_000).saturating_add(nanos)
                }
                // A broken clock must fail a deadline, not hang the poll.
                Err(_) => u64::MAX,
            }
        }

        fn clock_hz(&mut self) -> Option<u32> {
            self.clock_hz
        }

        fn dma_window(&self) -> DmaWindow {
            self.window
        }
    }

    /// The real card backend: SDMA through the DMA mapping and the interrupt line.
    struct CardDevice {
        host: MmioHost,
        dma: DmaRegion,
        irq: Irq,
        ready: CardReady,
        info: BlockInfo,
    }

    impl CardDevice {
        fn transfer_read(&mut self, plan: &BlockPlan, buffer: &mut [u8]) -> Result<(), Errno> {
            if buffer.len() != plan.bytes() {
                return Err(Errno::EINVAL);
            }
            self.run(plan)?;
            self.dma.acquire();
            self.dma.read_bytes(0, buffer).map_err(|_| Errno::EIO)
        }

        fn transfer_write(&mut self, plan: &BlockPlan, data: &[u8]) -> Result<(), Errno> {
            let length = plan.bytes();
            if data.len() != length || length > self.dma.len() {
                return Err(Errno::EINVAL);
            }
            self.dma.write_bytes(0, data).map_err(|_| Errno::EIO)?;
            self.dma.publish();
            self.run(plan)
        }

        fn run(&mut self, plan: &BlockPlan) -> Result<(), Errno> {
            let length = plan.bytes();
            if length > self.dma.len() {
                return Err(Errno::EINVAL);
            }
            let window = self.ready.plan.dma;
            let bus = self.dma.bus_address();
            let offset = window
                .bus_base
                .checked_sub(window.cpu_base)
                .ok_or(Errno::EINVAL)?;
            let cpu = bus.checked_sub(offset).ok_or(Errno::EINVAL)?;
            let sdma = plan_sdma(window, cpu, length).map_err(dma_errno)?;
            let transfer = plan.transfer_flags().with_dma();
            let encoded = plan
                .command()
                .encode(Some(transfer))
                .map_err(|_| Errno::EINVAL)?;

            self.host.write(
                Register::NormalInterruptStatus,
                u32::from(NORMAL_STATUS_MASK),
            );
            self.host
                .write(Register::ErrorInterruptStatus, u32::from(ERROR_STATUS_MASK));
            self.host.write(Register::DmaAddress, sdma.bus_address);
            self.host
                .write(Register::BlockSize, u32::from(encoded.block_size));
            self.host
                .write(Register::BlockCount, u32::from(encoded.block_count));
            self.host
                .write(Register::TransferMode, u32::from(encoded.transfer_mode));
            self.host.write(Register::Argument, encoded.argument);
            let mut state = plan.transfer();
            self.host
                .write(Register::Command, u32::from(encoded.command));

            let mut timeouts = 0u32;
            loop {
                let normal = NormalInterrupt::from_raw(
                    self.host.read(Register::NormalInterruptStatus) as u16,
                );
                let errors =
                    ErrorInterrupt::from_raw(self.host.read(Register::ErrorInterruptStatus) as u16);
                let event = state
                    .poll(normal, errors)
                    .map_err(|error| transfer_errno(error.kind))?;
                self.host
                    .write(Register::NormalInterruptStatus, u32::from(normal.raw()));
                self.host
                    .write(Register::ErrorInterruptStatus, u32::from(errors.raw()));
                if matches!(event, TransferEvent::Complete { .. }) {
                    break;
                }
                match self.irq.wait(1_000_000_000) {
                    Ok(0) => {
                        timeouts = timeouts.saturating_add(1);
                        if timeouts >= 3 {
                            return Err(Errno::ETIMEDOUT);
                        }
                    }
                    Ok(_) => {}
                    Err(_) => return Err(Errno::EIO),
                }
                self.irq.ack().map_err(|_| Errno::EIO)?;
            }
            Ok(())
        }
    }

    impl BlockDevice for CardDevice {
        fn info(&self) -> BlockInfo {
            self.info
        }

        fn read(&mut self, plan: &BlockPlan, buffer: &mut [u8]) -> Result<(), Errno> {
            self.transfer_read(plan, buffer)
        }

        fn write(&mut self, plan: &BlockPlan, data: &[u8]) -> Result<(), Errno> {
            self.transfer_write(plan, data)
        }

        fn flush(&mut self) -> Result<(), Errno> {
            Ok(())
        }
    }

    /// The `disk:` handler over the block service.
    struct DiskHandler<D: BlockDevice> {
        service: BlockService<D>,
        opens: OpenTable<()>,
        pending: Option<(u64, String)>,
        shutdown_on_flush: bool,
        shutdown: bool,
    }

    impl<D: BlockDevice> DiskHandler<D> {
        fn new(service: BlockService<D>, shutdown_on_flush: bool) -> Self {
            Self {
                service,
                opens: OpenTable::new(),
                pending: None,
                shutdown_on_flush,
                shutdown: false,
            }
        }

        fn expect_open(&mut self, file_id: u64, path: &str) {
            self.pending = Some((file_id, path.to_string()));
        }

        fn shutdown_requested(&self) -> bool {
            self.shutdown
        }

        fn reply_error(&self, response: &mut [u8], error: Errno) -> HandlerResult<usize> {
            encode_reply(response, -i32::from(error.0), &[])
                .map_err(|_| HandlerError::from(Errno::EPROTO))
        }
    }

    impl<D: BlockDevice> Handler for DiskHandler<D> {
        fn open_with(
            &mut self,
            path: &str,
            flags: OpenFlags,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let Some((file_id, expected)) = self.pending.take() else {
                return Err(Errno::EINVAL.into());
            };
            if expected != path || (!path.is_empty() && path != "0") {
                return Err(Errno::ENOENT.into());
            }
            if !matches!(
                flags,
                OpenFlags::READ | OpenFlags::WRITE | OpenFlags::READ_WRITE
            ) {
                return Err(Errno::EINVAL.into());
            }
            if self.service.info().read_only != 0 && flags.writable() {
                return Err(Errno::EROFS.into());
            }
            if self.opens.len() >= 16 {
                return Err(Errno::EMFILE.into());
            }
            self.opens.insert_with_id(file_id, (), flags)?;
            Ok(Opened::File(file_id))
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.opens.close(file)?;
            Ok(())
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            Ok(self.opens.dup(file)?)
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            let flags = self.opens.get(file).ok_or(Errno::EBADF)?.flags;
            let decoded = Request::decode(request);
            if let Ok(decoded) = decoded {
                if decoded.op == Op::Read && !flags.readable() {
                    return self.reply_error(response, Errno::EBADF);
                }
                if decoded.op == Op::Write && !flags.writable() {
                    return self.reply_error(response, Errno::EBADF);
                }
            }
            let written = self
                .service
                .handle(request, response)
                .map_err(|_| HandlerError::from(Errno::EPROTO))?;
            if self.shutdown_on_flush
                && let Ok(decoded) = decoded
                && decoded.op == Op::Flush
            {
                self.shutdown = true;
                // Print before the flush reply so the qtest sees the end marker even if svcd
                // reaps this process as soon as the one-shot probe exits.
                raven_rt::println!("emmc2: serve done");
            }
            Ok(written)
        }
    }

    fn serve<D: BlockDevice>(handler: &mut DiskHandler<D>) -> Result<(), String> {
        let mut server = Server::register("disk")
            .map_err(|error| format!("register disk: {}", error.description()))?;
        raven_rt::println!("emmc2: registered disk");
        raven_rt::println!("emmc2: serving");
        loop {
            let Some(packet) = server.poll(None) else {
                continue;
            };
            if packet.packet.opcode == OPEN_OPCODE
                && let Ok(path) = core::str::from_utf8(packet.packet.path_bytes())
            {
                handler.expect_open(packet.packet.file_id, path);
            }
            let _ = server.dispatch(packet, handler);
            if handler.shutdown_requested() {
                break;
            }
        }
        Ok(())
    }

    fn describe(error: Errno) -> String {
        error.description().to_string()
    }

    fn describe_block(error: BlockError) -> String {
        format!("block service: {error:?}")
    }

    fn describe_driver(error: DriverError) -> String {
        format!("card init: {error:?}")
    }

    fn dma_errno(error: DmaError) -> Errno {
        match error {
            DmaError::MisalignedAddress { .. } | DmaError::MisalignedLength { .. } => Errno::EINVAL,
            DmaError::EmptyBuffer | DmaError::DescriptorLength { .. } => Errno::EINVAL,
            _ => Errno::EIO,
        }
    }

    fn transfer_errno(kind: TransferErrorKind) -> Errno {
        match kind {
            TransferErrorKind::CommandTimeout | TransferErrorKind::DataTimeout => Errno::ETIMEDOUT,
            TransferErrorKind::CommandCrc
            | TransferErrorKind::CommandEndBit
            | TransferErrorKind::CommandIndex
            | TransferErrorKind::DataCrc
            | TransferErrorKind::Adma
            | TransferErrorKind::Dma
            | TransferErrorKind::AutoCmd12
            | TransferErrorKind::Other => Errno::EIO,
            _ => Errno::EINVAL,
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
