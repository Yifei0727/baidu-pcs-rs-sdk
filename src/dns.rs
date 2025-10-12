use hickory_resolver::config::{
    NameServerConfig, NameServerConfigGroup, Protocol, ResolverConfig, ResolverOpts,
};
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::AsyncResolver;
use log::debug;
use std::net::{IpAddr, SocketAddr};

fn parse_dns_servers(dns: &str) -> Vec<SocketAddr> {
    dns.split(',')
        .filter_map(|s| {
            let s = s.trim();
            if s.is_empty() {
                return None;
            }
            if let Ok(sa) = s.parse::<SocketAddr>() {
                Some(sa)
            } else if let Ok(ip) = s.parse::<IpAddr>() {
                Some(SocketAddr::new(ip, 53))
            } else {
                None
            }
        })
        .collect()
}

async fn resolve_hosts_with_servers(
    servers: &[SocketAddr],
    hosts: &[&str],
) -> Vec<(String, Vec<IpAddr>)> {
    let mut group = NameServerConfigGroup::with_capacity(servers.len());
    for s in servers {
        group.push(NameServerConfig::new(*s, Protocol::Udp));
        // Also add TCP as fallback
        group.push(NameServerConfig::new(*s, Protocol::Tcp));
    }
    // No system config, only specified servers
    let cfg = ResolverConfig::from_parts(None, vec![], group);
    let opts = ResolverOpts::default();

    // hickory-resolver 0.24: `new` returns the resolver directly
    let resolver = AsyncResolver::new(cfg, opts, TokioConnectionProvider::default());

    let mut results = Vec::with_capacity(hosts.len());
    for &h in hosts {
        match resolver.lookup_ip(h).await {
            Ok(lookup) => {
                let ips: Vec<IpAddr> = lookup.iter().collect();
                if !ips.is_empty() {
                    debug!("DNS {} -> {:?}", h, ips);
                    results.push((h.to_string(), ips));
                }
            }
            Err(e) => {
                debug!("DNS lookup failed for {}: {}", h, e);
            }
        }
    }
    results
}

pub fn apply_custom_dns(
    mut builder: reqwest::ClientBuilder,
    dns: Option<&str>,
    hosts: &[&str],
) -> reqwest::ClientBuilder {
    let Some(dns) = dns else {
        return builder;
    };
    let servers = parse_dns_servers(dns);
    if servers.is_empty() {
        return builder;
    }
    // resolve in a temporary runtime if no current
    let rt = tokio::runtime::Runtime::new().expect("create temp tokio runtime for DNS");
    let resolved = rt.block_on(resolve_hosts_with_servers(&servers, hosts));
    drop(rt);
    for (host, ips) in resolved {
        for ip in ips {
            // reqwest::ClientBuilder::resolve expects a SocketAddr; map common HTTP/HTTPS ports
            builder = builder.resolve(&host, SocketAddr::new(ip, 80));
            builder = builder.resolve(&host, SocketAddr::new(ip, 443));
        }
    }
    builder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dns_servers_basic() {
        let out = parse_dns_servers("8.8.8.8");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], "8.8.8.8:53".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn test_parse_dns_servers_with_ports_and_whitespace() {
        let out = parse_dns_servers(" 1.1.1.1:5353 ,  8.8.4.4 ");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "1.1.1.1:5353".parse::<SocketAddr>().unwrap());
        assert_eq!(out[1], "8.8.4.4:53".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn test_parse_dns_servers_ignores_empty() {
        let out = parse_dns_servers(",,  ,\n\t");
        assert!(out.is_empty());
    }
}
