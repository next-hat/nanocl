use std::collections::HashMap;

use serde::{Serialize, Deserialize};

// Helper for tabled may be needed later
pub fn optional_string(s: &Option<String>) -> String {
  match s {
    None => String::from(""),
    Some(s) => s.to_owned(),
  }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PgGenericDelete {
  pub(crate) count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PgGenericCount {
  pub(crate) count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenericNamespaceQuery {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) namespace: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProgressDetail {
  #[serde(rename = "current")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub current: Option<i64>,

  #[serde(rename = "total")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub total: Option<i64>,
}

#[allow(non_camel_case_types)]
#[derive(
  Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Eq, Ord,
)]
pub enum PortTypeEnum {
  #[serde(rename = "")]
  Empty,
  #[serde(rename = "tcp")]
  Tcp,
  #[serde(rename = "udp")]
  Udp,
  #[serde(rename = "sctp")]
  Sctp,
}

/// An open port on a container
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Port {
  /// Host IP address that the container's port is mapped to
  #[serde(rename = "IP")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ip: Option<String>,

  /// Port on the container
  #[serde(rename = "PrivatePort")]
  pub private_port: i64,

  /// Port exposed on the host
  #[serde(rename = "PublicPort")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub public_port: Option<i64>,

  #[serde(rename = "Type")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub typ: Option<PortTypeEnum>,
}

/// EndpointIPAMConfig represents an endpoint's IPAM configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EndpointIpamConfig {
  #[serde(rename = "IPv4Address")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ipv4_address: Option<String>,

  #[serde(rename = "IPv6Address")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ipv6_address: Option<String>,

  #[serde(rename = "LinkLocalIPs")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub link_local_i_ps: Option<Vec<String>>,
}

/// Configuration for a network endpoint.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EndpointSettings {
  #[serde(rename = "IPAMConfig")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ipam_config: Option<EndpointIpamConfig>,

  #[serde(rename = "Links")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub links: Option<Vec<String>>,

  #[serde(rename = "Aliases")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub aliases: Option<Vec<String>>,

  /// Unique ID of the network.
  #[serde(rename = "NetworkID")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub network_id: Option<String>,

  /// Unique ID for the service endpoint in a Sandbox.
  #[serde(rename = "EndpointID")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub endpoint_id: Option<String>,

  /// Gateway address for this network.
  #[serde(rename = "Gateway")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub gateway: Option<String>,

  /// IPv4 address.
  #[serde(rename = "IPAddress")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ip_address: Option<String>,

  /// Mask length of the IPv4 address.
  #[serde(rename = "IPPrefixLen")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ip_prefix_len: Option<i64>,

  /// IPv6 gateway address.
  #[serde(rename = "IPv6Gateway")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ipv6_gateway: Option<String>,

  /// Global IPv6 address.
  #[serde(rename = "GlobalIPv6Address")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub global_ipv6_address: Option<String>,

  /// Mask length of the global IPv6 address.
  #[serde(rename = "GlobalIPv6PrefixLen")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub global_ipv6_prefix_len: Option<i64>,

  /// MAC address for the endpoint on this network.
  #[serde(rename = "MacAddress")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub mac_address: Option<String>,

  /// DriverOpts is a mapping of driver options and values. These options are passed directly to the driver and are driver specific.
  #[serde(rename = "DriverOpts")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub driver_opts: Option<HashMap<String, String>>,
}
