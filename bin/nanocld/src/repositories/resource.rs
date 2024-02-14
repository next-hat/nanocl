use diesel::prelude::*;

use jsonschema::{Draft, JSONSchema};
use nanocl_error::{
  io::IoResult,
  http::{HttpError, HttpResult},
};

use nanocl_stubs::{
  generic::GenericFilter,
  resource::{Resource, ResourcePartial},
  resource_kind::ResourceKind,
};

use crate::{
  utils,
  schema::{specs, resources},
  gen_multiple, gen_where4json, gen_where4string,
  models::{Pool, ResourceDb, ResourceKindDb, ResourceUpdateDb, SpecDb},
};

use super::generic::*;

impl RepositoryBase for ResourceDb {}

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
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resources::table
      .inner_join(crate::schema::specs::table)
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, resources::key, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, resources::kind, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, specs::data, value);
    }
    if let Some(value) = r#where.get("metadata") {
      gen_where4json!(query, specs::metadata, value);
    }
    if is_multiple {
      gen_multiple!(query, resources::created_at, filter);
    }
    query
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
      kind_key: item.name.to_owned(),
      version: version.to_owned(),
      data: item.data.clone(),
      metadata: item.metadata.clone(),
    };
    let spec = SpecDb::create_from(spec, pool).await?;
    let new_item = ResourceDb {
      key: item.name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      kind,
      spec_key: spec.key.to_owned(),
    };
    let dbmodel = ResourceDb::create_from(new_item, pool).await?;
    let item = dbmodel.with_spec(&spec);
    Ok(item)
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
      kind_key: resource.spec.resource_key,
      version: version.clone(),
      data: item.data.clone(),
      metadata: item.metadata.clone(),
    };
    let spec = SpecDb::create_from(spec, pool).await?;
    let resource_update = ResourceUpdateDb {
      key: None,
      spec_key: Some(spec.key.to_owned()),
    };
    let dbmodel = ResourceDb::update_pk(&key, resource_update, pool).await?;
    let item = dbmodel.with_spec(&spec);
    Ok(item)
  }

  /// This hook is called when a resource is created.
  /// It call a custom controller at a specific url or just validate a schema.
  /// If the resource is a Kind Kind, it will create a resource Kind with an associated version.
  /// To call a custom controller, the resource Kind must have a Url field in his config.
  /// Unless it must have a Schema field in his config that is a JSONSchema to validate the resource.
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
      let schema: JSONSchema = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(schema)
        .map_err(|err| {
          HttpError::bad_request(format!("Invalid schema {}", err))
        })?;
      schema.validate(&resource.data).map_err(|err| {
        let mut msg = String::from("Invalid config ");
        for error in err {
          msg += &format!("{} ", error);
        }
        HttpError::bad_request(msg)
      })?;
    }
    if let Some(url) = &kind.data.url {
      let ctrl_client = utils::ctrl_client::CtrlClient::new(&kind.name, url);
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
    let (kind, version) = ResourceDb::parse_kind(&resource.kind, pool).await?;
    let kind: ResourceKind = SpecDb::get_version(&kind, &version, pool)
      .await?
      .try_into()?;
    log::debug!("hook_delete_resource kind: {kind:?}");
    if let Some(url) = &kind.data.url {
      let ctrl_client = utils::ctrl_client::CtrlClient::new(&kind.name, url);
      ctrl_client
        .delete_rule(&resource.spec.version, &resource.spec.resource_key)
        .await?;
    }
    Ok(())
  }
}
