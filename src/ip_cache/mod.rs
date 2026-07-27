mod fs;

pub use fs::{IpCacheReader, IpCacheWriter};
use indexmap::IndexMap;

use crate::state::PublicIps;

/// Caches the latest IPs (v4 and v6) for a given zone ID.
#[derive(Debug, Default, Clone)]
pub struct IpCache {
    cache: IndexMap<String, PublicIps>,
}

impl IpCache {
    #[must_use]
    pub fn new(cache: IndexMap<String, PublicIps>) -> Self {
        Self { cache }
    }

    /// True if the new IPs differ from cached state, or no cache entry exists.
    #[must_use]
    pub fn has_changed(&self, zone_id: &str, ips: &PublicIps) -> bool {
        self.cache.get(zone_id).is_none_or(|cached| cached != ips)
    }

    /// Store the IPs for this zone.
    pub fn set(&mut self, zone_id: &str, ips: PublicIps) {
        self.cache.insert(zone_id.to_owned(), ips);
    }

    #[must_use]
    pub fn iter(&self) -> indexmap::map::Iter<'_, String, PublicIps> {
        self.cache.iter()
    }
}

impl<'a> IntoIterator for &'a IpCache {
    type IntoIter = indexmap::map::Iter<'a, String, PublicIps>;
    type Item = (&'a String, &'a PublicIps);

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use std::str::FromStr;

    fn v4(s: &str) -> PublicIps {
        PublicIps::V4(IpAddr::from_str(s).unwrap())
    }

    fn v6(s: &str) -> PublicIps {
        PublicIps::V6(IpAddr::from_str(s).unwrap())
    }

    fn both(v4s: &str, v6s: &str) -> PublicIps {
        PublicIps::Both {
            ipv4: IpAddr::from_str(v4s).unwrap(),
            ipv6: IpAddr::from_str(v6s).unwrap(),
        }
    }

    #[test]
    fn has_changed_new_zone() {
        let cache = IpCache::default();
        assert!(cache.has_changed("zone1", &v4("1.2.3.4")));
    }

    #[test]
    fn has_changed_unchanged() {
        let mut cache = IpCache::default();
        let ips = v4("1.2.3.4");
        cache.set("zone1", ips.clone());
        assert!(!cache.has_changed("zone1", &ips));
    }

    #[test]
    fn has_changed_different_ip() {
        let mut cache = IpCache::default();
        cache.set("zone1", v4("1.2.3.4"));
        assert!(cache.has_changed("zone1", &v4("5.6.7.8")));
    }

    #[test]
    fn has_changed_variant_changed() {
        let mut cache = IpCache::default();
        cache.set("zone1", v4("1.2.3.4"));
        assert!(cache.has_changed("zone1", &both("1.2.3.4", "::1")));
    }
}
