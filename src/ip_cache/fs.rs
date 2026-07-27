use std::fmt::Write;
use std::net::IpAddr;
use std::str::FromStr;

use camino::Utf8Path;
use color_eyre::Result;
use color_eyre::eyre::eyre;
use indexmap::IndexMap;

use crate::state::PublicIps;

static DELIMITER: &str = ";";

use super::IpCache;

pub struct IpCacheReader;

impl IpCacheReader {
    pub fn load(path: &Utf8Path) -> Result<IpCache> {
        if path.is_file() {
            let body = fs_err::read_to_string(path)?;

            let mut cache = IndexMap::new();

            for line in body.lines() {
                let parts: Vec<&str> = line.split(DELIMITER).collect();
                if parts.len() < 2 {
                    return Err(eyre!(
                        "Cache line should contain at least two values, separated by {DELIMITER}"
                    ));
                }
                let key = parts[0].to_owned();

                let v4 = if parts[1].is_empty() {
                    None
                } else {
                    Some(IpAddr::from_str(parts[1]).map_err(|e| {
                        eyre!("Invalid IPv4 in cache line: {e}")
                    })?)
                };

                let v6 = if parts.len() >= 3 && !parts[2].is_empty() {
                    Some(IpAddr::from_str(parts[2]).map_err(|e| {
                        eyre!("Invalid IPv6 in cache line: {e}")
                    })?)
                } else {
                    None
                };

                let ips = match (v4, v6) {
                    (Some(v4), Some(v6)) => PublicIps::Both { ipv4: v4, ipv6: v6 },
                    (Some(v4), None) => PublicIps::V4(v4),
                    (None, Some(v6)) => PublicIps::V6(v6),
                    (None, None) => {
                        return Err(eyre!(
                            "Cache line has no IP address (zone: {key})"
                        ));
                    },
                };

                cache.insert(key, ips);
            }

            Ok(IpCache::new(cache))
        } else if path.exists() {
            Err(eyre!("Cache file path exists, but is not a file!"))
        } else {
            Ok(IpCache::default())
        }
    }
}

pub struct IpCacheWriter;

impl IpCacheWriter {
    pub fn save(&self, ip_cache: &IpCache, path: &Utf8Path) -> Result<()> {
        let body = ip_cache.into_iter().fold(
            String::new(),
            |mut acc, (key, ips)| {
                let (v4_str, v6_str) = match ips {
                    PublicIps::V4(ip) => (ip.to_string(), String::new()),
                    PublicIps::V6(ip) => (String::new(), ip.to_string()),
                    PublicIps::Both { ipv4, ipv6 } => {
                        (ipv4.to_string(), ipv6.to_string())
                    },
                };
                writeln!(acc, "{key}{DELIMITER}{v4_str}{DELIMITER}{v6_str}")
                    .unwrap();
                acc
            },
        );

        fs_err::write(path, body)?;

        Ok(())
    }
}
