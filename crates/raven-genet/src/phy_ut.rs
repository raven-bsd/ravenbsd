// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the BCM54213PE rules.

use super::*;

#[test]
fn the_bcm54213pe_identifier_decodes_its_oui_model_and_revision() {
    let id = PhyId::from_regs(0x600d, 0x84a2);
    assert_eq!(id.oui(), BCM54213PE_OUI);
    assert_eq!(id.model(), BCM54213PE_MODEL);
    assert_eq!(id.revision(), 2);
    assert!(id.is_bcm54213pe());
}

#[test]
fn a_different_broadcom_model_is_not_a_bcm54213pe() {
    let id = PhyId::from_regs(0x600d, 0x84b0);
    assert_eq!(id.oui(), BCM54213PE_OUI);
    assert_eq!(id.model(), 0x0b);
    assert!(!id.is_bcm54213pe());
}

#[test]
fn a_different_oui_is_not_a_bcm54213pe() {
    let id = PhyId::from_regs(0x1234, 0x84a0);
    assert_ne!(id.oui(), BCM54213PE_OUI);
    assert!(!id.is_bcm54213pe());
}

#[test]
fn a_down_link_has_no_resolved_speed_or_duplex() {
    let status = LinkStatus::resolve(0, AUXSTS_RES_1000FD).expect("down link");
    assert!(!status.up);
    assert_eq!(status.speed, None);
    assert_eq!(status.duplex, None);
    assert!(!status.autoneg_complete);
    assert_eq!(status.umac_speed_bits(), None);
}

#[test]
fn a_gigabit_full_duplex_link_resolves() {
    let status = LinkStatus::resolve(BMSR_LINK | BMSR_ACOMP, AUXSTS_RES_1000FD).expect("link");
    assert!(status.up);
    assert_eq!(status.speed, Some(Speed::Thousand));
    assert_eq!(status.duplex, Some(Duplex::Full));
    assert!(status.autoneg_complete);
    assert_eq!(status.umac_speed_bits(), Some(UMAC_CMD_SPEED_1000));
}

#[test]
fn every_resolution_value_maps_to_a_speed_and_duplex() {
    let cases = [
        (AUXSTS_RES_1000FD, Speed::Thousand, Duplex::Full),
        (AUXSTS_RES_1000HD, Speed::Thousand, Duplex::Half),
        (AUXSTS_RES_100FD, Speed::Hundred, Duplex::Full),
        (AUXSTS_RES_100T4, Speed::Hundred, Duplex::Full),
        (AUXSTS_RES_100HD, Speed::Hundred, Duplex::Half),
        (AUXSTS_RES_10FD, Speed::Ten, Duplex::Full),
        (AUXSTS_RES_10HD, Speed::Ten, Duplex::Half),
    ];
    for (resolution, speed, duplex) in cases {
        let status = LinkStatus::resolve(BMSR_LINK, resolution).expect("resolution");
        assert_eq!(status.speed, Some(speed));
        assert_eq!(status.duplex, Some(duplex));
    }
}

#[test]
fn an_unknown_resolution_is_rejected() {
    assert_eq!(
        LinkStatus::resolve(BMSR_LINK, 0),
        Err(LinkError::UnknownResolution(0))
    );
    assert_eq!(
        LinkStatus::resolve(BMSR_LINK, 0x0800),
        Err(LinkError::UnknownResolution(0))
    );
}

#[test]
fn the_umac_speed_encoding_round_trips() {
    for speed in [Speed::Ten, Speed::Hundred, Speed::Thousand] {
        assert_eq!(Speed::from_umac_bits(speed.umac_bits()), Some(speed));
    }
    assert_eq!(Speed::from_umac_bits(3), None);
}

#[test]
fn the_phy_init_sequence_is_ordered_and_ends_ready() {
    let mut init = PhyInit::new();
    assert_eq!(init.current(), Some(PhyInitStep::Reset));
    for expected in PHY_INIT_STEPS.iter().skip(1) {
        assert_eq!(init.advance(), Some(*expected));
    }
    assert!(init.is_ready());
    assert_eq!(init.advance(), Some(PhyInitStep::Ready));
}

#[test]
fn an_rgmii_rxid_phy_sets_the_rx_skew_and_leaves_the_tx_delay_off() {
    let mode = PhyMode::RgmiiRxid;
    assert!(mode.rx_delay());
    assert!(!mode.tx_delay());
    assert_eq!(
        rgmii_auxctl_write(0, mode),
        AUXCTL_MISC_WRITE_EN | AUXCTL_SHADOW_MISC | AUXCTL_MISC_RGMII_SKEW_EN
    );
    assert_eq!(
        rgmii_shadow_1c_write(0, mode),
        SHADOW_1C_WRITE_EN | SHADOW_1C_CLK_CTRL
    );
}

