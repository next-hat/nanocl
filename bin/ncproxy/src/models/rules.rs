use std::collections::{BTreeMap, BTreeSet};

use nanocld_client::stubs::proxy::ResourceProxyRule;

/// Declarations accepted by this controller process, before nanocld publishes
/// their resource revision. Reconnects resolve these declarations again.
#[derive(Default)]
pub(crate) struct AcceptedRules {
  pub(crate) initialized: bool,
  pub(crate) generation: u64,
  rules: BTreeMap<String, ResourceProxyRule>,
  removed_before_init: BTreeSet<String>,
}

impl AcceptedRules {
  pub(crate) fn apply(&mut self, name: String, rule: ResourceProxyRule) {
    self.removed_before_init.remove(&name);
    self.rules.insert(name, rule);
    self.generation = self.generation.wrapping_add(1);
  }

  pub(crate) fn remove(&mut self, name: &str) {
    self.rules.remove(name);
    if !self.initialized {
      self.removed_before_init.insert(name.to_owned());
    }
    self.generation = self.generation.wrapping_add(1);
  }

  pub(crate) fn initialize(&mut self, rules: Vec<(String, ResourceProxyRule)>) {
    if self.initialized {
      return;
    }
    for (name, rule) in rules {
      if !self.removed_before_init.contains(&name) {
        self.rules.entry(name).or_insert(rule);
      }
    }
    self.removed_before_init.clear();
    self.initialized = true;
    self.generation = self.generation.wrapping_add(1);
  }

  pub(crate) fn snapshot(&self) -> Vec<(String, ResourceProxyRule)> {
    self
      .rules
      .iter()
      .map(|(name, rule)| (name.clone(), rule.clone()))
      .collect()
  }

  /// Event queries can still observe the revision preceding a callback.
  pub(crate) fn resolve(
    &self,
    name: &str,
    fallback: &ResourceProxyRule,
  ) -> Option<ResourceProxyRule> {
    self.rules.get(name).cloned().or_else(|| {
      (!self.initialized && !self.removed_before_init.contains(name))
        .then(|| fallback.clone())
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn rule(port: u16) -> ResourceProxyRule {
    serde_json::from_value(serde_json::json!({
      "Rules": [{
        "Domain": "recovery.test",
        "Network": "All",
        "Port": port,
        "Locations": [{
          "Path": "/",
          "Target": { "Url": "http://127.0.0.1:9000" }
        }]
      }]
    }))
    .unwrap()
  }

  fn assert_snapshot(
    actual: &AcceptedRules,
    expected: Vec<(String, ResourceProxyRule)>,
  ) {
    let actual = actual.snapshot();
    assert_eq!(actual.len(), expected.len());
    for ((name, rule), (expected_name, expected_rule)) in
      actual.into_iter().zip(expected)
    {
      assert_eq!(name, expected_name);
      assert_eq!(
        serde_json::to_value(rule).unwrap(),
        serde_json::to_value(expected_rule).unwrap()
      );
    }
  }

  #[test]
  fn bootstrap_preserves_accepted_puts_and_deletes() {
    let mut rules = AcceptedRules::default();
    rules.apply("updated".into(), rule(8081));
    rules.remove("removed");
    assert_eq!(
      serde_json::to_value(rules.resolve("updated", &rule(8080)).unwrap())
        .unwrap(),
      serde_json::to_value(rule(8081)).unwrap()
    );
    assert!(rules.resolve("removed", &rule(8080)).is_none());
    rules.initialize(vec![
      ("updated".into(), rule(8080)),
      ("removed".into(), rule(8080)),
      ("existing".into(), rule(8080)),
    ]);
    assert_snapshot(
      &rules,
      vec![
        ("existing".into(), rule(8080)),
        ("updated".into(), rule(8081)),
      ],
    );
    // Reconnecting cannot replay a database snapshot predating a callback.
    rules.initialize(vec![("updated".into(), rule(8080))]);
    assert_eq!(
      serde_json::to_value(&rules.snapshot()[1].1).unwrap(),
      serde_json::to_value(rule(8081)).unwrap()
    );
  }

  #[test]
  fn later_callbacks_replace_bootstrap_tombstones_and_declarations() {
    let mut rules = AcceptedRules::default();
    rules.remove("restored");
    rules.apply("restored".into(), rule(8081));
    rules.initialize(vec![("restored".into(), rule(8080))]);
    assert_snapshot(&rules, vec![("restored".into(), rule(8081))]);
    rules.remove("restored");
    assert!(rules.snapshot().is_empty());
    assert!(rules.resolve("restored", &rule(8081)).is_none());
    // A publication-failure compensation callback is accepted normally.
    rules.apply("restored".into(), rule(8080));
    assert_snapshot(&rules, vec![("restored".into(), rule(8080))]);
  }

  #[test]
  fn callbacks_and_compensation_invalidate_prior_recovery_snapshots() {
    let mut rules = AcceptedRules::default();
    rules.initialize(vec![("rule".into(), rule(8080))]);
    let initial = rules.generation;
    rules.apply("rule".into(), rule(8081));
    assert_ne!(rules.generation, initial);
    let updated = rules.generation;
    rules.apply("rule".into(), rule(8080));
    assert_ne!(rules.generation, updated);
    let compensated = rules.generation;
    rules.remove("rule");
    assert_ne!(rules.generation, compensated);
    assert!(rules.snapshot().is_empty());
  }
}
