use serde::Deserialize;

use crate::provider::DnsRecordType;

mod fs;

pub use fs::ApplicationConfigLoader;

#[derive(Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct ApplicationConfig {
    public_ip_url: String,
    cloudflare: Option<ProviderConfig>,
    bunny: Option<ProviderConfig>,
}

impl ApplicationConfig {
    #[must_use]
    pub fn public_ip_url(&self) -> &str {
        &self.public_ip_url
    }

    #[must_use]
    pub fn cloudflare(&self) -> Option<&ProviderConfig> {
        self.cloudflare.as_ref()
    }

    #[must_use]
    pub fn bunny(&self) -> Option<&ProviderConfig> {
        self.bunny.as_ref()
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct ProviderConfig {
    token: String,
    #[serde(default)]
    zones: Vec<ZoneConfig>,
}

impl ProviderConfig {
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    #[must_use]
    pub fn zones(&self) -> &[ZoneConfig] {
        &self.zones
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct ZoneConfig {
    pub name: String,
    records: Vec<RecordConfig>,
}

impl ZoneConfig {
    #[must_use]
    pub fn records(&self) -> &[RecordConfig] {
        &self.records
    }

    #[must_use]
    pub fn is_record_selected(
        &self,
        record_name: &str,
        record_type: DnsRecordType,
    ) -> bool {
        self.records.iter().any(|r| r.matches(record_name, record_type))
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(untagged)]
pub enum RecordConfig {
    Full {
        #[serde(rename = "type", default)]
        record_type: DnsRecordType,
        name: String,
    },
    Name(String),
}

impl RecordConfig {
    fn matches(&self, record_name: &str, record_type: DnsRecordType) -> bool {
        let self_name = self.name();
        let name_matches = self_name == record_name
            || record_name.starts_with(&format!("{self_name}."));
        let type_matches = self.record_type() == record_type;
        type_matches && name_matches
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            RecordConfig::Full { name, .. } | RecordConfig::Name(name) => name,
        }
    }

    #[must_use]
    pub fn record_type(&self) -> DnsRecordType {
        match self {
            RecordConfig::Full { record_type, .. } => *record_type,
            RecordConfig::Name(_) => DnsRecordType::A,
        }
    }
}

#[cfg(test)]
mod test {
    use color_eyre::Result;

    use super::*;

    const EXAMPLE: &str = include_str!("../../test/example.toml");

    fn get_expected_config() -> ApplicationConfig {
        ApplicationConfig {
            public_ip_url: "https://example.ip".to_owned(),
            cloudflare: Some(ProviderConfig {
                token: "cf_token".to_owned(),
                zones: vec![ZoneConfig {
                    name: "example.nl".to_owned(),
                    records: vec![
                        RecordConfig::Full {
                            record_type: DnsRecordType::A,
                            name: "www".to_owned(),
                        },
                        RecordConfig::Name("mail".to_owned()),
                    ],
                }],
            }),
            bunny: Some(ProviderConfig {
                token: "bunny_token".to_owned(),
                zones: vec![ZoneConfig {
                    name: "otherexample.com".to_owned(),
                    records: vec![
                        RecordConfig::Full {
                            record_type: DnsRecordType::A,
                            name: "www".to_owned(),
                        },
                        RecordConfig::Name("mail".to_owned()),
                    ],
                }],
            }),
        }
    }

    #[test]
    fn test_deserialize() -> Result<()> {
        let config: ApplicationConfig = toml::from_str(EXAMPLE)?;
        assert_eq!(config, get_expected_config());
        Ok(())
    }
}
