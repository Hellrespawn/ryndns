use clap::Parser;
use color_eyre::Result;

use crate::detect_ip;
use crate::{IPV4_URLS, IPV6_URLS};

#[derive(Debug, Parser)]
/// Show the current public IP addresses (IPv4 and IPv6).
struct Args;

pub async fn main() -> Result<()> {
    crate::init()?;

    let _args = Args::parse();

    let ipv4 = detect_ip(IPV4_URLS).await;
    let ipv6 = detect_ip(IPV6_URLS).await;

    match (&ipv4, &ipv6) {
        (Ok(v4), Ok(v6)) => {
            println!("IPv4: {v4}");
            println!("IPv6: {v6}");
        },
        (Ok(v4), Err(_)) => {
            println!("IPv4: {v4}");
            println!("IPv6: <not detected>");
        },
        (Err(_), Ok(v6)) => {
            println!("IPv4: <not detected>");
            println!("IPv6: {v6}");
        },
        (Err(_), Err(_)) => {
            color_eyre::eyre::bail!(
                "Failed to detect both IPv4 and IPv6 addresses"
            );
        },
    }

    Ok(())
}
