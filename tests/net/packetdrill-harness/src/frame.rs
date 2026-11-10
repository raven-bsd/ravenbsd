// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Ethernet/IPv4/TCP frame construction and parsing for the harness.

pub const SUT_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 1];
pub const PEER_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 2];
pub const SUT_IP: [u8; 4] = [10, 0, 0, 1];
pub const PEER_IP: [u8; 4] = [10, 0, 0, 2];
pub const SUT_PORT: u16 = 8080;
pub const PEER_PORT: u16 = 12345;
pub const PEER_ISN: u32 = 1000;

pub const FIN: u8 = 0x01;
pub const SYN: u8 = 0x02;
pub const RST: u8 = 0x04;
pub const PSH: u8 = 0x08;
pub const ACK: u8 = 0x10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TcpFrame {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub flags: u8,
    pub window: u16,
    pub payload_len: usize,
}

impl TcpFrame {
    pub fn flags_text(self) -> String {
        let mut text = String::new();
        if self.flags & SYN != 0 {
            text.push('S');
        }
        if self.flags & ACK != 0 {
            text.push('.');
        }
        if self.flags & PSH != 0 {
            text.push('P');
        }
        if self.flags & FIN != 0 {
            text.push('F');
        }
        if self.flags & RST != 0 {
            text.push('R');
        }
        if text.is_empty() {
            text.push('-');
        }
        text
    }
}

#[allow(clippy::too_many_arguments)] // Mirrors the Ethernet/IPv4/TCP field list.
pub fn build_tcp(
    src_mac: [u8; 6],
    dst_mac: [u8; 6],
    src_ip: [u8; 4],
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    payload: &[u8],
) -> Vec<u8> {
    let tcp_len = 20 + payload.len();
    let ip_len = 20 + tcp_len;
    let mut frame = vec![0u8; 14 + ip_len];
    frame[0..6].copy_from_slice(&dst_mac);
    frame[6..12].copy_from_slice(&src_mac);
    frame[12] = 0x08;
    frame[13] = 0x00;

    let ip = &mut frame[14..];
    ip[0] = 0x45;
    ip[1] = 0;
    ip[2..4].copy_from_slice(&(ip_len as u16).to_be_bytes());
    ip[4..6].copy_from_slice(&1u16.to_be_bytes());
    ip[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
    ip[8] = 64;
    ip[9] = 6;
    ip[12..16].copy_from_slice(&src_ip);
    ip[16..20].copy_from_slice(&dst_ip);
    let ip_checksum = checksum(&ip[..20]);
    ip[10..12].copy_from_slice(&ip_checksum.to_be_bytes());

    let tcp = &mut ip[20..];
    tcp[0..2].copy_from_slice(&src_port.to_be_bytes());
    tcp[2..4].copy_from_slice(&dst_port.to_be_bytes());
    tcp[4..8].copy_from_slice(&seq.to_be_bytes());
    tcp[8..12].copy_from_slice(&ack.to_be_bytes());
    tcp[12] = 5 << 4;
    tcp[13] = flags;
    tcp[14..16].copy_from_slice(&window.to_be_bytes());
    tcp[20..].copy_from_slice(payload);

    let mut pseudo = Vec::with_capacity(12 + tcp.len());
    pseudo.extend_from_slice(&src_ip);
    pseudo.extend_from_slice(&dst_ip);
    pseudo.push(0);
    pseudo.push(6);
    pseudo.extend_from_slice(&(tcp.len() as u16).to_be_bytes());
    pseudo.extend_from_slice(tcp);
    let tcp_checksum = checksum(&pseudo);
    tcp[16..18].copy_from_slice(&tcp_checksum.to_be_bytes());

    while frame.len() < 60 {
        frame.push(0);
    }
    frame
}

pub fn parse_tcp(frame: &[u8]) -> Option<TcpFrame> {
    if frame.len() < 34 || frame[12..14] != [0x08, 0x00] {
        return None;
    }
    let ip = &frame[14..];
    if ip.len() < 20 || ip[0] >> 4 != 4 || ip[9] != 6 {
        return None;
    }
    let ihl = usize::from(ip[0] & 0x0f) * 4;
    if ihl < 20 || ip.len() < ihl + 20 {
        return None;
    }
    let total_len = usize::from(u16::from_be_bytes([ip[2], ip[3]]));
    if total_len < ihl + 20 || ip.len() < total_len {
        return None;
    }
    let tcp = &ip[ihl..total_len];
    let data_offset = usize::from(tcp[12] >> 4) * 4;
    if data_offset < 20 || tcp.len() < data_offset {
        return None;
    }
    Some(TcpFrame {
        src_port: u16::from_be_bytes([tcp[0], tcp[1]]),
        dst_port: u16::from_be_bytes([tcp[2], tcp[3]]),
        seq: u32::from_be_bytes([tcp[4], tcp[5], tcp[6], tcp[7]]),
        ack: u32::from_be_bytes([tcp[8], tcp[9], tcp[10], tcp[11]]),
        flags: tcp[13],
        window: u16::from_be_bytes([tcp[14], tcp[15]]),
        payload_len: tcp.len() - data_offset,
    })
}

pub fn arp_request_target(frame: &[u8]) -> Option<[u8; 4]> {
    if frame.len() < 42 || frame[12..14] != [0x08, 0x06] {
        return None;
    }
    let arp = &frame[14..];
    if u16::from_be_bytes([arp[0], arp[1]]) != 1
        || u16::from_be_bytes([arp[2], arp[3]]) != 0x0800
        || arp[4] != 6
        || arp[5] != 4
        || u16::from_be_bytes([arp[6], arp[7]]) != 1
    {
        return None;
    }
    Some([arp[24], arp[25], arp[26], arp[27]])
}

pub fn build_arp_reply(
    dst_mac: [u8; 6],
    src_mac: [u8; 6],
    sender_ip: [u8; 4],
    target_ip: [u8; 4],
    target_mac: [u8; 6],
) -> Vec<u8> {
    let mut frame = vec![0u8; 60];
    frame[0..6].copy_from_slice(&dst_mac);
    frame[6..12].copy_from_slice(&src_mac);
    frame[12..14].copy_from_slice(&[0x08, 0x06]);
    let arp = &mut frame[14..42];
    arp[0..2].copy_from_slice(&1u16.to_be_bytes());
    arp[2..4].copy_from_slice(&0x0800u16.to_be_bytes());
    arp[4] = 6;
    arp[5] = 4;
    arp[6..8].copy_from_slice(&2u16.to_be_bytes());
    arp[8..14].copy_from_slice(&src_mac);
    arp[14..18].copy_from_slice(&sender_ip);
    arp[18..24].copy_from_slice(&target_mac);
    arp[24..28].copy_from_slice(&target_ip);
    frame
}

pub fn payload(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}

fn checksum(bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut chunks = bytes.chunks_exact(2);
    for chunk in &mut chunks {
        sum += u32::from(u16::from_be_bytes([chunk[0], chunk[1]]));
    }
    if let Some(byte) = chunks.remainder().first() {
        sum += u32::from(u16::from_be_bytes([*byte, 0]));
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}
