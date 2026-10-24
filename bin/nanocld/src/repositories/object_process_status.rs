use std::collections::HashMap;

use diesel::prelude::*;

use nanocl_error::io::IoResult;
use nanocl_stubs::{
  generic::GenericFilter,
  system::{ObjPsHealthStatusKind, ObjPsStatusKind},
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{
    ColumnType, ObjPsStatusDb, ObjPsStatusUpdate, OperationOwner, Pool,
  },
  schema::object_process_statuses,
};

use super::generic::*;

impl RepositoryBase for ObjPsStatusDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Text, "object_process_statuses.key")),
      (
        "wanted",
        (ColumnType::Text, "object_process_statuses.wanted"),
      ),
      (
        "prev_wanted",
        (ColumnType::Text, "object_process_statuses.prev_wanted"),
      ),
      (
        "actual",
        (ColumnType::Text, "object_process_statuses.actual"),
      ),
      (
        "prev_actual",
        (ColumnType::Text, "object_process_statuses.prev_actual"),
      ),
      (
        "health",
        (ColumnType::Text, "object_process_statuses.health"),
      ),
      (
        "created_at",
        (
          ColumnType::Timestamptz,
          "object_process_statuses.created_at",
        ),
      ),
    ])
  }
}

impl RepositoryCreate for ObjPsStatusDb {}

impl RepositoryDelByPk for ObjPsStatusDb {}

impl RepositoryUpdate for ObjPsStatusDb {
  type UpdateItem = ObjPsStatusUpdate;
}

impl RepositoryReadBy for ObjPsStatusDb {
  type Output = ObjPsStatusDb;

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::pg::PgConnection,
    Self::Output,
  > {
    let mut query = object_process_statuses::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(object_process_statuses::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl ObjPsStatusDb {
  pub(crate) async fn update_owned<G: Send + 'static>(
    key: &str,
    update: ObjPsStatusUpdate,
    owner: Option<OperationOwner>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<Self> {
    if owner
      .as_ref()
      .is_some_and(|owner| owner.resource_key != key)
    {
      return Err(
        super::operation::conflict(
          "Status does not belong to its operation owner",
        )
        .into(),
      );
    }
    let key = key.to_owned();
    super::operation::run_mutation_owned(pool, keepalive, owner, move |conn| {
      Ok(
        diesel::update(object_process_statuses::table.find(&key))
          .set(&update)
          .get_result(conn)?,
      )
    })
    .await
  }

  pub(crate) async fn update_actual_status_owned<G: Send + 'static>(
    key: &str,
    status: &ObjPsStatusKind,
    owner: Option<OperationOwner>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<()> {
    if owner
      .as_ref()
      .is_some_and(|owner| owner.resource_key != key)
    {
      return Err(
        super::operation::conflict(
          "Status does not belong to its operation owner",
        )
        .into(),
      );
    }
    let key = key.to_owned();
    let status = status.to_string();
    super::operation::run_mutation_owned(pool, keepalive, owner, move |conn| {
      let current = object_process_statuses::table
        .find(&key)
        .first::<Self>(conn)?;
      if current.actual != status {
        diesel::update(object_process_statuses::table.find(&key))
          .set(ObjPsStatusUpdate {
            actual: Some(status.clone()),
            prev_actual: Some(current.actual),
            ..Default::default()
          })
          .execute(conn)?;
      }
      Ok(())
    })
    .await
  }

  pub async fn update_actual_status(
    key: &str,
    status: &ObjPsStatusKind,
    pool: &Pool,
  ) -> IoResult<()> {
    let curr_status = ObjPsStatusDb::read_by_pk(&key, pool).await?;
    let status = status.to_string();
    if curr_status.actual == status {
      return Ok(());
    }
    let new_status = ObjPsStatusUpdate {
      actual: Some(status),
      prev_actual: Some(curr_status.actual),
      ..Default::default()
    };
    ObjPsStatusDb::update_pk(key, new_status, pool).await?;
    Ok(())
  }

  pub async fn update_health_status(
    key: &str,
    health: &ObjPsHealthStatusKind,
    pool: &Pool,
  ) -> IoResult<bool> {
    let curr_status = ObjPsStatusDb::read_by_pk(key, pool).await?;
    let health = health.to_string();
    if curr_status.health == health {
      return Ok(false);
    }
    let new_status = ObjPsStatusUpdate {
      health: Some(health),
      ..Default::default()
    };
    ObjPsStatusDb::update_pk(key, new_status, pool).await?;
    Ok(true)
  }
}
