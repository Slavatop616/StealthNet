use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn packet_destination(packet: &[u8]) -> Option<IpAddr> {
    let first = *packet.first()?;
    let version = first >> 4;
    match version {
        4 => {
            if packet.len() < 20 {
                return None;
            }
            Some(IpAddr::V4(Ipv4Addr::new(
                packet[16], packet[17], packet[18], packet[19],
            )))
        }
        6 => {
            if packet.len() < 40 {
                return None;
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&packet[24..40]);
            Some(IpAddr::V6(Ipv6Addr::from(octets)))
        }
        _ => None,
    }
}
