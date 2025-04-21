use std::collections::HashSet;
use har;
use std::net::{IpAddr, Ipv4Addr};
use futures;

pub type StrResult<T> = Result<T, String>;

pub fn hostnames_from_har(path: &str) -> StrResult<HashSet<String>> {
    let har = har::from_path(path).map_err(|e| format!("could not parse HAR file {path}: {e}"))?;
    match har.log {
        har::Spec::V1_2(log) => {
            let hosts = log.entries
                .into_iter()
                .try_fold(HashSet::new(), |mut acc, x| {
                    let hostname = hostname_from_url(&x.request.url)
                        .ok_or(format!("could not extract hostname from URL {}", &x.request.url))?;
                    acc.insert(hostname);
                    Ok::<HashSet<String>, String>(acc)
                })?;
            Ok(hosts)
        },
        har::Spec::V1_3(_log) => {
            todo!()
        }
    }
}

pub fn nameservers_from_host(host: &str) -> StrResult<HashSet<String>> {
    let resolver = hickory_resolver::Resolver::builder_tokio().unwrap().build();
    let domain_name = domain_from_host(host)?;
    //println!("getting nameservers for host {host} and its domain name {domain_name}");
    let lookup_ns_future  = resolver.ns_lookup(domain_name);
    let io_loop = tokio::runtime::Runtime::new().unwrap();
    let response = io_loop.block_on(lookup_ns_future).unwrap();

    let lookup_ip_futures = response.iter().map(|rsp| {
        let ns_hostname = rsp.to_string().trim_end_matches('.').to_string();
        resolver.lookup_ip(ns_hostname)
    }).collect::<Vec<_>>();

    let responses = io_loop.block_on(async{
        futures::future::join_all(lookup_ip_futures).await
    });

    let ns_ips = responses.into_iter()
        .try_fold(HashSet::new(), |mut acc, x| -> StrResult<HashSet<String>> {
            let looked_up = x.map_err(|_| "could not lookup IPs of nameserver hostname".to_string())?;
            let ip = looked_up.iter().next().ok_or("empty IP list for nameserver hostname")?;
            acc.insert(ip.to_string());
            Ok(acc)
        })?;
    Ok(ns_ips)
}

pub fn resolve_host_multiple(host: &str, nameserver_ips: &HashSet<String>) -> StrResult<HashSet<String>> {
    println!("resolving host {host} using nameservers {nameserver_ips:?}");
    let global_dns = ["8.8.8.8", "1.1.1.1", "9.9.9.9"].into_iter().map(|ip_str| IpAddr::V4(ip_str.parse().unwrap()));

    let nameserver_addrs: Result<Vec<IpAddr>, String> = nameserver_ips
        .into_iter()
        .try_fold(Vec::from_iter(global_dns), |mut acc, x| {
            let a = x.parse::<Ipv4Addr>().map_err(|e| format!("could not parse {x} as IPv4 addr: {e}"))?;
            acc.push(IpAddr::V4(a));
            Ok(acc)
        });

    let server_group = hickory_resolver::config::NameServerConfigGroup::from_ips_clear(
        nameserver_addrs?.as_slice(),
        53,
        true
    );

    let ns_config = hickory_resolver::config::ResolverConfig::from_parts(
        None,
        Vec::new(),
        server_group
    );

    let resolver = hickory_resolver::Resolver::builder_with_config(
        ns_config, 
        hickory_resolver::name_server::TokioConnectionProvider::default()).build();

    let lookup_ip_future = resolver.lookup_ip(host);
    let io_loop = tokio::runtime::Runtime::new().unwrap();
    if let Ok(response) = io_loop.block_on(lookup_ip_future) {
        Ok(response.iter().map(|rsp| rsp.to_string()).collect::<HashSet<_>>())
    } else {
        println!("warning: cannot resolve host {host} with nameservers {nameserver_ips:?}");
        Ok(HashSet::new())
    }
}

fn hostname_from_url(url: &str) -> Option<String> {
    let stripped_suffix = url
        .strip_prefix("https://")
        .or(url.strip_prefix("http://"))
        .or(url.strip_prefix("wss://"));
    stripped_suffix.and_then(|s|s.split('/').next()).map(String::from)
}

