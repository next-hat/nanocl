/// Utils to manipulate `key` property of a model
/// The key property is based on the `namespace` and the `name` of the given model
/// or on the key of the parent for relational purpose.
/// For example if we create a cargo `get-started` in the default namespace `global`
/// The cargo key will be `get-started.global`
use rand::{RngExt, distr::Alphanumeric, rng};

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::{process::ProcessKind, resource_key::ResourceKey};

/// Resolve a namespace for an operation that creates a resource.
///
/// Creation retains the `global` default for omitted, empty, or whitespace-only
/// namespace values.
pub fn resolve_nsp(nsp: &Option<String>) -> String {
  normalize_nsp(nsp).unwrap_or("global").to_owned()
}

/// Normalize an optional namespace used to filter a collection.
///
/// Omitted, empty, and whitespace-only values all mean that no namespace
/// filter should be applied.
pub fn normalize_nsp(nsp: &Option<String>) -> Option<&str> {
  nsp.as_deref().map(str::trim).filter(|nsp| !nsp.is_empty())
}

/// Parse and validate a canonical namespaced resource key.
pub fn parse_resource_key(key: &str) -> IoResult<ResourceKey> {
  key
    .parse::<ResourceKey>()
    .map_err(|err| IoError::invalid_input("Resource key", &err.to_string()))
}

/// Generate the stable key of a node-scoped Docker network.
pub fn gen_network_key(node_name: &str, network_name: &str) -> String {
  nanocl_stubs::network::gen_network_key(node_name, network_name)
}

/// Generate a short id based on the length
pub fn generate_short_id(length: usize) -> String {
  let rng = rng();
  let short_id: String = rng
    .sample_iter(&Alphanumeric)
    .take(length)
    .map(char::from)
    .collect();
  short_id
}

pub fn ensure_kind(kind: &str) -> IoResult<()> {
  if kind.split('/').collect::<Vec<_>>().len() != 2 {
    return Err(IoError::invalid_input(
      "Kind",
      "must be of the form `domain.tld/kind`",
    ));
  }
  Ok(())
}

pub fn validate_kind_key(kind: &ProcessKind, key: &str) -> IoResult<()> {
  match kind {
    ProcessKind::Job => Ok(()),
    ProcessKind::Cargo | ProcessKind::Vm => {
      parse_resource_key(key)?;
      Ok(())
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn namespace_filter_omits_blank_values() {
    assert_eq!(normalize_nsp(&None), None);
    assert_eq!(normalize_nsp(&Some(String::new())), None);
    assert_eq!(normalize_nsp(&Some(" \t ".to_owned())), None);
    assert_eq!(normalize_nsp(&Some(" system ".to_owned())), Some("system"));
  }

  #[test]
  fn create_namespace_keeps_global_default() {
    assert_eq!(resolve_nsp(&None), "global");
    assert_eq!(resolve_nsp(&Some("  ".to_owned())), "global");
    assert_eq!(resolve_nsp(&Some("system".to_owned())), "system");
  }

  #[test]
  fn network_key_contains_node_and_network_name() {
    assert_eq!(
      gen_network_key("node-a.nanocl.io", "private.api"),
      "node-a.nanocl.io.private.api"
    );
  }
}
