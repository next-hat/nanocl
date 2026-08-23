use std::{
  collections::{BTreeMap, HashMap, HashSet},
  net::IpAddr,
  time::{Duration, Instant},
};

use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::NanocldClient;
use nanocld_client::stubs::network::{Network, gen_network_key};
use nanocld_client::stubs::{
  dns::ResourceDnsRule,
  generic::{GenericClause, GenericFilter, NetworkKind},
};
use ntex::rt;

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

fn rule_selectors(
  rules: &BTreeMap<String, ResourceDnsRule>,
  cache: &HashMap<NetworkKind, IpAddr>,
) -> Vec<NetworkKind> {
  let mut selectors = HashSet::new();
  for rule in rules.values() {
    if !cache.contains_key(&rule.network) {
      selectors.insert(rule.network.clone());
    }
    for entry in &rule.entries {
      if !cache.contains_key(&entry.ip_address) {
        selectors.insert(entry.ip_address.clone());
      }
    }
  }
  selectors.into_iter().collect()
}

async fn resolve_selectors(
  selectors: Vec<NetworkKind>,
  cache: &mut HashMap<NetworkKind, IpAddr>,
  client: &NanocldClient,
) -> IoResult<()> {
  for selector in selectors {
    log::debug!("dns::reconcile: resolving selector {selector}");
    resolve_cached(&selector, cache, client).await?;
    log::debug!("dns::reconcile: resolved selector {selector}");
  }
  Ok(())
}

