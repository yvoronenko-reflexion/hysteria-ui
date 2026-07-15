//! Generate a Hysteria2 client YAML config from our [`HysteriaConfig`] model.
//!
//! Only fields that are actually set are emitted, so the output stays close to
//! what a human would hand-write. The user's free-form `advanced` YAML is
//! parsed and merged last, overriding anything above it.

use crate::model::HysteriaConfig;
use serde_yaml::{Mapping, Value};

fn s(v: &str) -> Value {
    Value::from(v.to_owned())
}

/// Render the config to a YAML string, or an error describing what's invalid.
pub fn generate_yaml(cfg: &HysteriaConfig) -> Result<String, String> {
    if cfg.server.trim().is_empty() {
        return Err("Server address is required".to_string());
    }

    let mut root = Mapping::new();
    root.insert(s("server"), s(&cfg.server));
    if !cfg.auth.is_empty() {
        root.insert(s("auth"), s(&cfg.auth));
    }

    // --- tls ---
    let mut tls = Mapping::new();
    if !cfg.tls.sni.is_empty() {
        tls.insert(s("sni"), s(&cfg.tls.sni));
    }
    if cfg.tls.insecure {
        tls.insert(s("insecure"), Value::Bool(true));
    }
    if !cfg.tls.pin_sha256.is_empty() {
        tls.insert(s("pinSHA256"), s(&cfg.tls.pin_sha256));
    }
    if !cfg.tls.ca.is_empty() {
        tls.insert(s("ca"), s(&cfg.tls.ca));
    }
    if !tls.is_empty() {
        root.insert(s("tls"), Value::Mapping(tls));
    }

    // --- obfs (salamander | gecko) ---
    if cfg.obfs.is_enabled() {
        let kind = cfg.obfs.obfs_type.clone();
        let mut inner = Mapping::new();
        if !cfg.obfs.password.is_empty() {
            inner.insert(s("password"), s(&cfg.obfs.password));
        }
        if kind == "gecko" {
            if cfg.obfs.min_packet_size > 0 {
                inner.insert(s("minPacketSize"), Value::from(cfg.obfs.min_packet_size as u64));
            }
            if cfg.obfs.max_packet_size > 0 {
                inner.insert(s("maxPacketSize"), Value::from(cfg.obfs.max_packet_size as u64));
            }
        }
        let mut obfs = Mapping::new();
        obfs.insert(s("type"), s(&kind));
        obfs.insert(s(&kind), Value::Mapping(inner));
        root.insert(s("obfs"), Value::Mapping(obfs));
    }

    // --- bandwidth ---
    let mut bw = Mapping::new();
    if !cfg.bandwidth.up.is_empty() {
        bw.insert(s("up"), s(&cfg.bandwidth.up));
    }
    if !cfg.bandwidth.down.is_empty() {
        bw.insert(s("down"), s(&cfg.bandwidth.down));
    }
    if !bw.is_empty() {
        root.insert(s("bandwidth"), Value::Mapping(bw));
    }

    // --- socks5 ---
    if cfg.socks5.enabled {
        let mut m = Mapping::new();
        let listen = if cfg.socks5.listen.is_empty() {
            "127.0.0.1:1080"
        } else {
            &cfg.socks5.listen
        };
        m.insert(s("listen"), s(listen));
        if !cfg.socks5.username.is_empty() {
            m.insert(s("username"), s(&cfg.socks5.username));
        }
        if !cfg.socks5.password.is_empty() {
            m.insert(s("password"), s(&cfg.socks5.password));
        }
        root.insert(s("socks5"), Value::Mapping(m));
    }

    // --- http ---
    if cfg.http.enabled {
        let mut m = Mapping::new();
        let listen = if cfg.http.listen.is_empty() {
            "127.0.0.1:8080"
        } else {
            &cfg.http.listen
        };
        m.insert(s("listen"), s(listen));
        if !cfg.http.username.is_empty() {
            m.insert(s("username"), s(&cfg.http.username));
        }
        if !cfg.http.password.is_empty() {
            m.insert(s("password"), s(&cfg.http.password));
        }
        root.insert(s("http"), Value::Mapping(m));
    }

    // --- tun (full-tunnel; excludes the server IP to avoid a routing loop) ---
    if cfg.tun.enabled {
        let mut t = Mapping::new();
        if !cfg.tun.name.is_empty() {
            t.insert(s("name"), s(&cfg.tun.name));
        }
        let mtu = if cfg.tun.mtu == 0 { 1500 } else { cfg.tun.mtu };
        t.insert(s("mtu"), Value::from(mtu as u64));
        if !cfg.tun.timeout.is_empty() {
            t.insert(s("timeout"), s(&cfg.tun.timeout));
        }

        let mut v4_exclude: Vec<Value> = Vec::new();
        let mut v6_exclude: Vec<Value> = Vec::new();
        for cidr in &cfg.tun.exclude_routes {
            if cidr.contains(':') {
                v6_exclude.push(s(cidr));
            } else {
                v4_exclude.push(s(cidr));
            }
        }

        let mut route = Mapping::new();
        route.insert(s("ipv4"), Value::Sequence(vec![s("0.0.0.0/0")]));
        if !v4_exclude.is_empty() {
            route.insert(s("ipv4Exclude"), Value::Sequence(v4_exclude));
        }
        if !v6_exclude.is_empty() {
            route.insert(s("ipv6"), Value::Sequence(vec![s("2000::/3")]));
            route.insert(s("ipv6Exclude"), Value::Sequence(v6_exclude));
        }
        t.insert(s("route"), Value::Mapping(route));

        if !cfg.tun.dns.is_empty() {
            t.insert(
                s("dns"),
                Value::Sequence(cfg.tun.dns.iter().map(|d| s(d)).collect()),
            );
        }
        root.insert(s("tun"), Value::Mapping(t));
    }

    if cfg.fast_open {
        root.insert(s("fastOpen"), Value::Bool(true));
    }
    if cfg.lazy {
        root.insert(s("lazy"), Value::Bool(true));
    }

    // --- advanced passthrough (merged last, overrides above) ---
    if !cfg.advanced.trim().is_empty() {
        let adv: Value = serde_yaml::from_str(&cfg.advanced)
            .map_err(|e| format!("Advanced YAML is invalid: {e}"))?;
        match adv {
            Value::Mapping(m) => {
                for (k, v) in m {
                    root.insert(k, v);
                }
            }
            Value::Null => {}
            _ => return Err("Advanced YAML must be a mapping (key: value pairs)".to_string()),
        }
    }

    serde_yaml::to_string(&Value::Mapping(root)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uri::parse_uri;

    #[test]
    fn generates_from_example() {
        let mut p = parse_uri("hysteria2://deadbeefcafe0123@example.com:443?obfs=salamander&pinSHA256=AAAABBBBCCCCDDDDEEEEFFFF00001111222233334444555566667777888899AA&obfs-password=testpassword123&sni=example.org&security=tls&insecure=1#Test-Profile").unwrap();
        p.config.socks5.enabled = true;
        let yaml = generate_yaml(&p.config).unwrap();

        // must be valid YAML and contain the essentials
        let parsed: Value = serde_yaml::from_str(&yaml).unwrap();
        let m = parsed.as_mapping().unwrap();
        assert_eq!(m.get(s("server")).unwrap().as_str().unwrap(), "example.com:443");
        assert_eq!(
            m.get(s("auth")).unwrap().as_str().unwrap(),
            "deadbeefcafe0123"
        );
        let obfs = m.get(s("obfs")).unwrap().as_mapping().unwrap();
        assert_eq!(obfs.get(s("type")).unwrap().as_str().unwrap(), "salamander");
        let sal = obfs.get(s("salamander")).unwrap().as_mapping().unwrap();
        assert_eq!(
            sal.get(s("password")).unwrap().as_str().unwrap(),
            "testpassword123"
        );
        let tls = m.get(s("tls")).unwrap().as_mapping().unwrap();
        assert_eq!(tls.get(s("sni")).unwrap().as_str().unwrap(), "example.org");
        assert!(tls.get(s("insecure")).unwrap().as_bool().unwrap());
        let socks5 = m.get(s("socks5")).unwrap().as_mapping().unwrap();
        assert_eq!(socks5.get(s("listen")).unwrap().as_str().unwrap(), "127.0.0.1:1080");
    }

    #[test]
    fn requires_server() {
        let cfg = HysteriaConfig::default();
        assert!(generate_yaml(&cfg).is_err());
    }

    #[test]
    fn advanced_yaml_merges_and_overrides() {
        let mut cfg = HysteriaConfig {
            server: "h.com:443".to_string(),
            ..Default::default()
        };
        cfg.advanced = "quic:\n  maxIdleTimeout: 30s\nfastOpen: true".to_string();
        let yaml = generate_yaml(&cfg).unwrap();
        let m: Value = serde_yaml::from_str(&yaml).unwrap();
        let root = m.as_mapping().unwrap();
        assert!(root.get(s("quic")).is_some());
        assert!(root.get(s("fastOpen")).unwrap().as_bool().unwrap());
    }

    #[test]
    fn tun_full_tunnel_excludes_server_and_sets_dns() {
        let mut cfg = HysteriaConfig {
            server: "203.0.113.100:443".to_string(),
            ..Default::default()
        };
        cfg.tun.enabled = true;
        cfg.tun.name = "utun99".to_string();
        cfg.tun.dns = vec!["1.1.1.1".to_string()];
        cfg.tun.exclude_routes = vec!["203.0.113.100/32".to_string()];
        let yaml = generate_yaml(&cfg).unwrap();
        let m: Value = serde_yaml::from_str(&yaml).unwrap();
        let tun = m.as_mapping().unwrap().get(s("tun")).unwrap().as_mapping().unwrap();
        assert_eq!(tun.get(s("name")).unwrap().as_str().unwrap(), "utun99");
        let route = tun.get(s("route")).unwrap().as_mapping().unwrap();
        assert_eq!(
            route.get(s("ipv4")).unwrap().as_sequence().unwrap()[0].as_str().unwrap(),
            "0.0.0.0/0"
        );
        assert_eq!(
            route.get(s("ipv4Exclude")).unwrap().as_sequence().unwrap()[0].as_str().unwrap(),
            "203.0.113.100/32"
        );
        assert_eq!(tun.get(s("dns")).unwrap().as_sequence().unwrap()[0].as_str().unwrap(), "1.1.1.1");
    }

    #[test]
    fn invalid_advanced_yaml_errors() {
        let cfg = HysteriaConfig {
            server: "h.com:443".to_string(),
            advanced: "just a string, not a map".to_string(),
            ..Default::default()
        };
        assert!(generate_yaml(&cfg).is_err());
    }
}
