// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the B39 `nic:` frame service.

use super::*;
use crate::frame::FrameBackend;
use crate::std::string::ToString;
use crate::tests::ready_controller;
use raven_genet::b39::{Desc, Ring, RingConsumer, RingHeader, RingProducer};
use raven_genet::phy::{AUXSTS_RES_1000FD, BMSR_ACOMP, BMSR_LINK, BRGPHY_MII_AUXSTS, MII_BMSR};
use raven_genet::regs::{INTRL2_CPU_CLEAR, INTRL2_CPU_STAT, IRQ_RXDMA_DONE, IRQ_TXDMA_DONE};
use raven_genet::ring::{
    RX_STATUS_BUFLEN_SHIFT, RX_STATUS_EOP, RX_STATUS_OWN, RX_STATUS_SOP, TX_STATUS_BUFLEN_MASK,
    TX_STATUS_BUFLEN_SHIFT, TX_STATUS_OWN, TxStatus,
};

const REGION_BUS: u64 = 0x1000_0000;
const DMA_LEN: usize = 0x1_0000;

fn backend(
    controller: crate::Controller<crate::tests::FakeSeam>,
) -> FrameBackend<crate::tests::FakeSeam> {
    FrameBackend::with_counts(controller, RING_RX_COUNT, RING_TX_BUFFERS as u16, 2)
        .expect("backend")
}

fn service() -> NicService<crate::tests::FakeSeam> {
    NicService::new(
        backend(ready_controller()),
        ring_layout().expect("layout"),
        REGION_BUS,
        DMA_LEN,
    )
    .expect("service")
}

fn started() -> (NicService<crate::tests::FakeSeam>, [u8; RING_REGION_LEN]) {
    let mut service = service();
    let mut region = [0u8; RING_REGION_LEN];
    service.init_region(&mut region).expect("header");
    service.arm_rx().expect("arm");
    service.start();
    (service, region)
}

fn push_tx(region: &mut [u8], layout: RingLayout, buffer: usize, len: usize) {
    let offset = layout.buffer_offset(buffer).expect("buffer");
    let desc = Desc::new(
        offset as u64,
        u32::try_from(len).expect("length"),
        DESC_FIRST | DESC_LAST,
    );
    RingProducer::new(region, layout, Ring::Tx)
        .expect("producer")
        .push(desc)
        .expect("push");
}

fn lease() -> Lease {
    Lease::new(
        [192, 168, 1, 23],
        24,
        [192, 168, 1, 1],
        Some([192, 168, 1, 1]),
    )
    .expect("lease")
}

fn ping() -> PingReport {
    PingReport {
        target: [192, 168, 1, 1],
        transmitted: 3,
        received: 3,
        rtt_ms: 0,
    }
}

#[test]
fn the_control_info_matches_the_netd_text_contract() {
    let service = service();
    let mut buffer = [0u8; 256];
    let length = service.info().encode(&mut buffer).expect("encode");
    assert_eq!(
        &buffer[..length],
        b"mac=02:00:00:00:00:42\n\
          mtu=1500\n\
          link=down\n\
          ring_offset=4096\n\
          ring_len=24576\n\
          rx=8\n\
          tx=8\n\
          pool=22528\n\
          dma=65536\n"
    );
}

#[test]
fn the_layout_maps_eight_rx_and_three_tx_buffers() {
    let layout = ring_layout().expect("layout");
    let platform = platform_layout(layout, DMA_LEN).expect("platform");
    assert_eq!(
        platform.rx_offset,
        RING_REGION_OFFSET + layout.pool_offset()
    );
    assert_eq!(
        platform.tx_offset,
        RING_REGION_OFFSET + layout.buffer_offset(RING_RX_BUFFERS).expect("tx start")
    );
    assert_eq!(platform.rx_count, 8);
    assert_eq!(platform.tx_count, 3);
    assert_eq!(platform.buffer_len, 2048);
    assert_eq!(
        platform.tx_offset + platform.tx_count * platform.stride,
        RING_REGION_OFFSET + RING_REGION_LEN
    );
    assert_eq!(platform_layout(layout, 0x6000), Err(NicError::BadLayout));
}

#[test]
fn init_region_writes_a_valid_b39_header() {
    let service = service();
    let mut region = [0u8; RING_REGION_LEN];
    service.init_region(&mut region).expect("header");
    let header = RingHeader::read_from(&region).expect("read header");
    assert_eq!(header.rx_count, RING_RX_COUNT);
    assert_eq!(header.tx_count, RING_TX_COUNT);
    assert_eq!(header.pool_offset as usize, service.layout().pool_offset());
    assert_eq!(header.pool_len as usize, service.layout().pool_len());
}

