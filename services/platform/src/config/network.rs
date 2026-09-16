use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IpCidr {
    pub(super) network: IpAddr,
    pub(super) prefix_len: u8,
}

impl IpCidr {
    pub fn contains(self, address: IpAddr) -> bool {
        match (self.network, address) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                let prefix = u32::from(self.prefix_len);
                let mask = if prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix)
                };
                u32::from(network) & mask == u32::from(address) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                let prefix = u32::from(self.prefix_len);
                let mask = if prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix)
                };
                u128::from(network) & mask == u128::from(address) & mask
            }
            _ => false,
        }
    }
}

impl FromStr for IpCidr {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (address, prefix_len) = match value.split_once('/') {
            Some((address, prefix)) => {
                let address = address.parse::<IpAddr>().map_err(|_| ())?;
                let prefix_len = prefix.parse::<u8>().map_err(|_| ())?;
                (address, prefix_len)
            }
            None => {
                let address = value.parse::<IpAddr>().map_err(|_| ())?;
                let prefix_len = if address.is_ipv4() { 32 } else { 128 };
                (address, prefix_len)
            }
        };
        let valid = match address {
            IpAddr::V4(_) => prefix_len <= 32,
            IpAddr::V6(_) => prefix_len <= 128,
        };
        valid
            .then_some(Self {
                network: address,
                prefix_len,
            })
            .ok_or(())
    }
}
