use nanocl_stubs::resource::ResourceSpec;

use super::SpecDb;

/// Helper to convert a `SpecDb` to a `ResourceSpec`
impl From<SpecDb> for ResourceSpec {
  fn from(db: SpecDb) -> Self {
    ResourceSpec {
      key: db.key,
      version: db.version,
      created_at: db.created_at,
      resource_key: db.kind_key,
      data: db.data,
      metadata: db.metadata,
    }
  }
}
