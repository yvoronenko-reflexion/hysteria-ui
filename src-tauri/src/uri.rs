//! Conversion between `hysteria2://` share URIs and our [`Profile`] model.
//!
//! Format: `hysteria2://[auth@]host[:port]/?obfs=&obfs-password=&sni=&insecure=&pinSHA256=#name`
//! (`hy2://` is accepted as an alias). The URI intentionally carries only the
//! connection essentials — local settings (proxy listeners, bandwidth, tun)
//! are app-managed and never appear in the URI.

use crate::model::{HysteriaConfig, Obfs, Profile, Tls};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use url::Url;
use uuid::Uuid;

/// Characters we percent-encode inside the userinfo component. `:` is encoded
/// too, so an `auth` value that itself contains `:` (userpass form) survives a
/// round-trip as a single opaque string rather than being split.
const USERINFO: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'|')
    .add(b'&')
    .add(b'%');

/// Characters we percent-encode inside query values and the fragment.
const VALUE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'&')
    .add(b'+')
    .add(b'=')
    .add(b'?')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'%');

fn decode(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// Parse a `hysteria2://` / `hy2://` URI into a [`Profile`] (with a fresh id).
pub fn parse_uri(input: &str) -> Result<Profile, String> {
    let trimmed = input.trim();
    let normalized = match trimmed.strip_prefix("hy2://") {
        Some(rest) => format!("hysteria2://{rest}"),
        None => trimmed.to_string(),
    };
    if !normalized.starts_with("hysteria2://") {
        return Err("URI must start with hysteria2:// or hy2://".to_string());
    }

    let url = Url::parse(&normalized).map_err(|e| format!("Invalid URI: {e}"))?;

    let host = url
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or("URI is missing a server host")?;
    let port = url.port().unwrap_or(443);
    let server = format!("{host}:{port}");

    // Userinfo -> auth. Recombine user:password into our single opaque string.
    let user = decode(url.username());
    let auth = match url.password() {
        Some(pass) if !pass.is_empty() => format!("{user}:{}", decode(pass)),
        _ => user,
    };

    let mut config = HysteriaConfig {
        server,
        auth,
        ..Default::default()
    };
    let mut tls = Tls::default();
    let mut obfs = Obfs::default();

    for (key, value) in url.query_pairs() {
        // query_pairs() already returns percent-decoded values.
        match key.as_ref() {
            "obfs" => obfs.obfs_type = value.into_owned(),
            "obfs-password" => obfs.password = value.into_owned(),
            "sni" => tls.sni = value.into_owned(),
            "insecure" => tls.insecure = matches!(value.as_ref(), "1" | "true"),
            "pinSHA256" => tls.pin_sha256 = value.into_owned(),
            // `security`, `mport`, etc. are ignored — not part of our model.
            _ => {}
        }
    }

    config.tls = tls;
    config.obfs = obfs;

    let name = url
        .fragment()
        .map(decode)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| host.to_string());

    Ok(Profile {
        id: Uuid::new_v4().to_string(),
        name,
        config,
    })
}

/// Build a canonical `hysteria2://` URI from a profile's connection essentials.
pub fn build_uri(profile: &Profile) -> String {
    let cfg = &profile.config;
    let (host, port) = split_host_port(&cfg.server);

    let mut uri = String::from("hysteria2://");
    if !cfg.auth.is_empty() {
        uri.push_str(&utf8_percent_encode(&cfg.auth, USERINFO).to_string());
        uri.push('@');
    }
    uri.push_str(host);
    if let Some(port) = port {
        uri.push(':');
        uri.push_str(port);
    }

    let mut params: Vec<(&str, String)> = Vec::new();
    if cfg.obfs.is_enabled() {
        params.push(("obfs", cfg.obfs.obfs_type.clone()));
        if !cfg.obfs.password.is_empty() {
            params.push(("obfs-password", cfg.obfs.password.clone()));
        }
    }
    if !cfg.tls.sni.is_empty() {
        params.push(("sni", cfg.tls.sni.clone()));
    }
    if cfg.tls.insecure {
        params.push(("insecure", "1".to_string()));
    }
    if !cfg.tls.pin_sha256.is_empty() {
        params.push(("pinSHA256", cfg.tls.pin_sha256.clone()));
    }

    if !params.is_empty() {
        uri.push('?');
        let query: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{k}={}", utf8_percent_encode(v, VALUE)))
            .collect();
        uri.push_str(&query.join("&"));
    }

    if !profile.name.is_empty() {
        uri.push('#');
        uri.push_str(&utf8_percent_encode(&profile.name, VALUE).to_string());
    }

    uri
}

