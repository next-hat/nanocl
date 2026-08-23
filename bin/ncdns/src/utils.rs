use std::{collections::HashMap, net::IpAddr};

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocld_client::NanocldClient;
use nanocld_client::stubs::{
  dns::ResourceDnsRule,
  generic::NetworkKind,
  network::{Network, gen_network_key},
};

use crate::dnsmasq::DnsmasqScope;

/// Get public address of host
async fn get_host_addr(client: &NanocldClient) -> IoResult<String> {
  let info = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get host info"))?;
  Ok(info.host_gateway)
}

async fn get_local_node(client: &NanocldClient) -> IoResult<String> {
  if let Ok(node) = std::env::var("NANOCL_NODE") {
    let trimmed = node.trim();
    if !trimmed.is_empty() {
      return Ok(trimmed.to_owned());
    }
  }
  let info = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get local node info"))?;
  if !info.config.hostname.trim().is_empty() {
    return Ok(info.config.hostname);
  }
  info
    .docker
    .name
    .filter(|name| !name.trim().is_empty())
    .ok_or_else(|| {
      IoError::invalid_data(
        "Network",
        "NANOCL_NODE is unset and daemon info has no node name",
      )
    })
}

fn network_gateway(network: &Network) -> IoResult<String> {
  network
    .data
    .ipam
    .as_ref()
    .and_then(|ipam| ipam.config.as_ref())
    .and_then(|configs| {
      configs
        .iter()
        .filter_map(|config| config.gateway.as_deref())
        .find(|gateway| !gateway.is_empty())
    })
    .map(str::to_owned)
    .ok_or_else(|| {
      IoError::invalid_data(
        "Network",
        &format!("Network {} has no IPAM gateway", network.name),
      )
    })
}

async fn get_named_network_addr(
  name: &str,
  client: &NanocldClient,
) -> IoResult<String> {
  let node_name = get_local_node(client).await?;
  let key = gen_network_key(&node_name, name);
  let network = client.inspect_network(&key).await.map_err(|err| {
    err.map_err_context(|| {
      format!("Unable to inspect network {name} on node {node_name}")
    })
  })?;
  network_gateway(&network)
}

/// Get address of nanoclbr0 network
async fn get_bridge_addr(client: &NanocldClient) -> IoResult<String> {
  let info = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get host info"))?;
  let ipam = info.network.ipam.unwrap_or_default();
  let ipam_config = ipam.config.unwrap_or_default();
  let Some(network) = ipam_config.first() else {
    return Err(IoError::invalid_data(
      "Network",
      "No network found for nanoclbr0",
    ));
  };
  Ok(network.gateway.clone().unwrap_or_default())
}

/// Resolve a DNS listener or entry selector into a concrete local address.
async fn get_network_addr(
  network: &NetworkKind,
  client: &NanocldClient,
) -> IoResult<IpAddr> {
  let addr = match network {
    NetworkKind::Local => "127.0.0.1".to_owned(),
    NetworkKind::Public => get_host_addr(client).await?,
    NetworkKind::Internal => get_bridge_addr(client).await?,
    NetworkKind::Named(name) => get_named_network_addr(name, client).await?,
    NetworkKind::Other(ip) => return Ok(*ip),
    NetworkKind::All => {
      return Err(IoError::invalid_input(
        "Network",
        "All network is not supported by DNS rules",
      ));
    }
  };
  addr.parse::<IpAddr>().map_err(|err| {
    IoError::invalid_data(
      "Network",
      &format!("Invalid resolved network address {addr}: {err}"),
    )
  })
}

async fn resolve_cached(
  selector: &NetworkKind,
  cache: &mut HashMap<NetworkKind, IpAddr>,
  client: &NanocldClient,
) -> IoResult<IpAddr> {
  if let Some(address) = cache.get(selector) {
    return Ok(*address);
  }
  let address = get_network_addr(selector, client).await?;
  cache.insert(selector.clone(), address);
  Ok(address)
}

