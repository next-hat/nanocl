use nanocld_client::stubs::{cargo_spec::CargoSpecRevision, vm_spec::VmSpec};
use tabled::Tabled;

/// Shared table columns for immutable Cargo and VM specification revisions.
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct SpecHistoryRow {
  pub id: String,
  pub version: String,
  #[tabled(rename = "CREATED AT")]
  pub created_at: String,
}

impl From<CargoSpecRevision> for SpecHistoryRow {
  fn from(spec: CargoSpecRevision) -> Self {
    Self {
      id: spec.key.to_string(),
      version: spec.version,
      created_at: spec
        .created_at
        .and_utc()
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string(),
    }
  }
}

impl From<VmSpec> for SpecHistoryRow {
  fn from(spec: VmSpec) -> Self {
    Self {
      id: spec.key.to_string(),
      version: spec.version,
      created_at: spec
        .created_at
        .and_utc()
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn spec_history_rows_preserve_revision_identity_version_and_local_time() {
    let key = uuid::Uuid::from_u128(1);
    let created_at = chrono::DateTime::UNIX_EPOCH.naive_utc();
    let cargo: CargoSpecRevision = serde_json::from_value(serde_json::json!({
      "Key": key, "CargoKey": "global.backup", "Version": "v0.19",
      "CreatedAt": created_at, "Name": "backup", "Containers": [],
    }))
    .unwrap();
    let vm = VmSpec {
      key,
      vm_key: "global.backup".into(),
      version: "v0.19".into(),
      created_at,
      name: "backup".into(),
      ..Default::default()
    };
    let expected_time = created_at
      .and_utc()
      .with_timezone(&chrono::Local)
      .format("%Y-%m-%d %H:%M:%S")
      .to_string();
    for row in [SpecHistoryRow::from(cargo), SpecHistoryRow::from(vm)] {
      assert_eq!(row.id, key.to_string());
      assert_eq!(row.version, "v0.19");
      assert_eq!(row.created_at, expected_time);
    }
    assert_eq!(SpecHistoryRow::headers(), ["ID", "VERSION", "CREATED AT"]);
  }
}
