use hickory_resolver::config::{
    NameServerConfig, NameServerConfigGroup, Protocol, ResolverConfig, ResolverOpts,
};
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::AsyncResolver as HickoryAsyncResolver;
use reqwest::dns::{Addrs, Name, Resolve};
use reqwest::ClientBuilder;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;

pub(crate) fn parse_dns_servers(dns: &str) -> Vec<SocketAddr> {
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

struct HickoryReqwestResolver {
    inner: HickoryAsyncResolver<TokioConnectionProvider>,
}

impl Resolve for HickoryReqwestResolver {
    fn resolve(
        &self,
        name: Name,
    ) -> Pin<Box<dyn Future<Output = Result<Addrs, Box<dyn std::error::Error + Send + Sync>>> + Send>>
    {
        let host = name.as_str().to_string();
        let inner = self.inner.clone();
        Box::pin(async move {
            let resp = inner.lookup_ip(host.as_str()).await?;
            let addrs_vec: Vec<SocketAddr> = resp.iter().map(|ip| SocketAddr::new(ip, 0)).collect();
            let addrs: Addrs = Box::new(addrs_vec.into_iter());
            Ok(addrs)
        })
    }
}

/// If `dns` is provided, build a hickory AsyncResolver with the specified name servers
/// and inject it into the reqwest client so that all hostnames are resolved via these servers.
pub(crate) fn use_custom_dns_if_present(
    client_builder: ClientBuilder,
    dns: Option<&str>,
) -> ClientBuilder {
    let Some(hosts_str) = dns else {
        return client_builder;
    };

    let servers = parse_dns_servers(hosts_str);
    if servers.is_empty() {
        return client_builder;
    }

    let mut group = NameServerConfigGroup::with_capacity(servers.len());
    for addr in servers {
        group.push(NameServerConfig::new(addr, Protocol::Udp));
        group.push(NameServerConfig::new(addr, Protocol::Tcp));
    }
    let resolver_cfg = ResolverConfig::from_parts(None, vec![], group);
    let resolver_opts = ResolverOpts::default();

    // Build an AsyncResolver that uses the current Tokio runtime
    let inner = HickoryAsyncResolver::new(
        resolver_cfg,
        resolver_opts,
        TokioConnectionProvider::default(),
    );

    let resolver = HickoryReqwestResolver { inner };
    client_builder.dns_resolver(Arc::new(resolver))
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