fn build_scope(
  rule: &ResourceDnsRule,
  cache: &HashMap<NetworkKind, IpAddr>,
) -> IoResult<DnsmasqScope> {
  let listen_address = *cache.get(&rule.network).ok_or_else(|| {
    IoError::invalid_data(
      "Network",
      &format!("Network selector {} was not resolved", rule.network),
    )
  })?;
  let mut scope = DnsmasqScope::new(listen_address);
  for entry in &rule.entries {
    let address = *cache.get(&entry.ip_address).ok_or_else(|| {
      IoError::invalid_data(
        "Network",
        &format!("Network selector {} was not resolved", entry.ip_address),
      )
    })?;
    scope.add_entry(entry.name.clone(), address);
  }
  Ok(scope)
}

/// Resolve one complete Resource into its single dnsmasq listener scope.
pub(crate) async fn resolve_rule(
  rule: &ResourceDnsRule,
  client: &NanocldClient,
) -> IoResult<DnsmasqScope> {
  let mut cache = HashMap::new();
  resolve_cached(&rule.network, &mut cache, client).await?;
  for entry in &rule.entries {
    resolve_cached(&entry.ip_address, &mut cache, client).await?;
  }
  build_scope(rule, &cache)
}

#[cfg(test)]
pub mod tests {
  use std::collections::HashMap;

  pub use nanocl_utils::ntex::test_client::*;
  use nanocld_client::stubs::{
    dns::{DnsEntry, ResourceDnsRule},
    generic::NetworkKind,
  };
  use nanocld_client::{ConnectOpts, NanocldClient};
  use ntex::rt;

  use super::build_scope;
  use crate::{dnsmasq, services, vars};

  #[test]
  fn one_rule_builds_one_scope_and_deduplicates_its_entries() {
    let listener = "10.0.0.1".parse().unwrap();
    let address = "10.0.0.2".parse().unwrap();
    let rule = ResourceDnsRule {
      network: NetworkKind::Other(listener),
      entries: vec![
        DnsEntry {
          name: "api.internal".to_owned(),
          ip_address: NetworkKind::Other(address),
        },
        DnsEntry {
          name: "api.internal".to_owned(),
          ip_address: NetworkKind::Other(address),
        },
      ],
    };
    let cache = HashMap::from([
      (rule.network.clone(), listener),
      (rule.entries[0].ip_address.clone(), address),
    ]);

    let scope = build_scope(&rule, &cache).unwrap();

    assert_eq!(scope.listen_address, listener);
    assert_eq!(scope.entries["api.internal"].len(), 1);
  }

  #[test]
  fn named_network_rule_parses_from_state_yaml() {
    let rule = serde_yaml::from_str::<ResourceDnsRule>(
      r#"
Network: private-api
Entries:
- Name: database.internal
  IpAddress: private-db
"#,
    )
    .unwrap();

    assert_eq!(rule.network, NetworkKind::Named("private-api".to_owned()));
    assert_eq!(
      rule.entries[0].ip_address,
      NetworkKind::Named("private-db".to_owned())
    );
  }

  // Before a test
  pub fn before() {
    // Build a test env logger
    unsafe {
      std::env::set_var("TEST", "true");
    }
  }

  // Generate a test server
  pub async fn gen_default_test_client() -> TestClient {
    before();
    let dnsmasq = dnsmasq::Dnsmasq::new("/tmp/dnsmasq");
    dnsmasq
      .ensure()
      .expect("Expect to setup minimal dnsmasq config");
    let dnsmasq_cpy = dnsmasq.clone();
    rt::Arbiter::new().handle().spawn(async move {
      dnsmasq_cpy
        .start()
        .await
        .expect("Expect to start dnsmasq for tests");
    });
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    // Create test server
    let srv = ntex::web::test::server(async move || {
      ntex::web::App::new()
        .state(dnsmasq.clone())
        .state(client.clone())
        .configure(services::ntex_config)
    })
    .await;
    TestClient::new(srv, vars::VERSION)
  }
}
