use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoResult, FromIo, IoError};

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::job::{Job, JobPartial, JobUpdate};

use crate::utils;
use crate::models::{Pool, JobDbModel, JobUpdateDbModel};

pub async fn create(item: &JobPartial, pool: &Pool) -> IoResult<Job> {
  let dbmodel = JobDbModel {
    key: item.name.clone(),
    created_at: chrono::Local::now().naive_local(),
    updated_at: chrono::Local::now().naive_local(),
    data: serde_json::to_value(item)
      .map_err(|err| err.map_err_context(|| "Job"))?,
    metadata: item.metadata.clone(),
  };
  super::generic::insert_with_res::<_, _, JobDbModel>(dbmodel, pool).await?;
  let job = Job {
    name: item.name.clone(),
    secrets: item.secrets.clone(),
    metadata: item.metadata.clone(),
    containers: item.containers.clone(),
  };
  Ok(job)
}

pub async fn delete_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::jobs;
  let name = name.to_owned();
  super::generic::delete_by_id::<jobs::table, _>(name, pool).await
}

pub async fn update_by_name(
  name: &str,
  item: &JobUpdate,
  pool: &Pool,
) -> IoResult<JobDbModel> {
  use crate::schema::jobs;
  let name = name.to_owned();
  let dbmodel = JobUpdateDbModel {
    key: item.name.clone(),
    created_at: None,
    updated_at: Some(chrono::Local::now().naive_local()),
    data: Some(
      serde_json::to_value(item)
        .map_err(|err| err.map_err_context(|| "Job"))?,
    ),
    metadata: item.metadata.clone(),
  };
  let dbmodel =
    super::generic::update_by_id_with_res::<jobs::table, _, _, JobDbModel>(
      name, dbmodel, pool,
    )
    .await?;
  Ok(dbmodel)
}

pub async fn list(pool: &Pool) -> IoResult<Vec<JobDbModel>> {
  use crate::schema::jobs;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = jobs::dsl::jobs
      .order(jobs::dsl::created_at.desc())
      .get_results(&mut conn)
      .map_err(|err| err.map_err_context(|| "Cargo"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}

pub async fn find_by_name(name: &str, pool: &Pool) -> IoResult<JobDbModel> {
  use crate::schema::jobs;
  let name = name.to_owned();
  super::generic::find_by_id::<jobs::table, _, _>(name, pool).await
}
