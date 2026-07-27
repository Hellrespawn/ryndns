# ryndns: Full IPv6 (AAAA) Support

## Summary

The data model and provider layers already support AAAA records — the `DnsRecordType::AAAA` variant exists, both Cloudflare and bunny.net can list and update AAAA records, and user config accepts `{ type = "AAAA", name = "..." }`. However, the runtime pipeline is entirely IPv4-only: public IP detection returns `Ipv4Addr`, application state stores `Ipv4Addr`, the CLI overrides accept `Ipv4Addr`, the IP cache stores `Ipv4Addr`, and the update loop applies a single IPv4 address to all matched records regardless of type.

This spec adds full dual-stack IPv4+IPv6 support across the runtime pipeline with strict auto-routing (IPv4→A, IPv6→AAAA) driven by user config.

---

## `PublicIps` Enum (`src/state.rs`)

Introduce a new enum that replaces all `Ipv4Addr` usage in state, cache, and the update loop:

```rust
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicIps {
    V4(IpAddr),
    V6(IpAddr),
    Both { ipv4: IpAddr, ipv6: IpAddr },
}

impl PublicIps {
    /// Routes a DNS record type to the correct IP family.
    /// A → ipv4, AAAA → ipv6, all other types → None.
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
```

The enum is exhaustive — it cannot represent a state with zero IP addresses. `IpAddr` enforces valid address syntax at parse time.

---

## IP Detection (`src/lib.rs`)

### Hardcoded URL lists

`public_ip_url` is removed from the config file. Instead, two lists of fallback URLs are hardcoded:

```rust
const IPV4_URLS: &[&str] = &[
    "https://api.ipify.org",
    "https://ifconfig.me/ip",
    "https://icanhazip.com",
];

const IPV6_URLS: &[&str] = &[
    "https://api6.ipify.org",
    "https://ifconfig.co/ip",
];
```

### Detection function

```rust
use std::net::IpAddr;

async fn detect_ip(urls: &[&str]) -> Result<IpAddr> {
    for url in urls {
        if let Ok(ip) = get_public_ip_address(url).await {
            return Ok(ip);
        }
        warn!("Failed to detect IP from {url}, trying next...");
    }
    Err(eyre!("Failed to detect IP from all URLs"))
}
```

`get_public_ip_address` return type changes from `Ipv4Addr` to `IpAddr` (`&str` parse produces either).

### Resolution logic

Called from `dyndns.rs`:

```
1. ipv4 = args.ipv4_address, or detect_ip(IPV4_URLS) if not provided
2. ipv6 = args.ipv6_address, or detect_ip(IPV6_URLS) if not provided
3. Build PublicIps from the resolved values
```

Both families are always attempted unconditionally. CLI overrides skip detection for that family only.

---

## CLI Changes (`src/cli/dyndns.rs`)

### Args

- **Removed**: `--ip-address` (`Option<Ipv4Addr>`)
- **Added**: `--ipv4-address` (`Option<IpAddr>`), `--ipv6-address` (`Option<IpAddr>`)
- `--force`, `--preview`, `--config`, `--ip-cache` are unchanged

### Workflow

```
1. Resolve PublicIps:
   a. ipv4 = --ipv4-address, or detect_ip(IPV4_URLS)
   b. ipv6 = --ipv6-address, or detect_ip(IPV6_URLS)
   c. Build PublicIps::V4 / V6 / Both from resolved values

2. Check cache:
   a. For each zone in cache, if neither family changed → skip (unless --force)
   b. This avoids hitting the DNS provider API when IPs are unchanged

3. Load config, for each zone with changed IPs:
   a. Fetch DNS records from provider
   b. Filter records matching zone_config → collect targeted record types
      → determine needed_families
   c. Validate: if config targets A records but no IPv4 was resolved → error
                if config targets AAAA records but no IPv6 was resolved → error
                If neither family is needed → warn (empty zone_config)
   d. For each matched record:
      - ip = public_ips.get_for(record_type).expect("validated above")
      - If record.content != ip.to_string() → enqueue update
   e. Write PublicIps to cache for this zone (only on actual updates, not in preview)

4. Save cache file (unless --preview)
```

