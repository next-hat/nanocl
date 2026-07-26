use std::{error::Error, fmt, str::FromStr};

/// Canonical identity of a namespaced resource.
///
/// Nanocl resource keys keep the existing `{name}.{namespace}` order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ResourceKey {
  value: String,
  separator: usize,
}

impl ResourceKey {
  /// Build a canonical resource key from its components.
  pub fn new(
    name: impl AsRef<str>,
    namespace: impl AsRef<str>,
  ) -> Result<Self, ResourceKeyError> {
    let name = name.as_ref();
    let namespace = namespace.as_ref();
    validate_component("name", name)?;
    validate_component("namespace", namespace)?;
    if name.contains('.') {
      return Err(ResourceKeyError::new(
        "resource name cannot contain the key separator '.'",
      ));
    }
    let value = format!("{name}.{namespace}");
    Ok(Self {
      separator: name.len(),
      value,
    })
  }

  /// Return the local resource name.
  pub fn name(&self) -> &str {
    &self.value[..self.separator]
  }

  /// Return the resource namespace.
  pub fn namespace(&self) -> &str {
    &self.value[self.separator + 1..]
  }

  /// Return the canonical key as a string slice.
  pub fn as_str(&self) -> &str {
    &self.value
  }
}

impl fmt::Display for ResourceKey {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for ResourceKey {
  type Err = ResourceKeyError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    if value.trim() != value {
      return Err(ResourceKeyError::new(
        "resource key cannot contain surrounding whitespace",
      ));
    }
    let Some((name, namespace)) = value.split_once('.') else {
      return Err(ResourceKeyError::new(
        "resource key must use the `{name}.{namespace}` format",
      ));
    };
    Self::new(name, namespace)
  }
}

fn validate_component(
  component: &'static str,
  value: &str,
) -> Result<(), ResourceKeyError> {
  if value.is_empty() {
    return Err(ResourceKeyError::new(format!(
      "resource {component} cannot be empty"
    )));
  }
  if value.trim() != value {
    return Err(ResourceKeyError::new(format!(
      "resource {component} cannot contain surrounding whitespace"
    )));
  }
  Ok(())
}

/// Error returned when a namespaced resource key is invalid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceKeyError {
  message: String,
}

impl ResourceKeyError {
  fn new(message: impl Into<String>) -> Self {
    Self {
      message: message.into(),
    }
  }
}

impl fmt::Display for ResourceKeyError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.message)
  }
}

impl Error for ResourceKeyError {}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn formats_in_existing_name_namespace_order() {
    let key = ResourceKey::new("ncproxy", "system").unwrap();
    assert_eq!(key.as_str(), "ncproxy.system");
    assert_eq!(key.name(), "ncproxy");
    assert_eq!(key.namespace(), "system");
  }

  #[test]
  fn parses_namespaces_containing_separator() {
    let key = "website.team.production".parse::<ResourceKey>().unwrap();
    assert_eq!(key.name(), "website");
    assert_eq!(key.namespace(), "team.production");
  }

  #[test]
  fn rejects_invalid_keys_without_panicking() {
    for key in ["", "website", ".global", "website.", " website.global"] {
      assert!(
        key.parse::<ResourceKey>().is_err(),
        "{key:?} must be invalid"
      );
    }
  }
}
