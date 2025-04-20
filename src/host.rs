use std::net::Ipv4Addr;

pub struct Host {
    tcp_conns: Vec<Conn>,
    udp_conns: Vec<Conn>,
}

#[derive(Debug, Clone, PartialEq)]
struct Conn {
    src_ip: Ipv4Addr,
    src_port: u16,
    dst_ip: Ipv4Addr,
    dst_port: u16
}

impl Host {
    pub fn from_proc_net_tcp() -> Result<Self, String> {
        let v = ["tcp", "udp"].iter().map(|proto| -> Result<Vec<Conn>, String> {
            let contents = std::fs::read_to_string(format!("/proc/net/{proto}"))
                .map_err(|e| format!("could not read /proc/net/{proto}: {e}"))?;
            let conns = contents
                .lines()
                .skip(1)
                .try_fold(Vec::new(), |mut acc: Vec<Conn>, line| {
                    //dbg!(line);
                    let fields = line.trim().split(' ').take(3).collect::<Vec<&str>>();
                    if fields.len() < 3 {
                        Err(format!("not enough fields to parse 'ip:port' for proto {proto}: {line}"))
                    } else {
                        let (src_ip, src_port) = parse_ip_port(fields.get(1).unwrap())?;
                        let (dst_ip, dst_port) = parse_ip_port(fields.get(2).unwrap())?;
                        acc.push(Conn{ src_ip, dst_ip, src_port, dst_port });
                        Ok(acc)
                    }
                })?;
            Ok(conns)
        }).take(2).collect::<Vec<_>>();
        let tcp_conns = v.get(0).unwrap().clone()?;
        let udp_conns = v.get(1).unwrap().clone()?;
        Ok(Self { tcp_conns, udp_conns })
    }

    pub fn contains_dst(&self, net_str: &str) -> Option<(String, u16)> {
        let net: ipnetwork::Ipv4Network = net_str.parse().unwrap();
        [&self.tcp_conns, &self.udp_conns].into_iter()
            .flat_map(|conns| conns)
            .find(|c| net.contains(c.dst_ip))
            .map(|c| (c.dst_ip.to_string(), c.dst_port))
    }
}

fn parse_ip_port(s: &str) -> Result<(Ipv4Addr, u16), String> {
    //dbg!(s);
    let mut s_it = s.split(':');
    let s_ip = s_it.next().ok_or(format!("no ip in 'ip:port' pair to parse: {s}"))?;
    let s_port = s_it.next().ok_or(format!("no port in 'ip:port' pair to parse: {s}"))?;
    if s_ip.len() != 8 || s_port.len() != 4 {
        return Err(format!("too short ip:port pair to parse: {s}"));
    }
    let (a, b, c, d) = (from_hex2(&s_ip[0..2])?, from_hex2(&s_ip[2..4])?, from_hex2(&s_ip[4..6])?, from_hex2(&s_ip[6..8])?);
    let (x, y) = (from_hex2(&s_port[0..2])? as u16, from_hex2(&s_port[2..4])? as u16);

    Ok((Ipv4Addr::new(d, c, b, a), (x << 8) + y))
}

fn from_hex2(s: &str) -> Result<u8, String> {
    u8::from_str_radix(s, 16).map_err(|e| format!("could not convert '{s}' from hex string: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ip_port() {
        assert_eq!(parse_ip_port("C301A8C0:E5BC"), Ok((Ipv4Addr::new(192, 168, 1, 195), 58812)));
        assert!(parse_ip_port("C301A8C:E5BC").is_err());
        assert!(parse_ip_port("C301A8C0:E5BCC").is_err());
        assert!(parse_ip_port("C30xA8C0:E5BC").is_err());
    }

    #[test]
    fn test_host_tcp_udp() {
        let host = Host::from_proc_net_tcp().unwrap();
        [("tcp", host.tcp_conns), ("udp", host.udp_conns)].into_iter().for_each(|(title, conns)| {
            println!("{title} connections:");
            conns.into_iter().for_each(|c| println!("\t{}:{} -> {}:{}", c.src_ip, c.src_port, c.dst_ip, c.dst_port));
            println!("\n");
        });
    }

    fn conn_no_ports(src_ip: &str, dst_ip: &str) -> Conn {
        //println!("{src_ip}={:?} {dst_ip}={:?}", src_ip.parse::<Ipv4Addr>(), dst_ip.parse::<Ipv4Addr>());
        Conn { src_ip: src_ip.parse().unwrap(), src_port: 0, dst_ip: dst_ip.parse().unwrap(), dst_port: 0 }
    }

    fn assert_contains_dst(h: &Host, net: &str, expected: Option<&str>) {
        assert_eq!(h.contains_dst(net), expected.map(|ip| (ip.to_string(), 0)));
    }

    #[test]
    fn test_contains() {
        let host = Host {
            tcp_conns: vec![conn_no_ports("192.168.100.4", "192.168.200.5"), conn_no_ports("10.0.1.6", "10.0.2.7")],
            udp_conns: vec![conn_no_ports("172.17.200.4", "172.17.250.5"), conn_no_ports("12.0.1.6", "12.0.2.7")],
        };
        assert_contains_dst(&host, "192.168.200.0/24", Some("192.168.200.5"));
        assert_contains_dst(&host, "172.17.250.0/24", Some("172.17.250.5"));
        assert_contains_dst(&host, "10.0.0.0/16", Some("10.0.2.7"));
        assert_contains_dst(&host, "12.0.0.0/16", Some("12.0.2.7"));

        assert_contains_dst(&host, "13.0.0.0/8", None);
        assert_contains_dst(&host, "192.168.100.0/24", None);
    }
}