/// Split `host:port` into `(host, Some(port))`. Handles bracketed IPv6 and the
/// port-less / port-hopping (`443,444`) forms by returning the port as an
/// opaque string.
fn split_host_port(server: &str) -> (&str, Option<&str>) {
    let server = server.trim();
    if let Some(rest) = server.strip_prefix('[') {
        // [ipv6]:port
        if let Some(end) = rest.find(']') {
            let host = &server[..end + 2]; // include the brackets
            let after = &rest[end + 1..];
            let port = after.strip_prefix(':').filter(|p| !p.is_empty());
            return (host, port);
        }
    }
    match server.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() => (host, Some(port)),
        _ => (server, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = "hysteria2://deadbeefcafe0123@example.com:443?obfs=salamander&pinSHA256=AAAABBBBCCCCDDDDEEEEFFFF00001111222233334444555566667777888899AA&obfs-password=testpassword123&sni=example.org&security=tls&insecure=1#Test-Profile";

    #[test]
    fn parses_example_uri() {
        let p = parse_uri(EXAMPLE).unwrap();
        assert_eq!(p.config.server, "example.com:443");
        assert_eq!(p.config.auth, "deadbeefcafe0123");
        assert_eq!(p.config.obfs.obfs_type, "salamander");
        assert_eq!(p.config.obfs.password, "testpassword123");
        assert_eq!(p.config.tls.sni, "example.org");
        assert!(p.config.tls.insecure);
        assert_eq!(
            p.config.tls.pin_sha256,
            "AAAABBBBCCCCDDDDEEEEFFFF00001111222233334444555566667777888899AA"
        );
        assert_eq!(p.name, "Test-Profile");
    }

    #[test]
    fn round_trips_semantically() {
        let original = parse_uri(EXAMPLE).unwrap();
        let rebuilt = build_uri(&original);
        let reparsed = parse_uri(&rebuilt).unwrap();
        // ids differ (fresh uuid each parse); compare everything else.
        assert_eq!(original.name, reparsed.name);
        assert_eq!(original.config, reparsed.config);
    }

    #[test]
    fn accepts_hy2_alias_and_default_port() {
        let p = parse_uri("hy2://secret@example.com?sni=example.com#Test").unwrap();
        assert_eq!(p.config.server, "example.com:443");
        assert_eq!(p.config.auth, "secret");
    }

    #[test]
    fn round_trips_gecko_obfs() {
        let mut p = parse_uri("hysteria2://a@h.com:8443#g").unwrap();
        p.config.obfs.obfs_type = "gecko".to_string();
        p.config.obfs.password = "pw".to_string();
        let uri = build_uri(&p);
        assert!(uri.contains("obfs=gecko"));
        let re = parse_uri(&uri).unwrap();
        assert_eq!(re.config.obfs.obfs_type, "gecko");
        assert_eq!(re.config.obfs.password, "pw");
    }

    #[test]
    fn preserves_userpass_auth() {
        let p = parse_uri("hysteria2://user:pass@h.com:443#x").unwrap();
        assert_eq!(p.config.auth, "user:pass");
        let re = parse_uri(&build_uri(&p)).unwrap();
        assert_eq!(re.config.auth, "user:pass");
    }

    #[test]
    fn rejects_non_hysteria_scheme() {
        assert!(parse_uri("https://example.com").is_err());
    }
}
