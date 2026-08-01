use std::{error::Error, fmt, str::FromStr};

/// Canonical identity of a namespaced resource.
///
/// Nanocl resource keys use the `{namespace}.{name}` order.
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
    let value = format!("{namespace}.{name}");
    Ok(Self {
      separator: namespace.len(),
      value,
    })
  }

  /// Return the local resource name.
  pub fn name(&self) -> &str {
    &self.value[self.separator + 1..]
  }

  /// Return the resource namespace.
  pub fn namespace(&self) -> &str {
    &self.value[..self.separator]
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
    let Some((namespace, name)) = value.rsplit_once('.') else {
      return Err(ResourceKeyError::new(
        "resource key must use the `{namespace}.{name}` format",
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
  if let Some(unsafe_char) = value.chars().find(|ch| {
    ch.is_whitespace()
      || ch.is_control()
      || matches!(ch, '/' | '?' | '#' | '%' | '\\')
  }) {
    return Err(ResourceKeyError::new(format!(
      "resource {component} contains URL-unsafe character {unsafe_char:?}"
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
  fn formats_in_namespace_name_order() {
    let key = ResourceKey::new("ncproxy", "system").unwrap();
    assert_eq!(key.as_str(), "system.ncproxy");
    assert_eq!(key.name(), "ncproxy");
    assert_eq!(key.namespace(), "system");
  }

  #[test]
  fn parses_namespaces_containing_separator() {
    let key = "team.production.website".parse::<ResourceKey>().unwrap();
    assert_eq!(key.name(), "website");
    assert_eq!(key.namespace(), "team.production");
  }

  #[test]
  fn rejects_resource_names_containing_separator() {
    assert!(ResourceKey::new("public.api", "global").is_err());
  }

  #[test]
  fn rejects_url_unsafe_characters() {
    for unsafe_char in ['/', '?', '#', '%', '\\', ' '] {
      let name = format!("web{unsafe_char}site");
      let namespace = format!("team{unsafe_char}production");
      assert!(ResourceKey::new(&name, "global").is_err());
      assert!(ResourceKey::new("website", &namespace).is_err());
      assert!(format!("global.{name}").parse::<ResourceKey>().is_err());
      assert!(
        format!("{namespace}.website")
          .parse::<ResourceKey>()
          .is_err()
      );
    }
  }

  #[test]
  fn rejects_invalid_keys_without_panicking() {
    for key in ["", "website", ".website", "global.", " global.website"] {
      assert!(
        key.parse::<ResourceKey>().is_err(),
        "{key:?} must be invalid"
      );
    }
  }
}
