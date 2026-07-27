// #![warn(missing_docs)]
#![warn(clippy::pedantic)]
//#![warn(clippy::cargo)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::module_name_repetitions)]
#![allow(unknown_lints)] // For nightly lints

pub(crate) mod bunny_api;
pub mod cli;
pub(crate) mod cloudflare_api;
pub mod config;
pub mod ip_cache;
pub mod provider;
pub mod state;

use std::net::IpAddr;
use std::str::FromStr;

use color_eyre::Result;
use tracing::Level;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::FilterFn;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");
const CRATE_NAME: &str = env!("CARGO_CRATE_NAME");

const LOG_KEY: &str = "CFDD_LOG";

/// IPv4 detection URLs, tried in order until one succeeds.
pub const IPV4_URLS: &[&str] = &[
    "https://api.ipify.org",
    "https://ifconfig.me/ip",
    "https://icanhazip.com",
];

/// IPv6 detection URLs, tried in order until one succeeds.
pub const IPV6_URLS: &[&str] = &[
    "https://api6.ipify.org",
    "https://ifconfig.co/ip",
];

/// Install `color_eyre` and enable tracing. Defaults to `Level::INFO`.
pub fn init() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let var = std::env::var_os(LOG_KEY).map(|os| {
        os.into_string().expect("Environment variable is not UTF-8!")
    });

    let level_filter =
        var.map_or(Ok(LevelFilter::INFO), |l| LevelFilter::from_str(&l))?;

    // Output logging
    let user_layer = fmt::layer()
        .compact()
        .without_time()
        .with_target(false)
        .with_level(false)
        .with_filter(LevelFilter::INFO)
        .with_filter(FilterFn::new(|m| m.target().starts_with(CRATE_NAME)));

    let debug_layer = fmt::layer()
        .compact()
        .with_filter(level_filter)
        .with_filter(FilterFn::new(|m| m.target().starts_with(CRATE_NAME)));

    let registry = tracing_subscriber::registry();

    let level = level_filter.into_level();

    if let Some(level) = level
        && level > Level::INFO
    {
        registry.with(debug_layer).init();
    } else {
        registry.with(user_layer).init();
    }

    Ok(())
}

pub async fn get_public_ip_address(url: &str) -> Result<IpAddr> {
    Ok(reqwest::get(url).await?.text().await?.parse()?)
}

/// Try each URL in the list until one returns a valid IP address.
pub async fn detect_ip(urls: &[&str]) -> Result<IpAddr> {
    for url in urls {
        match get_public_ip_address(url).await {
            Ok(ip) => return Ok(ip),
            Err(e) => {
                tracing::warn!("Failed to detect IP from {url}: {e}");
            },
        }
    }
    Err(color_eyre::eyre::eyre!(
        "Failed to detect IP from all URLs"
    ))
}
