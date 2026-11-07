use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use futures::lock::Mutex;
use nanocld_client::{NanocldClient, stubs::dns::ResourceDnsRule};

use crate::dnsmasq::Dnsmasq;

/// Application state shared by the DNS controller handlers.
#[derive(Clone)]
pub struct SystemState {
  pub dnsmasq: Dnsmasq,
  pub client: NanocldClient,
  pub(crate) rules: Arc<Mutex<DnsRuleState>>,
}

impl SystemState {
  pub(crate) fn new(dnsmasq: Dnsmasq, client: NanocldClient) -> Self {
    Self {
      dnsmasq,
      client,
      rules: Arc::new(Mutex::new(DnsRuleState::default())),
    }
  }
}

/// Declarations accepted by this controller process. Callback precedence during
/// bootstrap does not provide a transaction across controller process crashes.
#[derive(Default)]
pub(crate) struct DnsRuleState {
  pub(crate) declarations: BTreeMap<String, ResourceDnsRule>,
  pub(crate) bootstrapped: bool,
  touched: BTreeSet<String>,
}

impl DnsRuleState {
  pub(crate) fn applied(&mut self, key: String, rule: ResourceDnsRule) {
    if !self.bootstrapped {
      self.touched.insert(key.clone());
    }
    self.declarations.insert(key, rule);
  }

  pub(crate) fn removed(&mut self, key: &str) {
    if !self.bootstrapped {
      self.touched.insert(key.to_owned());
    }
    self.declarations.remove(key);
  }

  pub(crate) fn bootstrap(&mut self, rules: BTreeMap<String, ResourceDnsRule>) {
    if self.bootstrapped {
      return;
    }
    for (key, rule) in rules {
      if !self.touched.contains(&key) {
        self.declarations.insert(key, rule);
      }
    }
    self.bootstrapped = true;
    self.touched.clear();
  }
}

impl ntex::web::State for SystemState {
  type Error = ntex::web::DefaultError;
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocld_client::stubs::generic::NetworkKind;

  fn rule(address: &str) -> ResourceDnsRule {
    ResourceDnsRule {
      network: NetworkKind::Other(address.parse().unwrap()),
      entries: Vec::new(),
    }
  }

  #[test]
  fn bootstrap_preserves_accepted_put_and_delete_before_publication() {
    let mut state = DnsRuleState::default();
    let stale = rule("10.0.0.1");
    let accepted = rule("10.0.1.1");
    state.applied("updated".into(), accepted.clone());
    state.removed("deleted");
    state.bootstrap(BTreeMap::from([
      ("updated".into(), stale.clone()),
      ("deleted".into(), stale.clone()),
      ("untouched".into(), stale.clone()),
    ]));
    assert_eq!(state.declarations.get("updated"), Some(&accepted));
    assert!(!state.declarations.contains_key("deleted"));
    assert_eq!(state.declarations.get("untouched"), Some(&stale));
    assert!(state.touched.is_empty());
  }

  #[test]
  fn reconnect_and_compensation_keep_latest_accepted_declarations() {
    let mut state = DnsRuleState::default();
    let original = rule("10.0.0.1");
    state.bootstrap(BTreeMap::from([("rule".into(), original.clone())]));
    let replacement = rule("10.0.1.1");
    state.applied("rule".into(), replacement.clone());
    state.bootstrap(BTreeMap::from([("rule".into(), original.clone())]));
    assert_eq!(state.declarations.get("rule"), Some(&replacement));
    // A compensating callback is accepted just like a normal PUT.
    state.applied("rule".into(), original.clone());
    assert_eq!(state.declarations.get("rule"), Some(&original));
    state.removed("rule");
    state.bootstrap(BTreeMap::from([("rule".into(), original)]));
    assert!(state.declarations.is_empty());
    assert!(state.touched.is_empty());
  }
}
