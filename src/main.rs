use std::collections::{HashSet, HashMap};

mod dns;
use dns::*;
mod host;
use host::Host;
use rayon::prelude::*;

fn parse_hars_from_cmdline() -> StrResult<HashSet<String>> {
    let parse_results = std::env::args()
        .skip(1)
        .collect::<Vec<String>>()
        .into_par_iter()
        .map(|file| hostnames_from_har(&file))
        .collect::<Vec<StrResult<HashSet<String>>>>();

    let ok_hosts = parse_results.into_iter().collect::<StrResult<Vec<HashSet<String>>>>()?;

    Ok(ok_hosts
        .into_iter()
        .flat_map(|hs| hs.into_iter())
        .collect::<HashSet<String>>())
}

fn gen_wg_routes() -> StrResult<String> {
    let hosts = parse_hars_from_cmdline()?;

    let hosts_and_ips = hosts.clone().into_iter().map(|host| -> (String, StrResult<HashSet<String>>) {
        (
            host.clone(),
            {
                let host = discard_port(&host);
                if let Some(ip) = hostname_is_ip(&host) {
                    if ip.is_loopback() || ip.is_broadcast() || ip.is_private() {
                        Ok(HashSet::new())
                    } else {
                        Ok(HashSet::from([host.to_string()]))
                    }
                } else {
                    nameservers_from_host(host).and_then(|nameservers|
                        resolve_host_multiple(host, &nameservers))
                }
            }
        )
    }).collect::<HashMap<_, _>>();
    
    let ok_hosts = hosts_and_ips
        .clone()
        .into_iter()
        .filter_map(|(host, res_ips)| {
            if let Ok(ips) = res_ips {
                Some((host, ips))
            } else {
                None
            }
        })
        .collect::<HashMap<String, HashSet<String>>>();

    let fail_hosts = hosts_and_ips
        .clone()
        .into_iter()
        .filter_map(|(host, res_ips)| {
            if let Err(err) = res_ips {
                Some((host, err))
            } else {
                None
            }
        })
        .collect::<HashMap<String, String>>();

    println!("\nResolved hosts:\n{ok_hosts:?}\n");
    println!("\nUnresolved hosts:\n{fail_hosts:?}\n");

    let host_util = Host::from_proc_net_tcp()?;

    let wg_str = ok_hosts
        .into_iter()
        .flat_map(|(_, ips)| ips).collect::<HashSet<String>>()
        .into_iter()
        .map(|ip| net_from_ip(&ip))
        .filter(|net| {
            if let Some(conn) = host_util.contains_dst(net) {
                println!("warning: host TCP connection to {}:{} would fall into routed network {net}, ignoring it", conn.0, conn.1);
                return false;
            } else {
                return true;
            }
        })
        .collect::<HashSet<String>>()
        .into_iter()
        .collect::<Vec<String>>()
        .join(", ");

    Ok(format!("AllowedIPs = {wg_str}"))
}

fn net_from_ip(ip: &str) -> String {
    let net_rev = ip.split('.').rev().skip(2).collect::<Vec<&str>>();
    let mut net = net_rev.into_iter().rev().collect::<Vec<&str>>();
    net.push("0");
    net.push("0");
    format!("{}/16", net.join("."))
}

fn main() -> Result<(), String>{
    println!("{}", gen_wg_routes()?);
    Ok(())
}
