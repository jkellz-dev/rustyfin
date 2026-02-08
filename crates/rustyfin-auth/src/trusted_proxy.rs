use std::net::IpAddr;

use ipnet::IpNet;

/// Maintains a list of trusted proxy CIDRs.
/// Forward auth headers are only honored from these IPs.
#[derive(Debug, Clone)]
pub struct TrustedProxies {
    cidrs: Vec<IpNet>,
}

impl TrustedProxies {
    pub fn new(cidrs: Vec<IpNet>) -> Self {
        Self { cidrs }
    }

    /// Check whether the given IP address falls within any trusted CIDR.
    pub fn is_trusted(&self, ip: &IpAddr) -> bool {
        self.cidrs.iter().any(|cidr| cidr.contains(ip))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipv4_loopback_trusted() {
        let proxies = TrustedProxies::new(vec!["127.0.0.1/32".parse().unwrap()]);
        assert!(proxies.is_trusted(&"127.0.0.1".parse().unwrap()));
        assert!(!proxies.is_trusted(&"127.0.0.2".parse().unwrap()));
    }

    #[test]
    fn test_ipv4_cidr() {
        let proxies = TrustedProxies::new(vec!["10.0.0.0/8".parse().unwrap()]);
        assert!(proxies.is_trusted(&"10.0.0.1".parse().unwrap()));
        assert!(proxies.is_trusted(&"10.255.255.255".parse().unwrap()));
        assert!(!proxies.is_trusted(&"11.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_ipv6_cidr() {
        let proxies = TrustedProxies::new(vec!["fd7a:115c:a1e0::/48".parse().unwrap()]);
        assert!(proxies.is_trusted(&"fd7a:115c:a1e0::1".parse().unwrap()));
        assert!(!proxies.is_trusted(&"fd7a:115c:a1e1::1".parse().unwrap()));
    }

    #[test]
    fn test_multiple_cidrs() {
        let proxies = TrustedProxies::new(vec![
            "10.0.0.0/8".parse().unwrap(),
            "172.16.0.0/12".parse().unwrap(),
            "100.64.0.0/10".parse().unwrap(),
        ]);
        assert!(proxies.is_trusted(&"10.1.2.3".parse().unwrap()));
        assert!(proxies.is_trusted(&"172.20.0.1".parse().unwrap()));
        assert!(proxies.is_trusted(&"100.100.0.1".parse().unwrap()));
        assert!(!proxies.is_trusted(&"8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn test_empty_list() {
        let proxies = TrustedProxies::new(vec![]);
        assert!(!proxies.is_trusted(&"127.0.0.1".parse().unwrap()));
    }
}