fn domain_from_host(h: &str) -> StrResult<String> {
    let parts = h.split('.').rev().collect::<Vec<_>>();
    if parts.iter().any(|s| s.is_empty()) {
        return Err(format!("too short component of hostname {h}"));
    }
    let parts = parts.into_iter().take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>();
    if parts.len() < 2 {
        Err(format!("too short hostname {h}"))
    } else {
        Ok(parts.join("."))
    }
}

pub fn hostname_is_ip(s: &str) -> Option<Ipv4Addr> {
    s.parse::<Ipv4Addr>().ok()
}

pub fn discard_port<'a>(s: &'a str) -> &'a str {
    s.split_once(':').map(|(before, _after)| before).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hostname_from_url() {
        assert_eq!(hostname_from_url("https://x.y"), Some("x.y".to_string()));
        assert_eq!(hostname_from_url("https://x.y/"), Some("x.y".to_string()));
        assert_eq!(hostname_from_url("http://x.y"), Some("x.y".to_string()));
        assert_eq!(hostname_from_url("http://x.y/"), Some("x.y".to_string()));
        assert_eq!(hostname_from_url("http://x.y/aksdh/akjsdh"), Some("x.y".to_string()));
        assert_eq!(hostname_from_url("http://x.y//s//h///?sdf=ass"), Some("x.y".to_string()));
        assert_eq!(hostname_from_url("rtsp://x.y"), None);
        assert_eq!(hostname_from_url("rtsp://x.y/"), None);
        assert_eq!(hostname_from_url("x.y"), None);
        assert_eq!(hostname_from_url("x.y/"), None);
    }

    #[test]
    fn test_hostnames_from_har() {
        let may_be_entries = std::fs::read_dir(format!("{}/tests/private/", env!("CARGO_MANIFEST_DIR")))
            .into_iter()
            .flatten();
        std::fs::read_dir(format!("{}/tests/", env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .into_iter()
            .chain(may_be_entries)
            .for_each(|path| {
                let path = path.unwrap().path();
                if path.is_file() && path.extension().is_some_and(|ext| ext.to_str().unwrap() == "har") {
                    let path = path.to_str().unwrap();
                    let hostnames = hostnames_from_har(path).unwrap();
                    println!("{path} yields hostnames ({}): {hostnames:?}", hostnames.len());
                    assert!(hostnames.len() >= 3);
                }
            });
    }

    #[test]
    fn test_resolve_multiple1() {
        let ips = resolve_host_multiple(
            "asus.com", 
            &HashSet::from(["8.8.8.8".into(), "1.1.1.1".into()])).unwrap();
        println!("asus.com => {ips:?}");
        assert!(!ips.is_empty());
    }

    #[test]
    fn test_resolve_multiple2() {
        let ips = resolve_host_multiple(
            "amazon.com", 
            &HashSet::from(["156.154.150.1".into(), "156.154.64.10".into()])).unwrap();
        println!("amazon.com => {ips:?}");
        assert!(!ips.is_empty());
    }

    #[test]
    fn test_domain_from_host() {
        assert_eq!(domain_from_host("x.y"), Ok("x.y".to_string()));
        assert_eq!(domain_from_host("x.y.z"), Ok("y.z".to_string()));
        assert!(domain_from_host("").is_err());
        assert!(domain_from_host("x").is_err());
        assert!(domain_from_host("x..").is_err());
        assert!(domain_from_host("..x").is_err());
        assert!(domain_from_host("x..y").is_err());
        assert!(domain_from_host("x.y.").is_err());
        assert!(domain_from_host(".x.y").is_err());
        assert!(domain_from_host(".x.y.").is_err());
        assert!(domain_from_host(".xxxx.yyy.").is_err());
    }

    #[test]
    fn test_nameservers() {
        let nss = nameservers_from_host("amazon.com").unwrap();
        println!("amazon's webservers: {nss:?}");
    }

    #[test]
    fn test_url_is_ip() {
        assert!(hostname_is_ip("10.1.2.3").is_some());
        assert!(hostname_is_ip("a.b.c.d").is_none());
    }

    #[test]
    fn test_discard_port() {
        assert_eq!(discard_port(""), "");
        assert_eq!(discard_port("noport"), "noport");
        assert_eq!(discard_port("a.b.c:4443"), "a.b.c");
    }
}
