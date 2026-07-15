//! Data model for a Hysteria2 client profile.
//!
//! The model captures the *common* config fields explicitly and keeps an
//! `advanced` raw-YAML passthrough for everything else (quic, port-hopping,
//! forwarding, sniff/acl...) so round-trips never silently drop fields.
//!
//! Serialization uses camelCase to line up with the TypeScript types the
//! frontend sends over the Tauri IPC boundary.

use serde::{Deserialize, Serialize};

/// A single saved server profile.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub config: HysteriaConfig,
}

/// The editable Hysteria2 client configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HysteriaConfig {
    /// Server address as `host:port` (port defaults to 443 when omitted).
    pub server: String,
    /// Authentication credential (single string; may be `user:pass`).
    pub auth: String,
    pub tls: Tls,
    pub obfs: Obfs,
    pub bandwidth: Bandwidth,
    pub socks5: Socks5,
    pub http: Http,
    pub tun: Tun,
    pub fast_open: bool,
    pub lazy: bool,
    /// Free-form YAML mapping merged last into the generated config.
    pub advanced: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Tls {
    pub sni: String,
    pub insecure: bool,
    #[serde(rename = "pinSHA256")]
    pub pin_sha256: String,
    pub ca: String,
}

/// Obfuscation type. `none` disables the `obfs` block entirely.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Obfs {
    /// One of `none`, `salamander`, `gecko`.
    #[serde(rename = "type")]
    pub obfs_type: String,
    pub password: String,
    /// gecko-only; 0 means "use hysteria default".
    pub min_packet_size: u32,
    /// gecko-only; 0 means "use hysteria default".
    pub max_packet_size: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Bandwidth {
    pub up: String,
    pub down: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Socks5 {
    pub enabled: bool,
    pub listen: String,
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Http {
    pub enabled: bool,
    pub listen: String,
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Tun {
    pub enabled: bool,
    /// On macOS this must be `utun<number>`; the runner assigns a free one.
    pub name: String,
    pub mtu: u32,
    pub timeout: String,
    pub dns: Vec<String>,
    /// CIDRs kept OUTSIDE the tunnel (server IPs, filled at connect time) so the
    /// proxy's own packets aren't routed back into the tunnel.
    pub exclude_routes: Vec<String>,
}

impl Obfs {
    pub fn is_enabled(&self) -> bool {
        matches!(self.obfs_type.as_str(), "salamander" | "gecko")
    }
}