fn build_scopes(
  rules: &BTreeMap<String, ResourceDnsRule>,
  cache: &HashMap<NetworkKind, IpAddr>,
) -> IoResult<Vec<DnsmasqScope>> {
  let mut scopes = BTreeMap::<IpAddr, DnsmasqScope>::new();
  for rule in rules.values() {
    let listen_address = *cache.get(&rule.network).ok_or_else(|| {
      IoError::invalid_data(
        "Network",
        &format!("Network selector {} was not resolved", rule.network),
      )
    })?;
    let scope = scopes
      .entry(listen_address)
      .or_insert_with(|| DnsmasqScope::new(listen_address));
    for entry in &rule.entries {
      let address = *cache.get(&entry.ip_address).ok_or_else(|| {
        IoError::invalid_data(
          "Network",
          &format!("Network selector {} was not resolved", entry.ip_address),
        )
      })?;
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
    let committed =
      overrides
        .get(key)
        .is_some_and(|pending| match &pending.desired {
          Some(desired) => persisted.get(key) == Some(desired),
          None => !persisted.contains_key(key),
        });
    if committed {
      overrides.remove(key);
    }
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

fn effective_rules(
  persisted: &BTreeMap<String, ResourceDnsRule>,
  mut overrides: BTreeMap<String, PendingDnsRule>,
  acknowledged_key: Option<&str>,
  now: Instant,
) -> (
  BTreeMap<String, PendingDnsRule>,
  BTreeMap<String, ResourceDnsRule>,
) {
  retain_uncommitted_overrides(
    persisted,
    &mut overrides,
    acknowledged_key,
    now,
  );
  let mut effective = persisted.clone();
  apply_overrides(&mut effective, &overrides);
  (overrides, effective)
}

async fn list_committed_rules(
  client: &NanocldClient,
) -> IoResult<BTreeMap<String, ResourceDnsRule>> {
  let filter = GenericFilter::new()
    .limit(10_000)
    .r#where("kind", GenericClause::Eq(vars::RULE_KEY.to_owned()));
  let resources = client.list_resource(Some(&filter)).await.map_err(|err| {
    err.map_err_context(|| "Unable to list ncdns resources from nanocld")
  })?;
  resources
    .into_iter()
    .map(|resource| {
      let key = resource.spec.resource_key;
      let rule = serde_json::from_value(resource.spec.data).map_err(|err| {
        err.map_err_context(|| {
          format!("Unable to deserialize ncdns resource {key}")
        })
      })?;
      Ok((key, rule))
    })
    .collect()
}

async fn reconcile_committed_snapshot(
  persisted: BTreeMap<String, ResourceDnsRule>,
  acknowledged_key: Option<&str>,
  dnsmasq: &Dnsmasq,
  client: &NanocldClient,
) -> IoResult<()> {
  let mut cache = HashMap::new();
  loop {
    let mut update = dnsmasq.lock_updates().await;
    let (overrides, effective) = effective_rules(
      &persisted,
      update.overrides.clone(),
      acknowledged_key,
      Instant::now(),
    );
    let missing = rule_selectors(&effective, &cache);
    if !missing.is_empty() {
      drop(update);
      resolve_selectors(missing, &mut cache, client).await?;
      continue;
    }

    let scopes = build_scopes(&effective, &cache)?;
    dnsmasq.apply_scopes(scopes).await?;
    update.persisted = persisted;
    update.overrides = overrides;
    return Ok(());
  }
}

/// Refresh committed DNS rules outside resource-controller transactions and
/// atomically reconcile them with any still-pending hook overrides.
pub(crate) async fn refresh_committed_rules(
  acknowledged_key: Option<&str>,
  dnsmasq: &Dnsmasq,
  client: &NanocldClient,
) -> IoResult<()> {
  let persisted = list_committed_rules(client).await?;
  reconcile_committed_snapshot(persisted, acknowledged_key, dnsmasq, client)
    .await
}

/// Rebuild all isolated DNS instances from persisted rules, optionally
/// replacing or removing the rule currently being handled by nanocld.
pub(crate) async fn reconcile_entries(
  current_key: Option<&str>,
  replacement: Option<&ResourceDnsRule>,
  dnsmasq: &Dnsmasq,
  client: &NanocldClient,
) -> IoResult<()> {
  let expires_at =
    reconcile_entries_inner(current_key, replacement, dnsmasq, client).await?;
  if let Some(expires_at) = expires_at {
    let dnsmasq = dnsmasq.clone();
    let client = client.clone();
    rt::spawn(async move {
      ntex::time::sleep(expires_at.saturating_duration_since(Instant::now()))
        .await;
      if let Err(err) = refresh_committed_rules(None, &dnsmasq, &client).await {
        log::warn!("dns::reconcile: override expiry failed: {err}");
      }
    });
  }
  Ok(())
}

async fn reconcile_entries_inner(
  current_key: Option<&str>,
  replacement: Option<&ResourceDnsRule>,
  dnsmasq: &Dnsmasq,
  client: &NanocldClient,
) -> IoResult<Option<Instant>> {
  if current_key.is_none() && replacement.is_some() {
    return Err(IoError::invalid_input(
      "DnsRule",
      "a replacement requires a resource key",
    ));
  }
  // Resource hooks run inside nanocld's resource transaction. They must use the
  // last committed in-memory snapshot rather than calling list_resource, which
  // waits for the transaction that is itself waiting for this hook.
  let mut cache = HashMap::new();
  loop {
    log::debug!("dns::reconcile: waiting for update lock");
    let mut update = dnsmasq.lock_updates().await;
    log::debug!("dns::reconcile: acquired update lock");
    let rules = update.persisted.clone();
    let now = Instant::now();
    let mut overrides = update.overrides.clone();
    retain_uncommitted_overrides(&rules, &mut overrides, None, now);
    let mut expires_at = None;
    if let Some(key) = current_key {
      let expiry = now + OVERRIDE_TTL;
      overrides.insert(
        key.to_owned(),
        PendingDnsRule {
          desired: replacement.cloned(),
          expires_at: expiry,
        },
      );
      expires_at = Some(expiry);
    }
    let mut effective_rules = rules.clone();
    apply_overrides(&mut effective_rules, &overrides);

    let missing = rule_selectors(&effective_rules, &cache);
    if !missing.is_empty() {
      log::debug!(
        "dns::reconcile: resolving {} selectors outside update lock",
        missing.len()
      );
      drop(update);
      resolve_selectors(missing, &mut cache, client).await?;
      continue;
    }

    let scopes = build_scopes(&effective_rules, &cache)?;
    log::debug!("dns::reconcile: applying {} scopes", scopes.len());
    dnsmasq.apply_scopes(scopes).await?;
    log::debug!("dns::reconcile: applied scopes");
    update.overrides = overrides;
    return Ok(expires_at);
  }
}

#[cfg(test)]
pub mod tests {
  use std::collections::{BTreeMap, HashMap};
  use std::time::{Duration, Instant};

  pub use nanocl_utils::ntex::test_client::*;
  use nanocld_client::stubs::{
    dns::{DnsEntry, ResourceDnsRule},
    generic::NetworkKind,
  };
  use nanocld_client::{ConnectOpts, NanocldClient};
  use ntex::rt;

  use super::{build_scopes, effective_rules};
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

  fn rule_with_entry(
    network: &str,
    name: &str,
    address: &str,
  ) -> ResourceDnsRule {
    ResourceDnsRule {
      network: NetworkKind::Named(network.to_owned()),
      entries: vec![DnsEntry {
        name: name.to_owned(),
        ip_address: NetworkKind::Other(address.parse().unwrap()),
      }],
    }
  }

  fn pending(
    desired: Option<ResourceDnsRule>,
    expires_at: Instant,
  ) -> PendingDnsRule {
    PendingDnsRule {
      desired,
      expires_at,
    }
  }

  #[test]
  fn persisted_rule_a_alone_produces_a() {
    let now = Instant::now();
    let persisted = BTreeMap::from([("a".to_owned(), rule("network-a"))]);

    let (_, effective) =
      effective_rules(&persisted, BTreeMap::new(), None, now);

    assert_eq!(effective, persisted);
  }

  #[test]
  fn persisted_rules_a_and_b_both_produce_entries() {
    let now = Instant::now();
    let persisted = BTreeMap::from([
      ("a".to_owned(), rule("network-a")),
      ("b".to_owned(), rule("network-b")),
    ]);

    let (_, effective) =
      effective_rules(&persisted, BTreeMap::new(), None, now);

    assert_eq!(effective, persisted);
  }

  #[test]
  fn pending_rule_b_is_added_without_removing_persisted_a() {
    let now = Instant::now();
    let persisted = BTreeMap::from([("a".to_owned(), rule("network-a"))]);
    let overrides = BTreeMap::from([(
      "b".to_owned(),
      pending(Some(rule("network-b")), now + Duration::from_secs(10)),
    )]);

    let (_, effective) = effective_rules(&persisted, overrides, None, now);

    assert_eq!(effective.get("a"), Some(&rule("network-a")));
    assert_eq!(effective.get("b"), Some(&rule("network-b")));
  }

  #[test]
  fn committing_rule_b_preserves_persisted_a() {
    let now = Instant::now();
    let persisted = BTreeMap::from([
      ("a".to_owned(), rule("network-a")),
      ("b".to_owned(), rule("network-b")),
    ]);
    let overrides = BTreeMap::from([(
      "b".to_owned(),
      pending(Some(rule("network-b")), now + Duration::from_secs(10)),
    )]);

    let (overrides, effective) =
      effective_rules(&persisted, overrides, Some("b"), now);

    assert_eq!(effective, persisted);
    assert!(!overrides.contains_key("b"));
  }

  #[test]
  fn updating_committed_b_preserves_a_and_replaces_only_b() {
    let now = Instant::now();
    let persisted = BTreeMap::from([
      ("a".to_owned(), rule("network-a")),
      ("b".to_owned(), rule("network-b-new")),
    ]);
    let overrides = BTreeMap::from([(
      "b".to_owned(),
      pending(Some(rule("network-b-new")), now + Duration::from_secs(10)),
    )]);

    let (_, effective) = effective_rules(&persisted, overrides, Some("b"), now);

    assert_eq!(effective.get("a"), Some(&rule("network-a")));
    assert_eq!(effective.get("b"), Some(&rule("network-b-new")));
  }

  #[test]
  fn deleting_committed_b_preserves_a() {
    let now = Instant::now();
    let persisted = BTreeMap::from([("a".to_owned(), rule("network-a"))]);
    let overrides = BTreeMap::from([(
      "b".to_owned(),
      pending(None, now + Duration::from_secs(10)),
    )]);

    let (overrides, effective) =
      effective_rules(&persisted, overrides, Some("b"), now);

    assert_eq!(effective, persisted);
    assert!(!overrides.contains_key("b"));
  }

  #[test]
  fn pending_b_shadows_only_persisted_b_before_commit() {
    let now = Instant::now();
    let persisted = BTreeMap::from([
      ("a".to_owned(), rule("network-a")),
      ("b".to_owned(), rule("network-b-old")),
    ]);
    let overrides = BTreeMap::from([(
      "b".to_owned(),
      pending(
        Some(rule("network-b-pending")),
        now + Duration::from_secs(10),
      ),
    )]);

    let (_, effective) = effective_rules(&persisted, overrides, None, now);

    assert_eq!(effective.get("a"), Some(&rule("network-a")));
    assert_eq!(effective.get("b"), Some(&rule("network-b-pending")));
  }

  #[test]
  fn committed_refresh_for_b_acknowledges_b_override() {
    let now = Instant::now();
    let persisted = BTreeMap::from([("b".to_owned(), rule("committed-b"))]);
    let overrides = BTreeMap::from([(
      "b".to_owned(),
      pending(Some(rule("committed-b")), now + Duration::from_secs(10)),
    )]);

    let (overrides, effective) =
      effective_rules(&persisted, overrides, Some("b"), now);

    assert!(overrides.is_empty());
    assert_eq!(effective, persisted);
  }

  #[test]
  fn stale_committed_event_does_not_acknowledge_newer_b_override() {
    let now = Instant::now();
    let persisted = BTreeMap::from([("b".to_owned(), rule("older-b"))]);
    let overrides = BTreeMap::from([(
      "b".to_owned(),
      pending(Some(rule("newer-b")), now + Duration::from_secs(10)),
    )]);

    let (overrides, effective) =
      effective_rules(&persisted, overrides, Some("b"), now);

    assert!(overrides.contains_key("b"));
    assert_eq!(effective.get("b"), Some(&rule("newer-b")));
  }

  #[test]
  fn committing_b_does_not_acknowledge_unrelated_override_a() {
    let now = Instant::now();
    let persisted = BTreeMap::from([
      ("a".to_owned(), rule("committed-a")),
      ("b".to_owned(), rule("committed-b")),
    ]);
    let overrides = BTreeMap::from([(
      "a".to_owned(),
      pending(Some(rule("pending-a")), now + Duration::from_secs(10)),
    )]);

    let (overrides, effective) =
      effective_rules(&persisted, overrides, Some("b"), now);

    assert!(overrides.contains_key("a"));
    assert_eq!(effective.get("a"), Some(&rule("pending-a")));
    assert_eq!(effective.get("b"), Some(&rule("committed-b")));
  }

  #[test]
  fn expired_override_falls_back_to_complete_persisted_state() {
    let now = Instant::now();
    let persisted = BTreeMap::from([
      ("a".to_owned(), rule("network-a")),
      ("b".to_owned(), rule("network-b")),
    ]);
    let overrides =
      BTreeMap::from([("b".to_owned(), pending(Some(rule("stale-b")), now))]);

    let (overrides, effective) =
      effective_rules(&persisted, overrides, None, now);

    assert!(overrides.is_empty());
    assert_eq!(effective, persisted);
  }

  #[test]
  fn rules_for_the_same_listener_are_merged_into_one_scope() {
    let rules = BTreeMap::from([
      (
        "a".to_owned(),
        rule_with_entry("shared", "a.internal", "10.0.0.2"),
      ),
      (
        "b".to_owned(),
        rule_with_entry("shared", "b.internal", "10.0.0.3"),
      ),
    ]);
    let cache = HashMap::from([
      (
        NetworkKind::Named("shared".to_owned()),
        "10.0.0.1".parse().unwrap(),
      ),
      (
        NetworkKind::Other("10.0.0.2".parse().unwrap()),
        "10.0.0.2".parse().unwrap(),
      ),
      (
        NetworkKind::Other("10.0.0.3".parse().unwrap()),
        "10.0.0.3".parse().unwrap(),
      ),
    ]);

    let scopes = build_scopes(&rules, &cache).unwrap();

    assert_eq!(scopes.len(), 1);
    assert!(scopes[0].entries.contains_key("a.internal"));
    assert!(scopes[0].entries.contains_key("b.internal"));
  }

  #[test]
  fn reconnect_full_refresh_replaces_the_committed_snapshot() {
    let now = Instant::now();
    let initial = BTreeMap::from([
      ("a".to_owned(), rule("network-a")),
      ("deleted".to_owned(), rule("network-deleted")),
    ]);
    let refreshed = BTreeMap::from([
      ("a".to_owned(), rule("network-a")),
      ("c".to_owned(), rule("network-c")),
    ]);

    let (_, initial_effective) =
      effective_rules(&initial, BTreeMap::new(), None, now);
    let (_, refreshed_effective) =
      effective_rules(&refreshed, BTreeMap::new(), None, now);

    assert!(initial_effective.contains_key("deleted"));
    assert!(!refreshed_effective.contains_key("deleted"));
    assert_eq!(refreshed_effective, refreshed);
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