### Error behavior

Missing needed families are fatal errors and prevent ALL DNS updates — no partial writes. The check happens at step 3c, after DNS records have been fetched but before any mutations. Detecting a family that no config targets is not an error — it's silently discarded.

### `--force`

Skips the cache check at step 2. All zones are processed regardless of whether IPs changed. Updates still only happen if the record content differs from the resolved IP.

### `--preview`

Skips DNS updates and cache writes. Logs "Would update {name}" for each record that would change.

---

## IP Cache (`src/ip_cache/mod.rs`)

### Data structure

```rust
pub struct IpCache {
    cache: IndexMap<String, PublicIps>,
}

impl IpCache {
    /// True if the new IPs differ from cached state (or no cache entry exists).
    pub fn has_changed(&self, zone_id: &str, ips: &PublicIps) -> bool;

    /// Store the IPs for this zone.
    pub fn set(&mut self, zone_id: &str, ips: PublicIps);
}
```

### File format

```
zone_id;ipv4_addr;ipv6_addr
```

- v4 or v6 may be empty (no value for that family) — the column must still exist
- Old 2-column lines (`zone_id;ipv4`) parse as `PublicIps::V4` for backward compat, then are invalidated by `has_changed` if a v6 IP is now present and needed

### `IpCacheResult` removal

The `IpCacheResult::Changed/Unchanged/New` enum is removed. `has_changed()` returns a simple `bool`. Logging of the change reason moves to the caller in `dyndns.rs`.

---

## `public_ip` Subcommand (`src/cli/public_ip.rs`)

No longer reads `public_ip_url` from config. Uses the same hardcoded `IPV4_URLS` and `IPV6_URLS` via `detect_ip()`. Output shows both addresses:

```
$ ryndns public-ip
IPv4: 203.0.113.5
IPv6: 2001:db8::1
```

If one family fails detection, show `<not detected>` (or similar) for that line. If both fail, error and exit.

---

## Config Changes (`src/config/mod.rs`)

- **Removed**: `public_ip_url` field from `ApplicationConfig`
- **Removed**: `public_ip_url()` accessor method
- No other config changes — AAAA record selection via `{ type = "AAAA", name = "..." }` already works
- Example config (`ryndns.example.toml`) and test config (`test/example.toml`) updated to remove `public_ip_url`
- Tests updated accordingly

---

## Provider Layer

**No changes.** `DnsRecordType::AAAA` already exists. Both `CloudflareProvider` and `BunnyProvider` already list, parse, and update AAAA records. The `update_record` method accepts an `ip: &str` parameter — it doesn't care about the address family.

---

## Files Changed

| File | Changes |
|------|---------|
| `src/lib.rs` | Return `IpAddr`, add `IPV4_URLS`/`IPV6_URLS` constants, add `detect_ip()` |
| `src/state.rs` | Add `PublicIps` enum, replace `Ipv4Addr` field with `public_ips: PublicIps` |
| `src/ip_cache/mod.rs` | `IndexMap<String, PublicIps>`, `has_changed()`/`set()`, new file format, remove `IpCacheResult` |
| `src/cli/dyndns.rs` | Replace `--ip-address` with `--ipv4-address`/`--ipv6-address`, two-pass loop with validation |
| `src/cli/public_ip.rs` | Hardcode URLs, show both v4 and v6 |
| `src/config/mod.rs` | Remove `public_ip_url` field and accessor |
| `ryndns.example.toml` | Remove `public_ip_url` line |
| `test/example.toml` | Remove `public_ip_url` line |

**No changes** to `src/provider/`, `src/cloudflare_api/`, `src/bunny_api/`, or `src/config/fs.rs`.

---

## Out of Scope

- IPv6 privacy extensions (temporary addresses)
- Additional record types (MX, TXT, SRV, CNAME) receiving IP updates — strict auto-routing only applies to A and AAAA
- Per-record family override — routing is always type-driven (A→v4, AAAA→v6)
- Configurable IP detection URLs — they remain hardcoded
