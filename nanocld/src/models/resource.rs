use diesel::prelude::*;

use nanocl_models::resource::ResourceKind;
use serde::{Deserialize, Serialize};

use crate::schema::resources;

#[derive(
  Debug, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(key))]
#[diesel(table_name = resources)]
pub struct ResourceDbModel {
  pub(crate) key: String,
  pub(crate) kind: ResourceKind,
  pub(crate) config_key: uuid::Uuid,
}

#[derive(AsChangeset)]
#[diesel(table_name = resources)]
pub struct ResourceUpdateModel {
  pub(crate) key: Option<String>,
  pub(crate) config_key: Option<uuid::Uuid>,
}
