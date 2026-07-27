use std::collections::HashMap;
use std::net::IpAddr;

use camino::Utf8PathBuf;
use clap::Parser;
use color_eyre::Result;
use color_eyre::eyre::eyre;
use tracing::{debug, info, trace};

use crate::config::{ApplicationConfigLoader, ProviderConfig, ZoneConfig};
use crate::ip_cache::{IpCacheReader, IpCacheWriter};
use crate::provider::bunny::BunnyProvider;
use crate::provider::cloudflare::CloudflareProvider;
use crate::provider::{DnsProvider, DnsRecordType, Zone};
use crate::state::{ApplicationState, ApplicationStateBuilder, PublicIps};
use crate::{IPV4_URLS, IPV6_URLS, detect_ip};

#[allow(clippy::doc_markdown)]
#[derive(Debug, Parser)]
/// Dynamic DNS for Cloudflare and bunny.net
struct Args {
    #[arg(short, long)]
    /// Configuration file location. Defaults to
    /// ~/.config/ryndns/ryndns.toml or /etc/ryndns/ryndns.toml when running as root.
    config: Option<Utf8PathBuf>,

    /// IP address cache file location. Defaults to the same location as the
    /// configuration file, with a .cache extension.
    ip_cache: Option<Utf8PathBuf>,

    /// The desired IPv4 address. Overrides automatic detection.
    #[arg(long)]
    ipv4_address: Option<IpAddr>,

    /// The desired IPv6 address. Overrides automatic detection.
    #[arg(long)]
    ipv6_address: Option<IpAddr>,

    /// Shows what would happen, but doesn't change any settings.
    #[arg(short, long)]
    preview: bool,

    /// Update records even if the cached IP address hasn't changed.
    #[arg(short, long)]
    force: bool,
}

pub async fn main() -> Result<()> {
    crate::init()?;
    debug!("Logging start...");

    let args = Args::parse();
    trace!("Parsed args:\n{:#?}", args);

    let config_path =
        args.config.unwrap_or(ApplicationConfigLoader::default_config_file()?);

    let config = ApplicationConfigLoader::load_config_from(&config_path)?;
    trace!("Configuration:\n{:#?}", config);

    if args.preview {
        info!("Preview mode — no changes will be made.");
    }

    if config.cloudflare().is_none() && config.bunny().is_none() {
        return Err(eyre!(
            "No provider configured. Add a [cloudflare] or [bunny] section to your config."
        ));
    }

    let ip_cache_path =
        args.ip_cache.unwrap_or(config_path.with_extension("cache"));
    let ip_cache = IpCacheReader::load(&ip_cache_path)?;
    debug!("IP cache:\n{:#?}", ip_cache);

    let ipv4 = if let Some(ip) = args.ipv4_address {
        Some(ip)
    } else {
        detect_ip(IPV4_URLS).await.ok()
    };

    let ipv6 = if let Some(ip) = args.ipv6_address {
        Some(ip)
    } else {
        detect_ip(IPV6_URLS).await.ok()
    };

    let public_ips = match (ipv4, ipv6) {
        (Some(v4), Some(v6)) => PublicIps::Both { ipv4: v4, ipv6: v6 },
        (Some(v4), None) => PublicIps::V4(v4),
        (None, Some(v6)) => PublicIps::V6(v6),
        (None, None) => {
            return Err(eyre!(
                "Failed to detect both IPv4 and IPv6. \
                 No address available to update records."
            ));
        },
    };

    let mut state = ApplicationStateBuilder::default()
        .config_path(config_path)
        .ip_cache(ip_cache)
        .ip_cache_path(ip_cache_path)
        .public_ips(public_ips.clone())
        .preview(args.preview)
        .force(args.force)
        .build()?;

    if let Some(cf_config) = config.cloudflare() {
        let provider = CloudflareProvider::new(cf_config.token())?;
        run_provider(&provider, cf_config, &mut state).await?;
    }

    if let Some(bunny_config) = config.bunny() {
        let provider = BunnyProvider::new(bunny_config.token())?;
        run_provider(&provider, bunny_config, &mut state).await?;
    }

    if state.preview {
        info!("Done. (preview — no changes were made)");
    } else {
        IpCacheWriter.save(&state.ip_cache, &state.ip_cache_path)?;
        info!("Done.");
    }
    Ok(())
}

async fn run_provider<P: DnsProvider>(
    provider: &P,
    provider_config: &ProviderConfig,
    state: &mut ApplicationState,
) -> Result<()> {
    let zone_list = provider.list_zones().await?;
    let zone_map: HashMap<String, Zone> =
        zone_list.into_iter().map(|z| (z.name.clone(), z)).collect();

    debug!("Retrieved zones: {:#?}", zone_map.keys().collect::<Vec<_>>());

    for zone_config in provider_config.zones() {
        let zone = zone_map.get(&zone_config.name).cloned().unwrap_or_else(|| {
            tracing::warn!(
                "Zone '{}' not found in provider's zone list — treating as zone ID directly",
                zone_config.name
            );
            Zone { id: zone_config.name.clone(), name: zone_config.name.clone() }
        });
        handle_zone(provider, &zone, zone_config, state).await?;
    }

    Ok(())
}

async fn handle_zone<P: DnsProvider>(
    provider: &P,
    zone: &Zone,
    zone_config: &ZoneConfig,
    state: &mut ApplicationState,
) -> Result<()> {
    if zone_config.records().is_empty() {
        return Err(eyre!(
            "There are no records selected for update on zone '{}'.",
            zone.name
        ));
    }

    info!("Handling zone '{}'", zone.name);

    let public_ips = &state.public_ips;

    if !state.force && !state.ip_cache.has_changed(&zone.id, public_ips) {
        info!("IP address unchanged: '{public_ips}'");
        return Ok(());
    }

    if state.force {
        info!("IP address: '{public_ips}', forcing update");
    } else {
        info!("IP address updated: '{public_ips}'");
    }

    let records = provider.list_records(zone).await?;
    debug!("Retrieved records for '{}':\n{:#?}", zone.name, records);

    let records_to_update: Vec<_> = records
        .iter()
        .filter(|r| zone_config.is_record_selected(&r.name, r.record_type))
        .collect();

    for record in &records_to_update {
        if public_ips.get_for(record.record_type).is_none() {
            let family = match record.record_type {
                DnsRecordType::A => "IPv4",
                DnsRecordType::AAAA => "IPv6",
                other => {
                    return Err(eyre!(
                        "Record type '{other}' is not supported for IP updates"
                    ));
                },
            };
            return Err(eyre!(
                "Config targets {} record '{}' on zone '{}', \
                 but no {family} address was detected. \
                 Use --ipv{}-address to provide one or ensure {family} connectivity.",
                record.record_type,
                record.name,
                zone.name,
                if matches!(record.record_type, DnsRecordType::A) { "4" } else { "6" },
            ));
        }
    }

    debug!("Updating {} records:", records_to_update.len());
    for record in &records_to_update {
        debug!("{:>4}: {}", record.record_type, record.name);
    }

    for record in records_to_update {
        let ip = public_ips
            .get_for(record.record_type)
            .expect("validated above");

        if state.preview {
            info!("Would update {} to {ip}.", record.name);
        } else {
            info!("Updating {} to {ip}...", record.name);
            provider
                .update_record(zone, record, &ip.to_string())
                .await?;
        }
    }

    if !state.preview {
        state.ip_cache.set(&zone.id, public_ips.clone());
    }

    Ok(())
}
