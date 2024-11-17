use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr, path::PathBuf,
};

use cidr_utils::cidr::IpCidr;
use clap::Parser;
use interfaces::{Address, Interface};

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Config {
    /// the address to listen on for the status server
    #[arg(short, long, default_value = "0.0.0.0:3053")]
    pub listen: SocketAddr,

    /// The domain to update on porkbun
    #[arg(short, long)]
    pub domain: String,

    /// Map of interface to domain (e.g. eth0=sub or just eth0)
    #[arg(short, long = "interface", value_parser)]
    pub interface_domains: Vec<InterfaceSubdomain>,

    /// Ping the porkbun API to verify credentials
    #[arg(long)]
    pub ping: bool,

    /// Only update the DNS once and exit
    #[arg(long)]
    pub once: bool,

    /// Test the retrieval of records
    #[arg(long)]
    pub test: Option<String>,

    #[arg(long)]
    /// Write pid to file
    pub write_pid: Option<PathBuf>,
}

impl Config {
    pub fn domains(&self) -> impl Iterator<Item = (String, Vec<IpAddr>)> + '_ {
        self.interface_domains.iter().map(|d| {
            let domain: String = d.subdomain.clone().unwrap_or_default();
            let ips = Interface::get_by_name(&d.interface)
                .ok()
                .flatten()
                .map(|i| public_ips(&i))
                .unwrap_or_default();
            (domain, ips)
        })
    }
}
#[derive(Debug, Clone)]
pub struct InterfaceSubdomain {
    interface: String,
    subdomain: Option<String>,
}

impl FromStr for InterfaceSubdomain {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once('=') {
            Some((interface, subdomain)) => {
                let interface = interface.to_string();
                let subdomain = Some(subdomain.to_string());
                Ok(Self {
                    interface,
                    subdomain,
                })
            }
            None => Ok(Self {
                interface: s.to_string(),
                subdomain: None,
            }),
        }
    }
}

fn public_ips(iface: &Interface) -> Vec<IpAddr> {
    iface
        .addresses
        .iter()
        .filter_map(is_public)
        .collect::<Vec<_>>()
}

fn is_public(address: &Address) -> Option<IpAddr> {
    let sockaddr = address.addr?;
    let ip = sockaddr.ip();
    let Ok(global_unicast) = IpCidr::from_str("2000::/3") else { return None };

    match &ip {
        IpAddr::V4(ip4) if !ip4.is_private() && !ip4.is_link_local() && !ip4.is_loopback() => {
            Some(ip)
        }
        IpAddr::V6(_) if global_unicast.contains(&ip) => Some(ip),
        _ => None,
    }
}
