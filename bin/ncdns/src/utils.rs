use std::{
  collections::{BTreeMap, HashMap},
  net::IpAddr,
  time::{Duration, Instant},
};

use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::NanocldClient;
use nanocld_client::stubs::dns::ResourceDnsRule;
use nanocld_client::stubs::generic::{
  GenericClause, GenericFilter, NetworkKind,
};
use nanocld_client::stubs::network::{Network, gen_network_key};

use crate::{
  dnsmasq::{Dnsmasq, DnsmasqScope, PendingDnsRule},
  vars,
};

const OVERRIDE_TTL: Duration = Duration::from_secs(10);

/// Get public address of host
async fn get_host_addr(client: &NanocldClient) -> IoResult<String> {
  let info = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get host info"))?;
  Ok(info.host_gateway)
}

async fn get_local_node(client: &NanocldClient) -> IoResult<String> {
  if let Ok(node) = std::env::var("NANOCL_NODE")
    && !node.trim().is_empty()
  {
    return Ok(node);
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

async fn build_scopes(
  rules: &[ResourceDnsRule],
  client: &NanocldClient,
) -> IoResult<Vec<DnsmasqScope>> {
  let mut cache = HashMap::new();
  let mut scopes = BTreeMap::<IpAddr, DnsmasqScope>::new();
  for rule in rules {
    let listen_address =
      resolve_cached(&rule.network, &mut cache, client).await?;
    let scope = scopes
      .entry(listen_address)
      .or_insert_with(|| DnsmasqScope::new(listen_address));
    for entry in &rule.entries {
      let address =
        resolve_cached(&entry.ip_address, &mut cache, client).await?;
      scope.add_entry(entry.name.clone(), address);
    }
  }
  Ok(scopes.into_values().collect())
}

fn retain_uncommitted_overrides(
  persisted: &BTreeMap<String, ResourceDnsRule>,
  overrides: &mut BTreeMap<String, PendingDnsRule>,
  acknowledged_key: Option<&str>,
  now: Instant,
) {
  if let Some(key) = acknowledged_key {
    overrides.remove(key);
  }
  overrides.retain(|key, pending| {
    if pending.expires_at <= now {
      return false;
    }
    match &pending.desired {
      Some(desired) => persisted.get(key) != Some(desired),
      None => persisted.contains_key(key),
    }
  });
}

fn apply_overrides(
  persisted: &mut BTreeMap<String, ResourceDnsRule>,
  overrides: &BTreeMap<String, PendingDnsRule>,
) {
  for (key, pending) in overrides {
    persisted.remove(key);
    if let Some(desired) = &pending.desired {
      persisted.insert(key.clone(), desired.clone());
    }
  }
}

/// Rebuild all isolated DNS instances from persisted rules, optionally
/// replacing or removing the rule currently being handled by nanocld.
pub(crate) async fn reconcile_entries(
  current_key: Option<&str>,
  replacement: Option<&ResourceDnsRule>,
  dnsmasq: &Dnsmasq,
  client: &NanocldClient,
) -> IoResult<()> {
  reconcile_entries_inner(current_key, replacement, None, dnsmasq, client).await
}

/// Rebuild from the committed nanocld snapshot. A resource event acknowledges
/// the matching pre-commit override; periodic calls expire any event that was
/// missed so stale desired state cannot survive indefinitely.
pub(crate) async fn reconcile_persisted_entries(
  acknowledged_key: Option<&str>,
  dnsmasq: &Dnsmasq,
  client: &NanocldClient,
) -> IoResult<()> {
  reconcile_entries_inner(None, None, acknowledged_key, dnsmasq, client).await
}

async fn reconcile_entries_inner(
  current_key: Option<&str>,
  replacement: Option<&ResourceDnsRule>,
  acknowledged_key: Option<&str>,
  dnsmasq: &Dnsmasq,
  client: &NanocldClient,
) -> IoResult<()> {
  if current_key.is_none() && replacement.is_some() {
    return Err(IoError::invalid_input(
      "DnsRule",
      "a replacement requires a resource key",
    ));
  }
  // Keep this guard across the nanocld snapshot and the filesystem apply.
  // Resource hooks run before nanocld commits, so confirmed/pending overrides
  // below also carry concurrent successful hooks across that commit window.
  let mut update = dnsmasq.lock_updates().await;
  let filter = GenericFilter::new()
    .limit(10_000)
    .r#where("kind", GenericClause::Eq(vars::RULE_KEY.to_owned()));
  let resources = client.list_resource(Some(&filter)).await.map_err(|err| {
    err.map_err_context(|| "Unable to list DNS resources from nanocld")
  })?;
  let mut rules = BTreeMap::new();
  for resource in resources {
    let key = resource.spec.resource_key;
    let rule = serde_json::from_value::<ResourceDnsRule>(resource.spec.data)
      .map_err(|err| err.map_err_context(|| "Unable to deserialize DnsRule"))?;
    rules.insert(key, rule);
  }

  // Once nanocld reflects a pending hook, its override is no longer needed.
  retain_uncommitted_overrides(
    &rules,
    &mut update.overrides,
    acknowledged_key,
    Instant::now(),
  );

  let previous = current_key.map(|key| {
    (
      key.to_owned(),
      update.overrides.insert(
        key.to_owned(),
        PendingDnsRule {
          desired: replacement.cloned(),
          expires_at: Instant::now() + OVERRIDE_TTL,
        },
      ),
    )
  });
  apply_overrides(&mut rules, &update.overrides);

  let result = async {
    let rules = rules.into_values().collect::<Vec<_>>();
    let scopes = build_scopes(&rules, client).await?;
    dnsmasq.apply_scopes(scopes).await
  }
  .await;
  if result.is_err()
    && let Some((key, previous)) = previous
  {
    match previous {
      Some(previous) => {
        update.overrides.insert(key, previous);
      }
      None => {
        update.overrides.remove(&key);
      }
    }
  }
  result
}

#[cfg(test)]
pub mod tests {
  use std::collections::BTreeMap;
  use std::time::{Duration, Instant};

  pub use nanocl_utils::ntex::test_client::*;
  use nanocld_client::stubs::{dns::ResourceDnsRule, generic::NetworkKind};
  use nanocld_client::{ConnectOpts, NanocldClient};
  use ntex::rt;

  use super::{apply_overrides, retain_uncommitted_overrides};
  use crate::{
    dnsmasq::{self, PendingDnsRule},
    services, vars,
  };

  fn rule(network: &str) -> ResourceDnsRule {
    ResourceDnsRule {
      network: NetworkKind::Named(network.to_owned()),
      entries: Vec::new(),
    }
  }

  #[test]
  fn pending_rule_overrides_close_the_pre_commit_window() {
    let now = Instant::now();
    let mut persisted = BTreeMap::from([
      ("updated".to_owned(), rule("old")),
      ("deleted".to_owned(), rule("private")),
    ]);
    let overrides = BTreeMap::from([
      (
        "created".to_owned(),
        PendingDnsRule {
          desired: Some(rule("private")),
          expires_at: now + Duration::from_secs(10),
        },
      ),
      (
        "updated".to_owned(),
        PendingDnsRule {
          desired: Some(rule("new")),
          expires_at: now + Duration::from_secs(10),
        },
      ),
      (
        "deleted".to_owned(),
        PendingDnsRule {
          desired: None,
          expires_at: now + Duration::from_secs(10),
        },
      ),
    ]);

    apply_overrides(&mut persisted, &overrides);

    assert_eq!(persisted.get("created"), Some(&rule("private")));
    assert_eq!(persisted.get("updated"), Some(&rule("new")));
    assert!(!persisted.contains_key("deleted"));
  }

  #[test]
  fn committed_rule_overrides_are_discarded() {
    let now = Instant::now();
    let persisted = BTreeMap::from([("created".to_owned(), rule("private"))]);
    let mut overrides = BTreeMap::from([
      (
        "created".to_owned(),
        PendingDnsRule {
          desired: Some(rule("private")),
          expires_at: now + Duration::from_secs(10),
        },
      ),
      (
        "deleted".to_owned(),
        PendingDnsRule {
          desired: None,
          expires_at: now + Duration::from_secs(10),
        },
      ),
      (
        "pending".to_owned(),
        PendingDnsRule {
          desired: Some(rule("pending")),
          expires_at: now + Duration::from_secs(10),
        },
      ),
    ]);

    retain_uncommitted_overrides(&persisted, &mut overrides, None, now);

    assert!(!overrides.contains_key("created"));
    assert!(!overrides.contains_key("deleted"));
    assert!(overrides.contains_key("pending"));
  }

  #[test]
  fn acknowledged_and_expired_overrides_yield_to_persisted_state() {
    let now = Instant::now();
    let persisted = BTreeMap::from([
      ("acknowledged".to_owned(), rule("committed")),
      ("expired".to_owned(), rule("committed")),
    ]);
    let mut overrides = BTreeMap::from([
      (
        "acknowledged".to_owned(),
        PendingDnsRule {
          desired: Some(rule("different")),
          expires_at: now + Duration::from_secs(10),
        },
      ),
      (
        "expired".to_owned(),
        PendingDnsRule {
          desired: Some(rule("different")),
          expires_at: now,
        },
      ),
    ]);

    retain_uncommitted_overrides(
      &persisted,
      &mut overrides,
      Some("acknowledged"),
      now,
    );

    assert!(overrides.is_empty());
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