#[test]
fn an_rgmii_txid_phy_sets_the_gtx_delay_and_leaves_the_rx_skew_off() {
    let mode = PhyMode::RgmiiTxid;
    assert!(!mode.rx_delay());
    assert!(mode.tx_delay());
    assert_eq!(
        rgmii_auxctl_write(0, mode),
        AUXCTL_MISC_WRITE_EN | AUXCTL_SHADOW_MISC
    );
    assert_eq!(
        rgmii_shadow_1c_write(0, mode),
        SHADOW_1C_WRITE_EN | SHADOW_1C_CLK_CTRL | SHADOW_1C_GTXCLK_EN
    );
}

#[test]
fn an_rgmii_id_phy_sets_both_delays_and_plain_rgmii_sets_neither() {
    for (mode, rx, tx) in [
        (PhyMode::RgmiiId, true, true),
        (PhyMode::Rgmii, false, false),
    ] {
        let auxctl = rgmii_auxctl_write(0, mode);
        let shadow = rgmii_shadow_1c_write(0, mode);
        assert_eq!(auxctl & AUXCTL_MISC_RGMII_SKEW_EN != 0, rx);
        assert_eq!(shadow & SHADOW_1C_GTXCLK_EN != 0, tx);
    }
}

#[test]
fn the_delay_writes_preserve_the_other_shadow_data_bits() {
    let current = AUXCTL_MISC_DATA_MASK & !AUXCTL_MISC_RGMII_SKEW_EN;
    let value = rgmii_auxctl_write(current, PhyMode::Rgmii);
    assert_eq!(value & AUXCTL_MISC_DATA_MASK, current);

    let current = SHADOW_1C_DATA_MASK & !SHADOW_1C_GTXCLK_EN;
    let value = rgmii_shadow_1c_write(current, PhyMode::RgmiiTxid);
    assert_eq!(value & SHADOW_1C_DATA_MASK, current | SHADOW_1C_GTXCLK_EN);
}

#[test]
fn the_external_rgmii_control_clears_oob_and_sets_the_mode() {
    let value = ext_rgmii_oob_ctrl(EXT_RGMII_OOB_OOB_DISABLE, PhyMode::RgmiiRxid, true);
    assert_eq!(value & EXT_RGMII_OOB_OOB_DISABLE, 0);
    assert_eq!(
        value & EXT_RGMII_OOB_RGMII_MODE_EN,
        EXT_RGMII_OOB_RGMII_MODE_EN
    );
    assert_eq!(value & EXT_RGMII_OOB_RGMII_LINK, EXT_RGMII_OOB_RGMII_LINK);
    assert_eq!(value & EXT_RGMII_OOB_ID_MODE_DISABLE, 0);
}

#[test]
fn plain_rgmii_sets_the_id_mode_disable_bit() {
    let value = ext_rgmii_oob_ctrl(0, PhyMode::Rgmii, true);
    assert_eq!(
        value & EXT_RGMII_OOB_ID_MODE_DISABLE,
        EXT_RGMII_OOB_ID_MODE_DISABLE
    );
    assert_eq!(value & EXT_RGMII_OOB_RGMII_LINK, EXT_RGMII_OOB_RGMII_LINK);
}

#[test]
fn a_down_link_clears_only_the_rgmii_link_bit() {
    let value = ext_rgmii_oob_ctrl(EXT_RGMII_OOB_RGMII_LINK, PhyMode::RgmiiRxid, false);
    assert_eq!(value & EXT_RGMII_OOB_RGMII_LINK, 0);
    assert_eq!(
        value & EXT_RGMII_OOB_RGMII_MODE_EN,
        EXT_RGMII_OOB_RGMII_MODE_EN
    );
}

#[test]
fn phy_mode_strings_round_trip_and_reject_unknown_names() {
    for mode in [
        PhyMode::Rgmii,
        PhyMode::RgmiiId,
        PhyMode::RgmiiRxid,
        PhyMode::RgmiiTxid,
    ] {
        assert_eq!(PhyMode::from_dt(mode.as_str()), Some(mode));
    }
    assert_eq!(PhyMode::from_dt("sgmii"), None);
    assert_eq!(PhyMode::from_dt(""), None);
}

#[test]
fn the_mii_register_numbers_match_the_ieee_and_broadcom_layout() {
    assert_eq!(MII_BMCR, 0);
    assert_eq!(MII_BMSR, 1);
    assert_eq!(MII_PHYIDR1, 2);
    assert_eq!(MII_PHYIDR2, 3);
    assert_eq!(BRGPHY_MII_AUXCTL, 0x18);
    assert_eq!(BRGPHY_MII_AUXSTS, 0x19);
    assert_eq!(BRGPHY_MII_SHADOW_1C, 0x1c);
    assert_eq!(BMCR_ANENABLE, 0x1000);
    assert_eq!(BMCR_ANRESTART, 0x0200);
    assert_eq!(BMSR_LINK, 0x0004);
    assert_eq!(BMSR_ACOMP, 0x0020);
}
