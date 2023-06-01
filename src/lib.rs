use mac_address::MacAddress;
use std::net::Ipv6Addr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Size {
    SinglePixel = 1,
    Area2x2 = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub x: u16,
    pub y: u16,
}

impl Pos {
    pub fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Color {
    pub fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EthernetInfo {
    pub src_mac: MacAddress,
    pub dest_mac: MacAddress,
}

impl EthernetInfo {
    pub fn new(src_mac: MacAddress, dest_mac: MacAddress) -> Self {
        Self { src_mac, dest_mac }
    }
}

pub fn to_addr(pos: Pos, color: Color, size: Size) -> Ipv6Addr {
    Ipv6Addr::new(
        0x2602,
        0xfa9b,
        0x202,
        pos.x | ((size as u16) << 12),
        pos.y,
        color.red as u16,
        color.green as u16,
        color.blue as u16,
    )
}

// https://datatracker.ietf.org/doc/html/rfc1071
pub fn icmpv6_checksum(src_ip: Ipv6Addr, dest_ip: Ipv6Addr, icmpv6_packet: &[u8]) -> u16 {
    let mut data = make_ipv6_pseudo_header(src_ip, dest_ip, icmpv6_packet.len() as u16);
    icmpv6_packet.iter().for_each(|byte| data.push(*byte));

    let mut total: u32 = 0;
    let mut i = 0;
    let mut words = (data.len() + 1) / 2;

    // Iterate over 16-bit words
    loop {
        if words <= 0 {
            break;
        }
        words -= 1;

        let val = ((if i + 1 < data.len() {
            data[i + 1] as u32
        } else {
            0x00
        }) << 8)
            | (data[i] as u32);
        total += val;
        i += 2;
    }

    while (total & 0xffff0000) > 0 {
        total = (total >> 16) + (total & 0xffff);
    }

    return !(total as u16);
}

pub fn make_ipv6_pseudo_header(
    src_ip: Ipv6Addr,
    dest_ip: Ipv6Addr,
    icmp_packet_len: u16,
) -> Vec<u8> {
    let mut data = Vec::new();
    src_ip.octets().into_iter().for_each(|byte| data.push(byte)); // Source Address
    dest_ip
        .octets()
        .into_iter()
        .for_each(|byte| data.push(byte)); // Destination Address

    data.push((icmp_packet_len >> 8) as u8);
    data.push((icmp_packet_len & 0xFF) as u8);

    data.push(0x00);
    data.push(0x00);
    data.push(0x00);
    data.push(0x3a); // Next header: ICMPv6 (58)
    data
}

pub fn make_icmpv6_packet(
    ethernet_info: Option<EthernetInfo>,
    src_ip: Ipv6Addr,
    dest_ip: Ipv6Addr,
) -> Vec<u8> {
    let mut data = Vec::new();

    // Ethernet header
    if let Some(ethernet_info) = ethernet_info {
        ethernet_info
            .dest_mac
            .bytes()
            .into_iter()
            .for_each(|byte| data.push(byte));
        ethernet_info
            .src_mac
            .bytes()
            .into_iter()
            .for_each(|byte| data.push(byte));
        let nextheader_type: u16 = 0x86dd; // IPv6
        data.push((nextheader_type >> 8) as u8);
        data.push((nextheader_type & 0xFF) as u8);
    }

    // IPv6 Header
    data.push(0x60); // Version 6
    data.push(0x08); // Something... traffic class... something
    data.push(0x0a); // ↑
    data.push(0xf4); // ↑

    let payload_length: u16 = 8;
    data.push((payload_length >> 8) as u8);
    data.push((payload_length & 0xFF) as u8);

    data.push(0x3a); // Next header: ICMPv6 (58)
    data.push(64); // Hop limit

    src_ip.octets().into_iter().for_each(|byte| data.push(byte)); // Source Address
    dest_ip
        .octets()
        .into_iter()
        .for_each(|byte| data.push(byte)); // Destination Address

    // ICMP Payload
    let icmpv6_header_start_index = data.len();
    data.push(0x80); // Type
    data.push(0x00); // Code

    // Checksum. Calculated later, left zeroed for now
    let icmpv6_checksum_index = data.len();
    data.push(0x00);
    data.push(0x00);

    let identifier: u16 = 0x0069;
    data.push((identifier >> 8) as u8);
    data.push((identifier & 0xFF) as u8);

    let sequence: u16 = 0x0001;
    data.push((sequence >> 8) as u8);
    data.push((sequence & 0xFF) as u8);

    // Ping Data...
    // <Empty> for now

    // Calculate ICMPv6 Checksum...
    let checksum = icmpv6_checksum(src_ip, dest_ip, &data[icmpv6_header_start_index..]);
    data[icmpv6_checksum_index] = (checksum & 0xFF) as u8;
    data[icmpv6_checksum_index + 1] = (checksum >> 8) as u8;

    data
}
