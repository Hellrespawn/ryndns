use std::fmt;
use std::net::IpAddr;

use camino::Utf8PathBuf;
use derive_builder::Builder;

use crate::ip_cache::IpCache;
use crate::provider::DnsRecordType;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PublicIps {
    V4(IpAddr),
    V6(IpAddr),
    Both { ipv4: IpAddr, ipv6: IpAddr },
}

impl PublicIps {
    #[must_use]
    pub fn get_for(&self, record_type: DnsRecordType) -> Option<IpAddr> {
        match self {
            PublicIps::V4(ip) if record_type == DnsRecordType::A => Some(*ip),
            PublicIps::V6(ip) if record_type == DnsRecordType::AAAA => Some(*ip),
            PublicIps::Both { ipv4, ipv6 } => match record_type {
                DnsRecordType::A => Some(*ipv4),
                DnsRecordType::AAAA => Some(*ipv6),
                _ => None,
            },
            _ => None,
        }
    }
}

impl fmt::Display for PublicIps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublicIps::V4(ip) | PublicIps::V6(ip) => write!(f, "{ip}"),
            PublicIps::Both { ipv4, ipv6 } => write!(f, "{ipv4} / {ipv6}"),
        }
    }
}

#[derive(Debug, Builder)]
pub struct ApplicationState {
    pub config_path: Utf8PathBuf,
    pub ip_cache: IpCache,
    pub ip_cache_path: Utf8PathBuf,
    pub public_ips: PublicIps,
    pub preview: bool,
    pub force: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn get_for_v4_to_a() {
        let ips = PublicIps::V4(IpAddr::from_str("1.2.3.4").unwrap());
        assert_eq!(ips.get_for(DnsRecordType::A), Some(IpAddr::from_str("1.2.3.4").unwrap()));
        assert_eq!(ips.get_for(DnsRecordType::AAAA), None);
    }

    #[test]
    fn get_for_v6_to_aaaa() {
        let ips = PublicIps::V6(IpAddr::from_str("::1").unwrap());
        assert_eq!(ips.get_for(DnsRecordType::AAAA), Some(IpAddr::from_str("::1").unwrap()));
        assert_eq!(ips.get_for(DnsRecordType::A), None);
    }

    #[test]
    fn get_for_both() {
        let ips = PublicIps::Both {
            ipv4: IpAddr::from_str("1.2.3.4").unwrap(),
            ipv6: IpAddr::from_str("::1").unwrap(),
        };
        assert_eq!(ips.get_for(DnsRecordType::A), Some(IpAddr::from_str("1.2.3.4").unwrap()));
        assert_eq!(ips.get_for(DnsRecordType::AAAA), Some(IpAddr::from_str("::1").unwrap()));
    }

    #[test]
    fn get_for_other_types_returns_none() {
        let ips = PublicIps::Both {
            ipv4: IpAddr::from_str("1.2.3.4").unwrap(),
            ipv6: IpAddr::from_str("::1").unwrap(),
        };
        assert_eq!(ips.get_for(DnsRecordType::MX), None);
        assert_eq!(ips.get_for(DnsRecordType::TXT), None);
    }
}
