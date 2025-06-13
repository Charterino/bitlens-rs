use std::{
    fmt::Display,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    str::FromStr,
};


use super::network_id::NetworkId;

#[derive(Default, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct AddressPortNetwork {
    pub network_id: NetworkId,
    pub port: u16,
    pub address: Vec<u8>,
}

impl Display for AddressPortNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.network_id {
            NetworkId::IPv4 => match self.address.len() {
                4 => {
                    let bytes: &[u8; 4] = self.address.get(0..4).unwrap().try_into().unwrap();
                    write!(f, "{}:{}", Ipv4Addr::from(*bytes), self.port)
                }
                other => write!(f, "invalid ipv4 length: {}", other),
            },
            NetworkId::IPv6 => match self.address.len() {
                16 => {
                    let bytes: &[u8; 16] = self.address.get(0..16).unwrap().try_into().unwrap();
                    write!(f, "{}:{}", Ipv6Addr::from(*bytes), self.port)
                }
                other => write!(f, "invalid ipv6 length: {}", other),
            },
            other => write!(f, "unsupported network: {}", other),
        }
    }
}

impl FromStr for AddressPortNetwork {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let addy = SocketAddr::from_str(s)?;
        match addy.ip() {
            IpAddr::V4(ipv4_addr) => Ok(AddressPortNetwork {
                network_id: NetworkId::IPv4,
                port: addy.port(),
                address: ipv4_addr.octets().to_vec(),
            }),
            IpAddr::V6(ipv6_addr) => Ok(AddressPortNetwork {
                network_id: NetworkId::IPv6,
                port: addy.port(),
                address: ipv6_addr.octets().to_vec(),
            }),
        }
    }
}
