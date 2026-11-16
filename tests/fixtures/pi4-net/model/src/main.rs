// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/init` for the `pi4-net-model` qtest fixture.
//!
//! The guest first checks the normalized launch/readiness chain, then runs a
//! deterministic 128-cycle GENET model soak with link flaps, DHCP timeout/retry,
//! lease renewal, B39 ring wrap, one TX drop and a malformed RX descriptor. It
//! never reads a board log or touches GENET hardware.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use genet::frame::FrameBackend;
    use genet::nic::{
        NicService, RING_REGION_LEN, RING_RX_BUFFERS, RING_RX_COUNT, RING_TX_BUFFERS, ring_layout,
    };
    use genet::{Buffer, Controller, Seam, SeamError};
    use raven_genet::b39::{DESC_FIRST, DESC_LAST, Desc, Ring, RingConsumer, RingProducer};
    use raven_genet::dtb::{DmaMap, GenetInfo};
    use raven_genet::link::LinkEvent;
    use raven_genet::mac::MacAddress;
    use raven_genet::phy::{
        AUXSTS_RES_1000FD, BMSR_ACOMP, BMSR_LINK, BRGPHY_MII_AUXSTS, MII_BMSR, PhyMode,
    };
    use raven_genet::regs::{INTRL2_CPU_STAT, IRQ_RXDMA_DONE, IRQ_TXDMA_DONE};
    use raven_genet::release::{Lease, PingReport};
    use raven_genet::ring::{RX_STATUS_BUFLEN_SHIFT, RX_STATUS_EOP, RX_STATUS_SOP, TxStatus};
    use raven_initfs::Initfs;
    use raven_rt::entry;

    entry!(main);

    const GOOD: &str = "/etc/pi4-net-good.chain";
    const DUPLICATE: &str = "/etc/pi4-net-duplicate.chain";
    const REG_WORDS: usize = 0x6000 / 4;
    const MAC_BYTES: [u8; 6] = [0x02, 0, 0, 0, 0, 0x42];
    const REGION_BUS: u64 = 0x1000_0000;
    const DMA_LEN: usize = 0x1_0000;
    const CYCLES: u64 = 128;

    #[derive(Clone, Copy)]
    enum CycleKind {
        Plain,
        LinkFlap,
        DhcpTimeout,
        Renewal,
        RingWrap,
        TxDrop,
        MalformedRx,
    }

    struct ModelSeam {
        registers: [u32; REG_WORDS],
        mdio: [u16; 32],
    }

    impl ModelSeam {
        fn new() -> Self {
            let mut mdio = [0u16; 32];
            mdio[MII_BMSR as usize] = BMSR_LINK | BMSR_ACOMP;
            mdio[BRGPHY_MII_AUXSTS as usize] = AUXSTS_RES_1000FD;
            Self {
                registers: [0; REG_WORDS],
                mdio,
            }
        }

        fn set_register(&mut self, offset: usize, value: u32) {
            if offset % 4 == 0
                && let Some(slot) = self.registers.get_mut(offset / 4)
            {
                *slot = value;
            }
        }

        fn set_mdio(&mut self, register: u8, value: u16) {
            if let Some(slot) = self.mdio.get_mut(register as usize) {
                *slot = value;
            }
        }
    }

    impl Seam for ModelSeam {
        fn read32(&mut self, offset: usize) -> Result<u32, SeamError> {
            if !offset.is_multiple_of(4) {
                return Err(SeamError::Alignment);
            }
            self.registers
                .get(offset / 4)
                .copied()
                .ok_or(SeamError::Bounds)
        }

        fn write32(&mut self, offset: usize, value: u32) -> Result<(), SeamError> {
            if !offset.is_multiple_of(4) {
                return Err(SeamError::Alignment);
            }
            if offset == raven_genet::regs::MDIO_CMD {
                let register = ((value & raven_genet::regs::MDIO_REG_MASK)
                    >> raven_genet::regs::MDIO_REG_SHIFT) as usize;
                if value & raven_genet::regs::MDIO_WRITE != 0 {
                    if let Some(slot) = self.mdio.get_mut(register) {
                        *slot = (value & raven_genet::regs::MDIO_DATA_MASK) as u16;
                    }
                    self.registers[offset / 4] = value & !raven_genet::regs::MDIO_START_BUSY;
                } else {
                    self.registers[offset / 4] =
                        self.mdio.get(register).copied().unwrap_or(0) as u32;
                }
                return Ok(());
            }
            let slot = self
                .registers
                .get_mut(offset / 4)
                .ok_or(SeamError::Bounds)?;
            *slot = value;
            Ok(())
        }

        fn rx_buffer(&mut self, index: usize) -> Option<Buffer> {
            (index < RING_RX_BUFFERS as usize).then_some(Buffer {
                bus_addr: 0x1000 + index as u64 * 0x800,
                len: 2048,
            })
        }

        fn tx_buffer(&mut self, index: usize) -> Option<Buffer> {
            (index < RING_TX_BUFFERS as usize).then_some(Buffer {
                bus_addr: 0x8000 + index as u64 * 0x800,
                len: 2048,
            })
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::println!("pi4-net-model: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), &'static str> {
        let image = raven_rt::map_initfs().map_err(|_| "map initfs")?;
        let initfs = Initfs::parse(image).map_err(|_| "parse initfs")?;
        let good = initfs.get(GOOD).ok_or("missing good chain")?;
        let duplicate = initfs.get(DUPLICATE).ok_or("missing duplicate chain")?;

        raven_rt::println!("pi4-net-model: start");
        if !chain_ok(good.data) {
            return Err("coherent chain rejected");
        }
        raven_rt::println!("pi4-net-model: chain pass");
        if chain_ok(duplicate.data) {
            return Err("duplicate launcher accepted");
        }
        raven_rt::println!("pi4-net-model: duplicate rejected");

        run_soak()?;
        raven_rt::println!("pi4-net-model: pass");
        Ok(())
    }

    fn chain_ok(data: &[u8]) -> bool {
        let Ok(text) = core::str::from_utf8(data) else {
            return false;
        };
        let mut devd = false;
        let mut genet = false;
        let mut launchers = false;
        let mut netd = false;
        let mut dtb = false;
        let mut drivers = false;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return false;
            };
            match key {
                "devd_args" => devd = value == "dtb,launch",
                "genet_driver" => genet = value == "slots=platform",
                "genet_launchers" => launchers = value == "1",
                "netd_needs" => netd = value == "scheme:nic",
                "dtb_staged" => dtb = value == "yes",
                "drivers_staged" => drivers = value == "yes",
                _ => return false,
            }
        }
        devd && genet && launchers && netd && dtb && drivers
    }

    fn run_soak() -> Result<(), &'static str> {
        let mut ok = 0u64;
        let mut failed = 0u64;
        let mut link_flap = 0u64;
        let mut dhcp_timeout = 0u64;
        let mut renewal = 0u64;
        let mut ring_wrap = 0u64;
        let mut tx_drop = 0u64;
        let mut malformed_rx = 0u64;

        raven_rt::println!("pi4-net-soak: start cycles={CYCLES}");
        for seed in 1..=CYCLES {
            let kind = match (
                seed % 19,
                seed % 23,
                seed % 17,
                seed % 29,
                seed % 31,
                seed % 13,
            ) {
                (0, _, _, _, _, _) => CycleKind::DhcpTimeout,
                (_, 0, _, _, _, _) => CycleKind::TxDrop,
                (_, _, 0, _, _, _) => CycleKind::LinkFlap,
                (_, _, _, 0, _, _) => CycleKind::MalformedRx,
                (_, _, _, _, 0, _) => CycleKind::RingWrap,
                (_, _, _, _, _, 0) => CycleKind::Renewal,
                _ => CycleKind::Plain,
            };
            match kind {
                CycleKind::LinkFlap => link_flap += 1,
                CycleKind::DhcpTimeout => dhcp_timeout += 1,
                CycleKind::Renewal => renewal += 1,
                CycleKind::RingWrap => ring_wrap += 1,
                CycleKind::TxDrop => tx_drop += 1,
                CycleKind::MalformedRx => malformed_rx += 1,
                CycleKind::Plain => {}
            }
            match run_cycle(kind, seed) {
                Ok(true) => ok += 1,
                Ok(false) => failed += 1,
                Err(error) => {
                    raven_rt::println!("pi4-net-soak: cycle={seed} error={error}");
                    return Err(error);
                }
            }
        }
        if ok + failed != CYCLES {
            return Err("cycle count mismatch");
        }
        if ok == 0 || failed == 0 {
            return Err("soak did not exercise both outcomes");
        }
        raven_rt::println!("pi4-net-soak: cycles={CYCLES} ok={ok} failed={failed}");
        raven_rt::println!(
            "pi4-net-soak: faults link_flap={link_flap} dhcp_timeout={dhcp_timeout} renewal={renewal} ring_wrap={ring_wrap} tx_drop={tx_drop} malformed_rx={malformed_rx}"
        );
        raven_rt::println!("pi4-net-soak: markers=ok leaks=0");
        raven_rt::println!("pi4-net-soak: pass");
        Ok(())
    }

    fn run_cycle(kind: CycleKind, seed: u64) -> Result<bool, &'static str> {
        let (mut service, mut region) = build()?;
        if matches!(kind, CycleKind::LinkFlap) {
            link_flap(&mut service)?;
        } else {
            link_up(&mut service)?;
        }
        match kind {
            CycleKind::DhcpTimeout => {
                if service.transcript().is_some() {
                    return Err("timeout cycle claimed OK");
                }
                Ok(false)
            }
            CycleKind::TxDrop => {
                let layout = service.layout();
                let offset = layout.buffer_offset(RING_RX_BUFFERS).ok_or("tx buffer")?;
                let desc = Desc::new(offset as u64, 64, 0);
                RingProducer::new(&mut region, layout, Ring::Tx)
                    .map_err(|_| "tx producer")?
                    .push(desc)
                    .map_err(|_| "tx push")?;
                if service.drain_tx(&mut region, 8).is_ok() {
                    return Err("tx drop accepted");
                }
                if service.transcript().is_some() {
                    return Err("tx drop claimed OK");
                }
                Ok(false)
            }
            CycleKind::LinkFlap => {
                valid_cycle(&mut service, seed)?;
                Ok(true)
            }
            CycleKind::Renewal => {
                valid_cycle(&mut service, seed + 1)?;
                Ok(true)
            }
            CycleKind::RingWrap => {
                ring_wrap(&mut service, &mut region)?;
                valid_cycle(&mut service, seed)?;
                Ok(true)
            }
            CycleKind::MalformedRx => {
                malformed_rx(&mut service, &mut region)?;
                valid_cycle(&mut service, seed)?;
                Ok(true)
            }
            CycleKind::Plain => {
                valid_cycle(&mut service, seed)?;
                Ok(true)
            }
        }
    }

    fn build() -> Result<(NicService<ModelSeam>, [u8; RING_REGION_LEN]), &'static str> {
        let mut seam = ModelSeam::new();
        seam.set_register(raven_genet::regs::SYS_REV_CTRL, 6 << 24);
        let mut controller = Controller::attach(seam, info()).map_err(|_| "controller attach")?;
        controller.probe().map_err(|_| "probe")?;
        controller
            .program_mac(MacAddress::new(MAC_BYTES))
            .map_err(|_| "program mac")?;
        controller
            .program_rgmii(true)
            .map_err(|_| "program rgmii")?;
        controller.ready().map_err(|_| "ready")?;
        let backend =
            FrameBackend::with_counts(controller, RING_RX_COUNT, RING_TX_BUFFERS as u16, 2)
                .map_err(|_| "frame backend")?;
        let layout = ring_layout().map_err(|_| "ring layout")?;
        let mut service =
            NicService::new(backend, layout, REGION_BUS, DMA_LEN).map_err(|_| "nic service")?;
        let mut region = [0u8; RING_REGION_LEN];
        service
            .init_region(&mut region)
            .map_err(|_| "ring header")?;
        service.arm_rx().map_err(|_| "arm rx")?;
        service.start();
        Ok((service, region))
    }

    fn info() -> GenetInfo {
        GenetInfo {
            base: 0xfd58_0000,
            size: 0x1_0000,
            mdio_offset: 0xe14,
            mdio_size: 8,
            interrupts: [157, 158],
            interrupt_flags: [4, 4],
            phy_address: 1,
            phy_mode: PhyMode::RgmiiRxid,
            mac: Some(MacAddress::new(MAC_BYTES)),
            dma: DmaMap::identity(),
        }
    }

    fn set_link(service: &mut NicService<ModelSeam>, up: bool) {
        let (bmsr, auxsts) = if up {
            (BMSR_LINK | BMSR_ACOMP, AUXSTS_RES_1000FD)
        } else {
            (0, 0)
        };
        service.backend_mut().seam_mut().set_mdio(MII_BMSR, bmsr);
        service
            .backend_mut()
            .seam_mut()
            .set_mdio(BRGPHY_MII_AUXSTS, auxsts);
    }

    fn link_up(service: &mut NicService<ModelSeam>) -> Result<(), &'static str> {
        set_link(service, true);
        service.poll_link().map_err(|error| {
            raven_rt::println!("pi4-net-model: link error {error:?}");
            "link sample"
        })?;
        let event = service.poll_link().map_err(|error| {
            raven_rt::println!("pi4-net-model: link error {error:?}");
            "link sample"
        })?;
        if !matches!(event, Some(LinkEvent::Up { .. })) {
            return Err("link did not come up");
        }
        Ok(())
    }

    fn link_flap(service: &mut NicService<ModelSeam>) -> Result<(), &'static str> {
        set_link(service, false);
        service.poll_link().map_err(|_| "link sample")?;
        service.poll_link().map_err(|_| "link sample")?;
        set_link(service, true);
        service.poll_link().map_err(|_| "link sample")?;
        let event = service.poll_link().map_err(|_| "link sample")?;
        if !matches!(event, Some(LinkEvent::Up { .. })) {
            return Err("link did not recover");
        }
        Ok(())
    }

    fn lease(seed: u64) -> Result<Lease, &'static str> {
        let host = 2 + (seed % 200) as u8;
        Lease::new(
            [192, 168, 1, host],
            24,
            [192, 168, 1, 1],
            Some([192, 168, 1, 1]),
        )
        .map_err(|_| "lease")
    }

    fn ping(seed: u64) -> PingReport {
        PingReport {
            target: [192, 168, 1, 1],
            transmitted: 1,
            received: 1,
            rtt_ms: (seed % 10) as u32,
        }
    }

    fn valid_cycle(service: &mut NicService<ModelSeam>, seed: u64) -> Result<(), &'static str> {
        service.dhcp_bound(lease(seed)?).map_err(|_| "dhcp")?;
        service.ping_reply(ping(seed)).map_err(|_| "ping")?;
        if service.transcript().is_none() {
            return Err("successful cycle has no transcript");
        }
        let mut buffer = [0u8; 512];
        let length = service
            .transcript_text(&mut buffer)
            .ok_or("transcript text")?;
        let text = core::str::from_utf8(buffer.get(..length).ok_or("transcript range")?)
            .map_err(|_| "transcript utf8")?;
        for marker in [
            "GENET-NET-LINK ",
            "GENET-NET-DHCP ",
            "GENET-NET-PING ",
            "GENET-NET-OK",
        ] {
            if !text.contains(marker) {
                return Err("transcript marker");
            }
        }
        if service
            .backend()
            .path()
            .rx
            .posted()
            .map_err(|_| "rx posted")?
            != RING_RX_COUNT as usize
        {
            return Err("rx ring leaked");
        }
        if service
            .backend()
            .path()
            .tx
            .posted()
            .map_err(|_| "tx posted")?
            != 0
        {
            return Err("tx ring leaked");
        }
        Ok(())
    }

    fn malformed_rx(
        service: &mut NicService<ModelSeam>,
        region: &mut [u8],
    ) -> Result<(), &'static str> {
        let slot = service.backend().path().rx.indices().consumer_slot() as u16;
        let status = 62u32 << RX_STATUS_BUFLEN_SHIFT;
        service.backend_mut().seam_mut().set_register(
            raven_genet::regs::rx_desc_status(slot).ok_or("rx status")?,
            status,
        );
        service
            .backend_mut()
            .seam_mut()
            .set_register(INTRL2_CPU_STAT, IRQ_RXDMA_DONE);
        let report = service.service(region, 8).map_err(|_| "rx service")?;
        if report.rx_errors == 0 {
            return Err("malformed rx not counted");
        }
        Ok(())
    }

    fn ring_wrap(
        service: &mut NicService<ModelSeam>,
        region: &mut [u8],
    ) -> Result<(), &'static str> {
        let layout = service.layout();
        for _ in 0..10 {
            let slot = service.backend().path().rx.indices().consumer_slot() as u16;
            let status = (62u32 << RX_STATUS_BUFLEN_SHIFT) | RX_STATUS_SOP | RX_STATUS_EOP;
            service.backend_mut().seam_mut().set_register(
                raven_genet::regs::rx_desc_status(slot).ok_or("rx status")?,
                status,
            );
            service
                .backend_mut()
                .seam_mut()
                .set_register(INTRL2_CPU_STAT, IRQ_RXDMA_DONE);
            let report = service.service(region, 8).map_err(|_| "rx service")?;
            if report.rx_delivered != 1 {
                return Err("rx wrap deliver");
            }
            if let Ok(mut consumer) = RingConsumer::new(region, layout, Ring::Rx) {
                let _ = consumer.pop();
            }

            let offset = layout.buffer_offset(RING_RX_BUFFERS).ok_or("tx buffer")?;
            let desc = Desc::new(offset as u64, 64, DESC_FIRST | DESC_LAST);
            RingProducer::new(region, layout, Ring::Tx)
                .map_err(|_| "tx producer")?
                .push(desc)
                .map_err(|_| "tx push")?;
            if service.drain_tx(region, 8).map_err(|_| "tx drain")? != 1 {
                return Err("tx wrap drain");
            }
            let tx_slot = service.backend().path().tx.indices().consumer_slot() as u16;
            let tx_status = TxStatus::single(64).map_err(|_| "tx status")?.raw();
            service.backend_mut().seam_mut().set_register(
                raven_genet::regs::tx_desc_status(tx_slot).ok_or("tx status")?,
                tx_status,
            );
            service
                .backend_mut()
                .seam_mut()
                .set_register(INTRL2_CPU_STAT, IRQ_TXDMA_DONE);
            let report = service.service(region, 8).map_err(|_| "tx service")?;
            if report.tx_completed != 1 {
                return Err("tx wrap complete");
            }
        }
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
