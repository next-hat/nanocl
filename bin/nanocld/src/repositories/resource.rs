use std::collections::HashMap;

use diesel::prelude::*;

use jsonschema::{Draft, Validator};
use nanocl_error::{
  http::{HttpError, HttpResult},
  http_client::{HttpClientError, HttpClientResult},
  io::{IoError, IoResult},
};

use nanocl_stubs::{
  generic::GenericFilter,
  resource::{Resource, ResourcePartial, ResourceSpec},
  resource_kind::ResourceKind,
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{
    ColumnType, Pool, ResourceDb, ResourceKindDb, ResourceUpdateDb, SpecDb,
  },
  schema::{resources, specs},
  utils,
};

use super::generic::*;

// Only a controller's typed HTTP 404 confirms an unknown rule. An IO NotFound
// (for example a missing controller socket) must retain the normal retry path.
fn controller_delete_result(
  result: HttpClientResult<()>,
  allow_missing: bool,
) -> HttpResult<()> {
  match result {
    Err(HttpClientError::HttpError(error))
      if allow_missing && error.status == ntex::http::StatusCode::NOT_FOUND =>
    {
      Ok(())
    }
    result => result.map_err(Into::into),
  }
}

impl RepositoryBase for ResourceDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Text, "resources.key")),
      (
        "created_at",
        (ColumnType::Timestamptz, "resources.created_at"),
      ),
      ("kind", (ColumnType::Text, "resources.kind")),
      ("spec_key", (ColumnType::Text, "resources.spec_key")),
      ("data", (ColumnType::Json, "specs.data")),
      ("metadata", (ColumnType::Json, "specs.metadata")),
    ])
  }
}

impl RepositoryCreate for ResourceDb {}

impl RepositoryUpdate for ResourceDb {
  type UpdateItem = ResourceUpdateDb;
}

impl RepositoryDelByPk for ResourceDb {}

impl RepositoryReadBy for ResourceDb {
  type Output = (ResourceDb, SpecDb);

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::PgConnection,
    Self::Output,
  > {
    let mut query = resources::table
      .inner_join(crate::schema::specs::table)
      .into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(resources::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for ResourceDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let mut query = resources::table
      .inner_join(crate::schema::specs::table)
      .into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}

impl RepositoryReadByTransform for ResourceDb {
  type NewOutput = Resource;

  fn transform(input: (ResourceDb, SpecDb)) -> IoResult<Self::NewOutput> {
    let item = input.0.with_spec(&input.1);
    Ok(item)
  }
}

impl WithSpec for ResourceDb {
  type Output = Resource;
  type Relation = SpecDb;

  fn with_spec(self, r: &Self::Relation) -> Self::Output {
    Self::Output {
      created_at: self.created_at,
      kind: self.kind,
      spec: r.clone().into(),
    }
  }
}

impl ResourceDb {
  fn history_query(
    key: &str,
    limit: Option<i64>,
    offset: Option<i64>,
  ) -> specs::BoxedQuery<'_, diesel::pg::Pg> {
    let mut query = specs::table
      .filter(specs::kind_name.eq("Resource"))
      .filter(specs::kind_key.eq(key))
      .order((specs::created_at.desc(), specs::key.desc()))
      .into_boxed();
    if let Some(limit) = limit {
      query = query.limit(limit);
    }
    if let Some(offset) = offset {
      query = query.offset(offset);
    }
    query
  }

  pub(crate) async fn history(
    key: &str,
    limit: Option<i64>,
    offset: Option<i64>,
    pool: &Pool,
  ) -> IoResult<Vec<ResourceSpec>> {
    let key = key.to_owned();
    super::operation::run_query_owned(pool, (), move |conn| {
      Ok(
        Self::history_query(&key, limit, offset)
          .load::<SpecDb>(conn)
          .map_err(Self::map_err)?
          .into_iter()
          .map(ResourceSpec::from)
          .collect(),
      )
    })
    .await
  }

  pub(crate) async fn history_spec(
    key: &str,
    revision: uuid::Uuid,
    pool: &Pool,
  ) -> IoResult<SpecDb> {
    let key = key.to_owned();
    super::operation::run_query_owned(pool, (), move |conn| {
      Self::history_query(&key, None, None)
        .filter(specs::key.eq(revision))
        .first::<SpecDb>(conn)
        .optional()
        .map_err(Self::map_err)?
        .ok_or_else(|| {
          IoError::not_found(
            "Resource history",
            "Revision does not belong to the selected resource",
          )
        })
    })
    .await
  }

  pub async fn parse_kind(
    kind: &str,
    pool: &Pool,
  ) -> IoResult<(String, String)> {
    let items = kind.split('/').collect::<Vec<_>>();
    match items.get(2) {
      Some(version) => {
        Ok((items[..2].join("/"), version.to_owned().to_string()))
      }
      None => {
        let kind = ResourceKindDb::transform_read_by_pk(kind, pool).await?;
        Ok((kind.name, kind.version))
      }
    }
  }

  /// Create a new resource from a spec.
  pub async fn create_from_spec(
    item: &ResourcePartial,
    pool: &Pool,
  ) -> IoResult<Resource> {
    let (kind, version) = ResourceDb::parse_kind(&item.kind, pool).await?;
    let spec = SpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      kind_name: "Resource".to_owned(),
      incarnation_key: None,
      kind_key: item.name.to_owned(),
      version: version.to_owned(),
      data: item.data.clone(),
      metadata: item.metadata.clone(),
    };
    let new_item = ResourceDb {
      key: item.name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      kind,
      spec_key: spec.key.to_owned(),
    };
    super::operation::run_query_owned(pool, (), move |conn| {
      super::operation::transaction(conn, |conn| {
        let spec = diesel::insert_into(specs::table)
          .values(&spec)
          .get_result::<SpecDb>(conn)?;
        let resource = diesel::insert_into(resources::table)
          .values(&new_item)
          .get_result::<ResourceDb>(conn)?;
        Ok(resource.with_spec(&spec))
      })
    })
    .await
  }

