use std::net::{IpAddr, SocketAddr};

use crate::error::{Error, Result};

pub(crate) fn url(node_ip: &str, path: &str) -> Result<String> {
    let ip = node_ip
        .parse::<IpAddr>()
        .map_err(|source| Error::InvalidNodeIp {
            value: node_ip.to_string(),
            source,
        })?;
    Ok(format!("https://{}{path}", SocketAddr::new(ip, 10250)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_ipv4_and_ipv6_urls() {
        assert_eq!(
            url("192.0.2.1", "/healthz").expect("valid IPv4 address"),
            "https://192.0.2.1:10250/healthz"
        );
        assert_eq!(
            url("2001:db8::1", "/healthz").expect("valid IPv6 address"),
            "https://[2001:db8::1]:10250/healthz"
        );
    }
}