#[test]
fn arming_rx_posts_eight_owned_descriptors() {
    let (mut service, _region) = started();
    assert_eq!(
        service.backend().path().rx.posted().expect("posted"),
        usize::from(RING_RX_COUNT)
    );
    let status = service
        .backend()
        .controller()
        .seam
        .register(raven_genet::regs::rx_desc_status(0).expect("status"));
    assert_eq!(status & RX_STATUS_OWN, RX_STATUS_OWN);
    assert_eq!(
        service.backend().controller().seam.register(
            raven_genet::regs::rx_dma_prod_index(raven_genet::regs::DEFAULT_QUEUE).expect("prod")
        ),
        8
    );
    assert_eq!(
        service.backend().controller().seam.register(
            raven_genet::regs::rx_dma_cons_index(raven_genet::regs::DEFAULT_QUEUE).expect("cons")
        ),
        0
    );
    assert_eq!(service.arm_rx().expect("second arm"), 0);
}

#[test]
fn an_rx_completion_is_published_to_the_b39_ring() {
    let (mut service, mut region) = started();
    let layout = service.layout();
    let offset = layout.buffer_offset(0).expect("buffer");
    for byte in &mut region[offset..offset + 60] {
        *byte = 0x5a;
    }
    let raw = (62u32 << RX_STATUS_BUFLEN_SHIFT) | RX_STATUS_SOP | RX_STATUS_EOP;
    service
        .backend_mut()
        .controller_mut()
        .seam
        .set_register(raven_genet::regs::rx_desc_status(0).expect("status"), raw);
    service
        .backend_mut()
        .controller_mut()
        .seam
        .set_register(INTRL2_CPU_STAT, IRQ_RXDMA_DONE);

    let report = service.service(&mut region, 8).expect("service");
    assert_eq!(report.rx_delivered, 1);
    assert_eq!(report.rx_refilled, 1);
    assert_eq!(report.rx_errors, 0);
    assert_eq!(
        service.backend().path().rx.posted().expect("posted"),
        usize::from(RING_RX_COUNT)
    );

    let mut consumer = RingConsumer::new(&mut region, layout, Ring::Rx).expect("consumer");
    let desc = consumer.pop().expect("descriptor");
    assert_eq!(desc.addr, offset as u64);
    assert_eq!(desc.len, 60);
    assert_eq!(
        desc.flags & (DESC_FIRST | DESC_LAST),
        DESC_FIRST | DESC_LAST
    );
    assert!(region[offset..offset + 60].iter().all(|byte| *byte == 0x5a));
    assert_eq!(
        service
            .backend()
            .controller()
            .seam
            .register(INTRL2_CPU_CLEAR),
        IRQ_RXDMA_DONE
    );
    let status = service
        .backend()
        .controller()
        .seam
        .register(raven_genet::regs::rx_desc_status(0).expect("status"));
    assert_eq!(status & RX_STATUS_OWN, RX_STATUS_OWN);
    assert_eq!(
        service.backend().controller().seam.register(
            raven_genet::regs::rx_dma_prod_index(raven_genet::regs::DEFAULT_QUEUE).expect("prod")
        ),
        9
    );
    assert_eq!(
        service.backend().controller().seam.register(
            raven_genet::regs::rx_dma_cons_index(raven_genet::regs::DEFAULT_QUEUE).expect("cons")
        ),
        1
    );
}

#[test]
fn a_malformed_rx_completion_is_counted_and_refilled() {
    let (mut service, mut region) = started();
    service.backend_mut().controller_mut().seam.set_register(
        raven_genet::regs::rx_desc_status(0).expect("status"),
        62u32 << RX_STATUS_BUFLEN_SHIFT,
    );
    service
        .backend_mut()
        .controller_mut()
        .seam
        .set_register(INTRL2_CPU_STAT, IRQ_RXDMA_DONE);

    let report = service.service(&mut region, 8).expect("service");
    assert_eq!(report.rx_delivered, 0);
    assert_eq!(report.rx_errors, 1);
    assert_eq!(report.rx_refilled, 1);
    let mut consumer =
        RingConsumer::new(&mut region, service.layout(), Ring::Rx).expect("consumer");
    assert!(consumer.pop().is_none());
}