  /// Update a resource from a spec.
  pub async fn update_from_spec(
    item: &ResourcePartial,
    pool: &Pool,
  ) -> IoResult<Resource> {
    let key = item.name.clone();
    let resource = ResourceDb::transform_read_by_pk(&item.name, pool).await?;
    let (_, version) = ResourceDb::parse_kind(&item.kind, pool).await?;
    let spec = SpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      kind_name: "Resource".to_owned(),
      incarnation_key: None,
      kind_key: resource.spec.resource_key,
      version: version.clone(),
      data: item.data.clone(),
      metadata: item.metadata.clone(),
    };
    let resource_update = ResourceUpdateDb {
      key: None,
      spec_key: Some(spec.key.to_owned()),
    };
    super::operation::run_query_owned(pool, (), move |conn| {
      super::operation::transaction(conn, |conn| {
        let spec = diesel::insert_into(specs::table)
          .values(&spec)
          .get_result::<SpecDb>(conn)?;
        let resource = diesel::update(resources::table.find(&key))
          .set(&resource_update)
          .get_result::<ResourceDb>(conn)?;
        Ok(resource.with_spec(&spec))
      })
    })
    .await
  }

  pub(crate) async fn delete_with_history(
    key: &str,
    pool: &Pool,
  ) -> IoResult<()> {
    let key = key.to_owned();
    super::operation::run_query_owned(pool, (), move |conn| {
      super::operation::transaction(conn, |conn| {
        diesel::delete(resources::table.find(&key)).execute(conn)?;
        diesel::delete(
          specs::table
            .filter(specs::kind_name.eq("Resource"))
            .filter(specs::kind_key.eq(&key)),
        )
        .execute(conn)?;
        Ok(())
      })
    })
    .await
  }

  /// This hook is called when a resource is created.
  /// It call a custom controller at a specific url or just validate a schema.
  /// If the resource is a Kind Kind, it will create a resource Kind with an associated version.
  /// To call a custom controller, the resource Kind must have a Url field in his config.
  /// Unless it must have a Schema field in his config that is a Validator to validate the resource.
  pub async fn hook_create(
    resource: &ResourcePartial,
    pool: &Pool,
  ) -> HttpResult<ResourcePartial> {
    let mut resource = resource.clone();
    let (kind, version) = ResourceDb::parse_kind(&resource.kind, pool).await?;
    log::trace!("hook_create_resource kind: {kind} {version}");
    let kind: ResourceKind = SpecDb::get_version(&kind, &version, pool)
      .await?
      .try_into()?;
    if let Some(schema) = &kind.data.schema {
      let schema: Validator = Validator::options()
        .with_draft(Draft::Draft7)
        .build(schema)
        .map_err(|err| {
          HttpError::bad_request(format!("Invalid schema {}", err))
        })?;
      schema.validate(&resource.data).map_err(|err| {
        HttpError::bad_request(format!("Invalid schema {err}"))
      })?;
    }
    if let Some(url) = &kind.data.url {
      let ctrl_client =
        utils::ctrl_client::CtrlClient::new(&kind.name, url).await?;
      let config = ctrl_client
        .apply_rule(&version, &resource.name, &resource.data)
        .await?;
      resource.data = config;
    }
    Ok(resource)
  }

  /// This hook is called when a resource is deleted.
  /// It call a custom controller at a specific url.
  /// If the resource is a Kind Kind, it will delete the resource Kind with an associated version.
  pub async fn hook_delete(resource: &Resource, pool: &Pool) -> HttpResult<()> {
    Self::delete_hook(resource, pool, false).await
  }

  /// Delete a persisted Resource even when its kind/version or controller rule
  /// is already absent. Compensation stays strict: a controller may return 404
  /// because its lookup of an uncommitted canonical Resource failed.
  pub(crate) async fn hook_delete_existing(
    resource: &Resource,
    pool: &Pool,
  ) -> HttpResult<()> {
    Self::delete_hook(resource, pool, true).await
  }

  async fn delete_hook(
    resource: &Resource,
    pool: &Pool,
    allow_missing: bool,
  ) -> HttpResult<()> {
    let (kind, version) = match Self::parse_kind(&resource.kind, pool).await {
      Ok(kind) => kind,
      Err(error)
        if allow_missing
          && error.inner.kind() == std::io::ErrorKind::NotFound =>
      {
        log::warn!(
          "Deleting Resource {} with missing kind {}; external controller cleanup cannot be verified",
          resource.spec.resource_key,
          resource.kind,
        );
        return Ok(());
      }
      Err(error) => return Err(error.into()),
    };
    let kind: ResourceKind = match SpecDb::get_version(&kind, &version, pool)
      .await
    {
      Ok(spec) => spec.try_into()?,
      Err(error)
        if allow_missing
          && error.inner.kind() == std::io::ErrorKind::NotFound =>
      {
        log::warn!(
          "Deleting Resource {} with a missing version of kind {}; external controller cleanup cannot be verified",
          resource.spec.resource_key,
          resource.kind,
        );
        return Ok(());
      }
      Err(error) => return Err(error.into()),
    };
    log::debug!("hook_delete_resource kind: {kind:?}");
    if let Some(url) = &kind.data.url {
      let ctrl_client =
        utils::ctrl_client::CtrlClient::new(&kind.name, url).await?;
      controller_delete_result(
        ctrl_client
          .delete_rule(&resource.spec.version, &resource.spec.resource_key)
          .await,
        allow_missing,
      )?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn resource_delete_accepts_only_typed_controller_absence() {
    use ntex::http::StatusCode;

    for allow_missing in [false, true] {
      assert!(controller_delete_result(Ok(()), allow_missing).is_ok());
      for status in [
        StatusCode::NOT_FOUND,
        StatusCode::BAD_REQUEST,
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::CONFLICT,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::SERVICE_UNAVAILABLE,
      ] {
        let result = controller_delete_result(
          Err(HttpClientError::HttpError(HttpError::new(
            status, "fixture",
          ))),
          allow_missing,
        );
        assert_eq!(
          result.is_ok(),
          allow_missing && status == StatusCode::NOT_FOUND
        );
      }
      for kind in [
        std::io::ErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied,
        std::io::ErrorKind::ConnectionRefused,
        std::io::ErrorKind::TimedOut,
      ] {
        let result = controller_delete_result(
          Err(HttpClientError::IoError(IoError::with_context(
            "controller",
            kind.into(),
          ))),
          allow_missing,
        );
        assert!(
          result.is_err(),
          "IO {kind:?} must not confirm controller deletion"
        );
      }
    }
  }

  #[test]
  fn resource_history_query_paginates_before_loading_with_scoped_order() {
    let query = ResourceDb::history_query("shared-name", Some(100), Some(20));
    let sql = diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string();
    assert!(sql.contains("\"specs\".\"kind_name\" = $1"), "{sql}");
    assert!(sql.contains("\"specs\".\"kind_key\" = $2"), "{sql}");
    assert!(
      sql.contains(
        "ORDER BY \"specs\".\"created_at\" DESC, \"specs\".\"key\" DESC"
      ),
      "{sql}"
    );
    assert!(sql.contains("LIMIT $3 OFFSET $4"), "{sql}");
    assert!(
      sql.contains("[\"Resource\", \"shared-name\", 100, 20]"),
      "{sql}"
    );
  }

  #[test]
  fn resource_history_query_scopes_kind_and_name_without_truncation() {
    let query = ResourceDb::history_query("shared-name", None, None);
    let sql = diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string();
    assert!(sql.contains("\"specs\".\"kind_name\" = $1"), "{sql}");
    assert!(sql.contains("\"specs\".\"kind_key\" = $2"), "{sql}");
    assert!(
      sql.contains(
        "ORDER BY \"specs\".\"created_at\" DESC, \"specs\".\"key\" DESC"
      ),
      "{sql}"
    );
    assert!(sql.contains("[\"Resource\", \"shared-name\"]"), "{sql}");
    assert!(!sql.contains("LIMIT"), "{sql}");

    let revision = uuid::Uuid::from_u128(1);
    let query = ResourceDb::history_query("shared-name", None, None)
      .filter(specs::key.eq(revision));
    let sql = diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string();
    assert!(sql.contains("\"specs\".\"kind_name\" = $1"), "{sql}");
    assert!(sql.contains("\"specs\".\"kind_key\" = $2"), "{sql}");
    assert!(sql.contains("\"specs\".\"key\" = $3"), "{sql}");
    assert!(sql.contains(&revision.to_string()), "{sql}");
  }
}
