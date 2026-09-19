use reqwest::ClientBuilder;
use std::net::{IpAddr, SocketAddr};

#[cfg(feature = "dns")]
use hickory_resolver::config::{
    NameServerConfig, NameServerConfigGroup, Protocol, ResolverConfig, ResolverOpts,
};
#[cfg(feature = "dns")]
use hickory_resolver::name_server::TokioConnectionProvider;
#[cfg(feature = "dns")]
use hickory_resolver::AsyncResolver as HickoryAsyncResolver;
#[cfg(feature = "dns")]
use reqwest::dns::{Addrs, Name, Resolve};
#[cfg(feature = "dns")]
use std::future::Future;
#[cfg(feature = "dns")]
use std::pin::Pin;
#[cfg(feature = "dns")]
use std::sync::Arc;

#[allow(dead_code)]
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

#[cfg(feature = "dns")]
struct HickoryReqwestResolver {
    inner: HickoryAsyncResolver<TokioConnectionProvider>,
}

#[cfg(feature = "dns")]
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

/// 若提供了 `dns` 且启用了 `dns` 特性，则构建并注入基于 hickory-resolver 的自定义 DNS 解析器。
/// 若未启用 `dns` 特性（例如第三方集成或 macOS 后端关闭了此特性），则完全基于系统原生 DNS（getaddrinfo），不引入 hickory-resolver。
pub(crate) fn use_custom_dns_if_present(
    client_builder: ClientBuilder,
    dns: Option<&str>,
) -> ClientBuilder {
    #[cfg(feature = "dns")]
    {
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

        // 构建绑定当前 Tokio runtime 的 Hickory AsyncResolver
        let inner = HickoryAsyncResolver::new(
            resolver_cfg,
            resolver_opts,
            TokioConnectionProvider::default(),
        );

        let resolver = HickoryReqwestResolver { inner };
        client_builder.dns_resolver(Arc::new(resolver))
    }

    #[cfg(not(feature = "dns"))]
    {
        if dns.is_some() {
            log::warn!("未启用 dns 特性，已忽略自定义 DNS 配置并使用系统原生 DNS (getaddrinfo)");
        }
        client_builder
    }
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

    #[test]
    fn test_use_custom_dns_none() {
        let builder = reqwest::Client::builder();
        let _ = use_custom_dns_if_present(builder, None);
    }

    #[test]
    fn test_use_custom_dns_with_server() {
        let builder = reqwest::Client::builder();
        let _ = use_custom_dns_if_present(builder, Some("8.8.8.8"));
    }
}