#[test]
fn a_b39_tx_descriptor_is_queued_to_genet() {
    let (mut service, mut region) = started();
    let layout = service.layout();
    push_tx(&mut region, layout, RING_RX_BUFFERS, 64);

    assert_eq!(service.drain_tx(&mut region, 8).expect("drain"), 1);
    assert_eq!(service.backend().path().tx.posted().expect("posted"), 1);
    let status = service
        .backend()
        .controller()
        .seam
        .register(raven_genet::regs::tx_desc_status(0).expect("status"));
    assert_eq!(status & TX_STATUS_OWN, TX_STATUS_OWN);
    assert_eq!(
        status & TX_STATUS_BUFLEN_MASK,
        64u32 << TX_STATUS_BUFLEN_SHIFT
    );
    let low = service
        .backend()
        .controller()
        .seam
        .register(raven_genet::regs::tx_desc_address_lo(0).expect("address"));
    assert_eq!(
        u64::from(low),
        REGION_BUS + layout.buffer_offset(RING_RX_BUFFERS).expect("tx buffer") as u64
    );
    assert_eq!(
        RingHeader::read_from(&region).expect("header").tx_cons,
        0,
        "the B39 consumer advances only after GENET completes"
    );
    assert_eq!(
        service.backend().controller().seam.register(
            raven_genet::regs::tx_dma_prod_index(raven_genet::regs::DEFAULT_QUEUE).expect("prod")
        ),
        1
    );
    assert_eq!(
        service.backend().controller().seam.register(
            raven_genet::regs::tx_dma_cons_index(raven_genet::regs::DEFAULT_QUEUE).expect("cons")
        ),
        0
    );
}

#[test]
fn a_tx_completion_advances_the_b39_consumer() {
    let (mut service, mut region) = started();
    let layout = service.layout();
    push_tx(&mut region, layout, RING_RX_BUFFERS, 64);
    service.drain_tx(&mut region, 8).expect("drain");
    service.backend_mut().controller_mut().seam.set_register(
        raven_genet::regs::tx_desc_status(0).expect("status"),
        TxStatus::single(64).expect("status").raw(),
    );
    service
        .backend_mut()
        .controller_mut()
        .seam
        .set_register(INTRL2_CPU_STAT, IRQ_TXDMA_DONE);

    let report = service.service(&mut region, 8).expect("service");
    assert_eq!(report.tx_completed, 1);
    assert_eq!(service.backend().path().tx.posted().expect("posted"), 0);
    assert_eq!(RingHeader::read_from(&region).expect("header").tx_cons, 1);
    assert_eq!(
        service.backend().controller().seam.register(
            raven_genet::regs::tx_dma_cons_index(raven_genet::regs::DEFAULT_QUEUE).expect("cons")
        ),
        1
    );
}

#[test]
fn tx_is_bounded_by_the_three_pool_buffers() {
    let (mut service, mut region) = started();
    let layout = service.layout();
    for buffer in [RING_RX_BUFFERS, RING_RX_BUFFERS + 1, RING_RX_BUFFERS + 2] {
        push_tx(&mut region, layout, buffer, 64);
    }
    push_tx(&mut region, layout, RING_RX_BUFFERS, 64);

    assert_eq!(service.drain_tx(&mut region, 8).expect("first drain"), 3);
    assert_eq!(service.drain_tx(&mut region, 8).expect("second drain"), 0);
    assert_eq!(service.backend().path().tx.posted().expect("posted"), 3);
}

#[test]
fn a_malformed_tx_descriptor_is_rejected() {
    let (mut service, mut region) = started();
    let layout = service.layout();
    let offset = layout.buffer_offset(RING_RX_BUFFERS).expect("tx buffer");
    let desc = Desc::new(offset as u64, 64, 0);
    RingProducer::new(&mut region, layout, Ring::Tx)
        .expect("producer")
        .push(desc)
        .expect("push");
    assert_eq!(
        service.drain_tx(&mut region, 8),
        Err(NicError::BadDescriptor)
    );
}

#[test]
fn the_link_dhcp_ping_feed_formats_the_release_transcript() {
    let mut controller = ready_controller();
    controller.seam.set_mdio(MII_BMSR, BMSR_LINK | BMSR_ACOMP);
    controller
        .seam
        .set_mdio(BRGPHY_MII_AUXSTS, AUXSTS_RES_1000FD);
    let mut service = NicService::new(
        backend(controller),
        ring_layout().expect("layout"),
        REGION_BUS,
        DMA_LEN,
    )
    .expect("service");

    assert_eq!(service.poll_link().expect("first sample"), None);
    assert!(matches!(
        service.poll_link().expect("second sample"),
        Some(LinkEvent::Up { .. })
    ));
    assert!(service.info().link);
    service.dhcp_bound(lease()).expect("dhcp");
    service.ping_reply(ping()).expect("ping");
    assert_eq!(
        service.transcript().expect("transcript").to_string(),
        "GENET-NET-LINK speed=1000 duplex=full\n\
         GENET-NET-DHCP addr=192.168.1.23/24 gateway=192.168.1.1 dns=192.168.1.1\n\
         GENET-NET-PING target=192.168.1.1 tx=3 rx=3 rtt_ms=0\n\
         GENET-NET-OK\n"
    );
}

#[test]
fn a_down_link_does_not_advance_the_release_gate() {
    let mut service = service();
    assert_eq!(service.poll_link().expect("first sample"), None);
    assert!(matches!(
        service.poll_link().expect("second sample"),
        Some(LinkEvent::Down)
    ));
    assert!(!service.info().link);
    assert!(service.transcript().is_none());
}

#[test]
fn a_backend_with_the_wrong_tx_count_is_rejected() {
    let backend = FrameBackend::with_counts(ready_controller(), RING_RX_COUNT, RING_TX_COUNT, 2)
        .expect("backend");
    assert_eq!(
        NicService::new(backend, ring_layout().expect("layout"), REGION_BUS, DMA_LEN).err(),
        Some(NicError::BadLayout)
    );
}

#[test]
fn a_missing_mac_is_rejected() {
    let mut controller = ready_controller();
    controller.info.mac = None;
    let backend = backend(controller);
    assert_eq!(
        NicService::new(backend, ring_layout().expect("layout"), REGION_BUS, DMA_LEN).err(),
        Some(NicError::MissingMac)
    );
}

#[test]
fn the_pump_starts_stopped() {
    let mut service = service();
    assert!(!service.is_started());
    service.start();
    assert!(service.is_started());
    service.stop();
    assert!(!service.is_started());
}

#[test]
fn release_commands_parse_the_probe_lines() {
    assert_eq!(
        ReleaseCommand::parse("dhcp 192.168.1.23/24 192.168.1.1 192.168.1.1"),
        Some(ReleaseCommand::Dhcp { lease: lease() })
    );
    assert_eq!(
        ReleaseCommand::parse("dhcp 10.0.2.15/24 10.0.2.2 none"),
        Some(ReleaseCommand::Dhcp {
            lease: Lease::new([10, 0, 2, 15], 24, [10, 0, 2, 2], None).expect("lease")
        })
    );
    assert_eq!(
        ReleaseCommand::parse("ping 192.168.1.1 tx=3 rx=3 rtt_ms=0"),
        Some(ReleaseCommand::Ping { report: ping() })
    );
    assert!(ReleaseCommand::parse("dhcp 192.168.1.23/33 192.168.1.1 none").is_none());
    assert!(ReleaseCommand::parse("ping 192.168.1.1 tx=3 rx=3").is_none());
    assert!(ReleaseCommand::parse("ping 192.168.1.1 tx=3 rx=3 rtt_ms=0 extra").is_none());
    assert!(ReleaseCommand::parse("unknown").is_none());
}

#[test]
fn release_feed_completes_the_transcript() {
    let mut controller = ready_controller();
    controller.seam.set_mdio(MII_BMSR, BMSR_LINK | BMSR_ACOMP);
    controller
        .seam
        .set_mdio(BRGPHY_MII_AUXSTS, AUXSTS_RES_1000FD);
    let mut service = NicService::new(
        backend(controller),
        ring_layout().expect("layout"),
        REGION_BUS,
        DMA_LEN,
    )
    .expect("service");
    assert_eq!(service.poll_link().expect("first sample"), None);
    assert!(matches!(
        service.poll_link().expect("second sample"),
        Some(LinkEvent::Up { .. })
    ));
    assert!(service.transcript_text(&mut [0u8; 512]).is_none());
    service
        .feed_release(
            ReleaseCommand::parse("dhcp 192.168.1.23/24 192.168.1.1 192.168.1.1").expect("dhcp"),
        )
        .expect("feed dhcp");
    service
        .feed_release(ReleaseCommand::parse("ping 192.168.1.1 tx=3 rx=3 rtt_ms=0").expect("ping"))
        .expect("feed ping");
    let mut buffer = [0u8; 512];
    let length = service.transcript_text(&mut buffer).expect("transcript");
    assert_eq!(
        &buffer[..length],
        b"GENET-NET-LINK speed=1000 duplex=full\n\
          GENET-NET-DHCP addr=192.168.1.23/24 gateway=192.168.1.1 dns=192.168.1.1\n\
          GENET-NET-PING target=192.168.1.1 tx=3 rx=3 rtt_ms=0\n\
          GENET-NET-OK\n"
    );
}

#[test]
fn release_feed_rejects_a_ping_before_dhcp() {
    let mut service = service();
    assert_eq!(
        service.feed_release(ReleaseCommand::Ping { report: ping() }),
        Err(NicError::Driver(DriverError::Release(
            raven_genet::release::ReleaseError::WrongStage {
                expected: raven_genet::release::Stage::Ping,
                actual: raven_genet::release::Stage::Link,
            }
        )))
    );
}
